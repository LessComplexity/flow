# ADR-0036: `scan` — the loop/fold middle class as a first-class Core op (candidate)

Date: 2026-07-25 · Status: **candidate — NOT decided; Sapir flagged the surrounding question open ("on loop-maps I need to think about it because not every loop is a map")** · number provisional · changes nothing until accepted. Related: ADR-0009 (collection-operator syntax), ADR-0018 (`zip`/`enumerate`), ADR-0028 (tree reduction / exact op folds), ADR-0027 (capture), S27b `LiftLoops` (R-LF/R-LM), `docs/notes/graph-advantage.md`, `docs/notes/2026-07-25-thesis-review.md`.

## Context (what forced the decision)

S27b shipped `PassId::LiftLoops`: loops whose bodies are recognizably map- or
fold-shaped are lifted (R-LM / R-LF) and then inline, tile and vectorize — _"even
loop naive implementations enjoy the perf boost automatically."_ `matmul4`'s loop
form now reaches the tiled emitter byte-exact; `fir`'s loop fold-lifts.

That success exposes the gap precisely. **Not every loop is a map or a fold, and
the residue is not small.** `examples/fib.flow` is the clean counter-example:

```flow
loop { (i < n) -> { -true-> { a + b -> t; b -> a; t -> b; i + 1 -> i; -> loop; }
                    -false-> a -> out; } }
```

Two carried values, each iteration's output feeding the next, no associative
combine to reassociate over. It is neither `map` (outputs are not independent)
nor `fold` (it is not "reduce an array to one value" — it is a state machine over
a counted range, and the _intermediate_ states are the point). It cannot lift,
so it stays opaque to every deduced query the ladder depends on: no `tile_plan`
site, no lane analysis, no reuse-as-fanout reading.

The general shape between `fold` and "arbitrary sequential loop" is **scan**
(prefix computation): `scan : (S × [A;n]) → [S;n]` — carry a state, emit every
intermediate. It is not an exotic case; it is a large fraction of real signal and
array work that Flow claims as its beachhead (VISION O2):

- prefix sums / cumulative products / running max — the textbook case;
- **IIR filters and recurrences** (the sibling of the FIR window rung S28 already
  won — FIR is a `map{fold}`, IIR is a `scan`, and today the second one falls off
  the ladder entirely);
- running normalizations, exponential moving averages, state machines over
  streams, `fib`-class iterations;
- the sequential half of attention-shaped work (online softmax is a scan).

And the payoff is not merely "it becomes a recognized site." **Scan has a known
parallel geometry** (Blelloch / Hillis–Steele work-efficient tree scan): a
declared `scan` over an associative combine is parallelizable in `O(log n)`
depth, on _every_ backend — the same "deduce the shape once, place it per `Loc`"
move as tiling (CPU: SIMD prefix + per-thread block scan + carry pass; CUDA: warp
shuffle scan then block scan — the standard primitive; FPGA: a pipelined chain,
which is its most natural form of all). Written as a `loop`, none of that is
available, because associativity is not recoverable from the graph. Written as
`scan` with a declared combine, it is.

