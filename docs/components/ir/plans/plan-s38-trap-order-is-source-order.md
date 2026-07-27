# plan-s38 — trap order is source order (approach A, ratified)

Status: **PLANNED — approach ratified by Sapir, implementation deferred to a later session.**
Built once in S37, measured, and **reverted deliberately**: it works and the differential is green,
but it churns 19 goldens across three crates and reorders emission for programs that were never
rewritten. Landing it wants its own session with a perf re-run, not a tail-end of someone else's.
Component: `ir` (`topo_order`) · fallout in `backend-llvm`, `backend-cuda`, `rewrite`.
Origin: the S37 `open_inline` counterexample, pinned at `85b2243`.

---

## 1. The bug

A randomised proptest draw found `PassId::Inline` changing what a program does:

```
Trapped(IndexOob)  !≈  Trapped(DivZero)
```

Shrunk program: `Call(helper)`, `Iota`, `Index{arr, idx:178}` (out of bounds), `FoldArr` whose body
divides by zero. Reproduced exactly, with the two walks side by side:

```
BEFORE:  Call, Iota, Index, Fold      → Index traps first → IndexOob
AFTER:   Iota, Fold, Index            → Fold  traps first → DivZero
```

`Index` and `Fold` are **independent** — the dataflow graph imposes no order between them at all.
`topo_order` breaks that tie on **object insertion order** (`algo.rs`, "ties broken by the order
objects were discovered"). Inlining removes the `Call` object and adds the body's, insertion order
reshuffles, and the tie flips. The rewriter changed the program's observable behaviour, which is the
one thing it may never do (`eval ∘ rewrite = eval`).

Pre-existing on `main`; nothing in the S37 work caused it. The seed is committed so it cannot pass
on a lucky draw.

**Note the 1,280-run differential is structurally blind to this class.** `mapal_trap` exits 101
whatever the kind, so both programs are exit-101 with stdout ignored; only the interpreter-level
property suite can see a trap *kind* change. That makes the randomised property tests the sole guard
here, and is an argument for keeping their draws random rather than pinning them down.

## 2. Why the repo's stated invariant did not hold

R-ORDER, `backend-cuda/plans/plan-minimal-emission.md`:

> **R-ORDER (effect/trap order):** statement order of Named/guarded/effect points is today's topo
> order restricted to those points; inlining never migrates a trapping or effectful op across a
> statement boundary. **Oracle trap order preserved by construction.**

The construction is what fails. "Topo order restricted to trapping points" is only equal to
statement order while insertion order happens to match statement order — and rewriting is precisely
the thing that breaks that coincidence.

## 3. The decision

**Traps are observable, therefore trap order must be a function of the PROGRAM, not of the
schedule.** The dataflow graph does not order two independent trapping ops, so the tie must break on
something intrinsic. Source position is the only such thing available.

This is S29's clock-read fence generalised: there, "the dataflow graph orders pure work against a
clock read not at all", and source order was the tie-break with meaning. Same reasoning, wider scope.

### 3.1 Why A and not a separate selection key

The system has **three** places that must agree on which trap is reported:

| site | mechanism today |
| --- | --- |
| the oracle | first trap reached walking `topo_order` |
| LLVM sequential / host spine | `mapal_trap` fired at the site, in emission order |
| LLVM parallel | `record_trap` → `mapal_par_trap(topo, kind)`, runtime CAS-**min on topo** |

They agree today **because all three derive from `topo_order`**. S24's speculate-and-order protocol
is already record-and-select — keyed on the topo index.

So changing only the interpreter to select by source position (the "B" proposal, considered and
rejected in-session) is unsound: the oracle would report the source-minimum trap while the compiled
binary reports the topo-minimum one, and they diverge exactly when the two orders differ. Making B
sound means changing the key in every backend as well, which creates a standing new invariant —
"oracle key == backend key", maintained across llvm, cuda and verilog forever — and requires the
interpreter to evaluate past a trap on dummy values, importing S24's soundness argument into the
definition of correctness itself.

**A keeps one order in the system.** Make `topo_order` source-respecting and the oracle, emission
order, and the runtime's CAS key all inherit it. No second mechanism, nothing to keep in sync.

## 4. The change

**4a. `topo_order` breaks ties on source position.** The ready-worklist orders by
`(loc.start, loc.end, raw key id)` rather than discovery order; the raw id keeps it total so the walk
stays deterministic when a desugaring emits several morphisms from one span.

**4b. testgen stops stamping every statement position zero.** `const L = SourceLoc { start: 0, end: 0 }`
at all 122 sites made source order carry no information, so the tie-break degenerated straight back
to insertion order — this is why a first attempt at 4a appeared to do nothing. A monotonic counter
gives "position order == the order testgen emitted them", which is the statement order these programs
model. Not a weakening of the test: generating programs whose statements all claim position 0 and
then asserting a rewrite preserves their outcome asks for a guarantee the language does not make.

**Verified in S37: with 4a + 4b, the counterexample passes** (`BEFORE = AFTER = Trapped(IndexOob)`),
`inline` is 15/15 green including the pinned seed, and the **1,280-run differential is 36/36 green at
`-O0`/`-O2`** — every value byte-identical.

## 5. What it costs, measured

- **19 golden failures / 18 pending snapshots** across `mapal-rewrite`, `backend-llvm`,
  `backend-cuda`, plus one CUDA assertion.
- The CUDA assertion is benign but instructive: arena zone offsets move from
  `o2@0, o3@256, o4@512` to `o2@0, o4@256, o3@512`. Still disjoint, copies still correct — the test
  pins an *ordering assumption*, not a correctness property.
- **The reach is wider than the bug.** That CUDA test uses `emit_src` on a **raw, unrewritten**
  graph and its offsets still moved, which proves lowering does not create objects in source-position
  order. So un-rewritten programs get all of the churn and none of the benefit.
- Emission reordering across matmul/conv/fir is **unmeasured**. It must be measured before landing —
  interleaved, ≥50 runs on the sub-millisecond cells, matmul as the negative control.

### 5.1 A′ — the variant worth pricing first

Make **lowering** create objects in source order, so `loc` order equals today's order for raw graphs.
Then 4a moves only *rewritten* programs, which is exactly where the bug lives, and the golden churn
shrinks to the rewritten goldens. Unscoped: nobody has measured how far lowering deviates. Price it
before choosing between A and A′.

## 6. Two obligations this creates, whichever variant lands

1. **Inlining must stamp spliced morphisms with the call-site position.** A callee's morphisms carry
   the *callee's* locs, which can sort earlier than the call site, so a trap inside an inlined body
   can still move. The pinned counterexample has an **empty** helper and therefore does not exercise
   this — the class is not closed by 4a alone. Needs its own counterexample: a helper whose body can
   trap, inlined into a caller with an earlier-positioned trap.
2. **`SourceLoc` stops being debug metadata and becomes a semantic attribute.** Once trap order is
   defined by it, every rewrite owes it a discipline exactly as it owes value-preservation. Deserves
   an ADR; testgen's all-zeros was a symptom of nothing having said so.

## 7. Steps

1. **Price A′** (§5.1): how far does lowering deviate from source order on the example corpus? Cheap
   to answer — compare `loc` order against insertion order per function. Decides A vs A′.
2. Land 4a + 4b. Gate on: the pinned counterexample, `inline` green, 1,280-run differential green.
3. Review the ~19 goldens **one at a time**; for each, confirm the change is ordering/numbering and
   the node and edge multisets are unchanged.
4. Update the CUDA arena assertion with the reason, not just the new offsets.
5. **Measure**: full ladder plus matmul, interleaved, min and median, ≥50 runs on sub-ms cells,
   1t and par. Pre-register that nothing should move; a change means emission order is
   performance-relevant and that is its own finding.
6. Write the ADR for §6.2, and close §6.1 with its own counterexample and fix.