This is the same argument as ADR-0028's tree reduction, one shape over: the
language must _state_ the algebraic property, because deducing associativity from
arbitrary arithmetic is not sound (and for floats it is not even true — hence
ADR-0032's reassociation contract, which is exactly the machinery this needs).

## Decision (recommended shape, if accepted)

**D1 — `scan` joins the realized Core op set** (an ADR-0018-class delta: one new
op, no Level-A change). `Scan { body } : (S × [A;n]) → [S;n]` — seed state, one
element per step, emit the state after each step. Surface follows the
collection-operator law (LC-2 / ADR-0009) verbatim, so it reads as the sibling of
`fold` it is:

```flow
(0, xs) -> scan { acc, x -> acc + x } -> prefix_sums;
```

Body rules are `fold`'s, unchanged: closed except for ADR-0027 read captures,
token-free, must produce a tail value, arity enforced.

**D2 — Sequential semantics is the oracle definition; parallel realization is
gated on a declared combine.** The interpreter defines `scan` left-to-right,
period — that is the meaning, and it makes `scan` immediately usable and
bit-exact everywhere with zero new theory. A backend may use the log-depth tree
realization **only** when the body is recognized associative _and_ the site's
precision contract admits reassociation (ADR-0032: `exact` forbids it for floats;
integer combines and the `contract`/`tf32-class` faces admit it). Recognition
reuses ADR-0028's machinery — the exact-op fold set — rather than inventing a
second associativity story. Default is therefore: correct sequentially
everywhere, fast in parallel where it is provably legal.

**D3 — `loop` stays, and its role is now stated.** With `map`/`fold`/`scan`
covering the deducible shapes, `loop` is the **escape hatch for genuinely
sequential, non-uniform iteration** — variable trip counts, early exit on a data
condition, state machines whose step is not uniform over a range. It is not
deprecated and should not be: E1's guarded trace is what gives Flow honest
partiality, and `LiftLoops` keeps rescuing the loops that _are_ secretly
map/fold/scan. What changes is the guidance: **`loop` is where you land when no
bulk op fits, and landing there means opting out of the ladder** — which should
be documented, since today it is an invisible performance cliff.

**D4 — `LiftLoops` gains an R-LS rule.** The lifter already recognizes map- and
fold-shaped loop bodies off `loop_plan` facts; scan-shaped bodies (carried state

- per-iteration store into an array indexed by the counter) are the third
  pattern, so naive loop-written prefix code reaches the ladder automatically —
  the same "naive implementations enjoy the boost" property S27b established.
  Every rejection pinned, as with R-LF/R-LM.

**D5 — Sequencing.** `scan` is small and self-contained: one IR op, one interp
arm, one lower builtin, backend arms that may start as the trivially-correct
sequential emission. It does **not** need coproducts, modules, or dynamic arrays.
It can therefore land inside the CPU phase without disturbing it, and it _adds_
to the ladder's evidence rather than competing with it — a fourth shape (after
matmul/FIR/conv2d) that the geometry story has to cover, and the first one whose
parallel form is a tree rather than a tiling.

## Consequences

- **Closes a real hole in the beachhead.** FIR won its table at S28; IIR — its
  direct sibling — currently cannot be expressed in a form the compiler can
  optimize at all. For a language positioning on signal/image work, that is the
  gap that would be noticed first.
- **Adds a second geometry family** (tree/prefix) beside the tiling family,
  which is a _strengthening_ of the genericity thesis: the claim becomes "the
  graph carries enough to deduce the right realization" rather than "the graph
  carries enough to tile."
- **Cost:** small for the sequential version, moderate for the parallel one, and
  the two are separable — ship D1+D3 first, D2's tree realization is its own rung.
- **Interacts with ADR-0032** as its first non-precision consumer: reassociation
  permission is what unlocks the parallel scan, so the contract lattice gets an
  immediate use beyond mma.
- **Interacts with ADR-0028**: same associativity recognizer, two consumers —
  which is the rule-of-three trigger for factoring it out properly.
- **Risk:** op-set creep. The counter-argument is that `scan` is not a
  convenience — it is the _only_ way to make an entire class of program visible
  to the optimizer, which is the same justification `map`/`fold` themselves have.

## Open questions

- **Q1** — Signature: `[S;n]` (states after each step, `n` outputs) or `[S;n+1]`
  (inclusive of the seed)? Exclusive/inclusive scan is a real API fork; `[S;n]`
  inclusive-of-step is recommended as the one that matches the loop it replaces.
- **Q2** — Does `scan` also want the tuple form `(S × [A;n]) → (S × [S;n])`
  returning the final state (so it subsumes `fold`)? That would make `fold` a
  projection of `scan` — categorically tidy, one more thing to lower.
- **Q3** — Is the associativity recognizer (ADR-0028) strong enough as-is, or
  does the body need a _declared_ combine (an annotation / a restricted body
  grammar) to be parallel-eligible?
- **Q4** — Sapir's open question upstream of this: given `LiftLoops`, is `loop`
  still wanted in the _surface_, or does it become IR-only with map/fold/scan as
  the sole surface forms? D3 recommends keeping it; the counter-case is that
  every surface `loop` is a cliff, and cliffs are better removed than documented.
  **This ADR does not decide that** — it only argues that removing `loop` is
  _safer_ once `scan` exists, because the residue that genuinely needs it shrinks.

## Spec impact

None until accepted. On acceptance: realized-op-set delta (ADR-0013 class, +1);
`flow-as-implemented.md` §2.6 gains `scan`; HANDOFF §4.1 collections line gains
it; user-guide §5 gains the form; the capability matrix gains a row.
