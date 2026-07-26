# Mapal — Spec Errata

This file records the accepted corrections to the v0.2 specification corpus. **"Frozen" is retired (ADR-0022 D1, ratified 2026-07-18):** the v0.2 documents are fixed text; all change flows through ADRs and is recorded here when it corrects spec text.

**Authority order (per HANDOFF §2.2, amended by ADR-0022 D1):** above all text, **oracle (interpreter) behavior is the final arbiter** (HANDOFF §5.4). Within text, highest wins:

1. Accepted ADRs in `docs/decisions/` (including the bootstrap ADRs encoding errata E1–E5)
2. `category-ir.md` v0.2 (formal semantics)
3. `user-guide.md` v0.2 and `architecture.md` v0.2 (tie: user-guide for language behavior, architecture for compiler structure)
4. `getting-started.md` v0.2
5. `CHANGES.md` (rationale, not normative)
6. `mapal-language-design.docx` (historical)

**Erratum → ADR mapping:**

- E1 → ADR-0002
- E2 → ADR-0003
- E3 → ADR-0004
- E4 → ADR-0005
- E5 → ADR-0006

The textual patches for E1–E5 were applied to the spec files on 2026-06-11 (Session 01). Affected sections in `category-ir.md` (§2.1/2.7/2.8/8.3) and `user-guide.md` (§3.6, §5.4, §6.5) carry an inline marker pointing back to this file and the corresponding ADR.

---

## E1 — Mapal-Cat cannot be both "total" and traced-cartesian

**Defect.** `category-ir.md` §2.1 defines morphisms as pure **total** functions; §2.8 claims Mapal-Cat is traced with monoidal product = categorical product. Jointly inconsistent: a traced cartesian category is equivalent to one with a Conway fixed-point operator (Hasegawa 1997), and total functions lack fixpoints in general (`not : Bool → Bool` has none). Unbounded loops are exactly where partiality enters (`loop { -> loop; }` is legal and diverges).

**Fix.**

- Loops/iteration live in the Kleisli category of the partiality (divergence) monad — the same §2.6 machinery already used for I/O and errors. The total core of Mapal-Cat has no trace; the traced structure exists on the partial extension (least-fixpoint / Elgot-iteration semantics).
- `category-ir.md` §8.3 must be rewritten: Clocked-Cat's trace is **guarded** (register = unit delay ⇒ always productive ⇒ total; Mealy-machine semantics). `F_Verilog` therefore maps an _iteration_ trace to a _guarded_ trace — different traced structures. "F commutes with Tr" is not free; it is a theorem with content, mediated by a **done-signal protocol**: _the iteration terminates in n steps with value v ⟺ the circuit asserts `done` at cycle n with output v_. This theorem is the project's most publishable single result; state it precisely, discharge it informally now, mechanize later.

**Implementation impact.** Interpreter loop semantics are partial: all loop evaluation carries a fuel/step-limit in tests; divergence is a defined outcome, not a hang. The Verilog FSM for any lowered loop implements the done protocol (`valid_in / busy / done / result` handshake).

## E2 — Parallel effects rule (replaces "executor decides")

**Defect.** `user-guide.md` §5.4 row 4: "Independent + effectful → Executor decides (may parallelize with non-deterministic order)." This makes program meaning scheduler-dependent, contradicting both "no data races by construction" and the functorial-correctness story (if the denotation is "whatever the executor did," there is nothing for a functor to preserve).

**Fix.** Effectful morphisms are **not permitted in parallel fanout**. Effects either (a) sequence via `seq`, or (b) communicate via channels with **Kahn process network semantics** — blocking reads, unbounded FIFOs — under which determinism independent of scheduling is a theorem (Kahn 1974). The streaming/FPGA subset later adopts synchronous-dataflow restrictions (Lee & Messerschmitt 1987) for static schedules and bounded buffers. Channels are out of Mapal-Core scope (§4), but the rule is fixed _now_ so the effect checker is built right the first time.

## E3 — Memory-model guarantee is scoped

**Defect.** "No use-after-free / double-free / leaks / races, with zero annotations" is claimed for the whole language. Whole-program region inference at general-purpose scope (closures, channels, cyclic structures) is historically treacherous (cf. Tofte–Talpin region pathologies); cycles already punt to refcounting.

**Fix.** State the guarantee as **proven for the first-order, non-cyclic dataflow core** (which contains Mapal-Core entirely) and **open for the full language**. `user-guide.md` §6.5 is amended accordingly. Implementation benefit: the Mapal-Core lifetime engine is simple (stack/static allocation for fixed-size data; last-use frontier for arrays) and can be exactly right.

## E4 — Operator-precedence example is self-contradictory

**Defect.** `user-guide.md` §3.6 table places `->` looser than `+`, but the example claims `a -> b + c -> d` parses as `(a -> b) + (c -> d)` — the impossible parse, and one that presupposes a flow has a value as an operand.

**Fix.** Per the table, `a -> b + c -> d` ≡ `a -> (b + c) -> d`. Additionally (parser-level decision, recorded in the same ADR): **a flow is a statement, not a value-producing expression**; `->`/`<-` chains are parsed at statement level. The example is corrected; an explanatory line is added.

## E5 — Rename surface keyword `category` → `type`

**Defect.** The keyword collision (surface `category` = type vs. ambient category-theoretic "category") actively confuses the spec's own exposition (flagged in `category-ir.md` Appendix A and `CHANGES.md` §8).

**Fix (accepted, pending Sapir's veto at bootstrap).** Rename now, while zero code exists — the last free moment. Affects `user-guide.md`, `getting-started.md`, all examples, the docx (deferred), and the `Ty` naming in the IR (already neutral). Keyword `category` may be reserved-and-rejected with a helpful error.

---

## Later corrections

### LC-1 — `?` inside parallel fanout in user-guide §7.3 (recorded 2026-06-11, Session 01)

user-guide.md §7.3 "Errors in parallel branches" shows `process_a?` / `process_b?` inside a parallel fanout. This interacts with Erratum E2's prohibition of effectful morphisms in fanout: error propagation is modeled as pure coproducts (CHANGES §1), so it is likely NOT an E2 effect — but the spec does not yet define how a join handles `Err` values from parallel branches. Error handling is Core+1 (HANDOFF §4.2). Resolution deferred to the Core+1 error-handling ADR; recorded here per HANDOFF §0 (never silently resolve a conflict). No spec text is changed by this entry.

### LC-2 — fold's inline block was written in call position (user-guide §10.4) (recorded 2026-06-11, Session 01)

user-guide.md §10.4 showed `array -> fold(0, { acc, item -> acc + item })` — the only place in the corpus where an inline block sits inside call parentheses, occupying an argument position. This contradicts HANDOFF §4.1 (the collection-operator block is **not a first-class value**), is inconsistent with `map`'s postfix-block form and the tuple-input call convention (`(v, lo, hi) -> clamp`), and would force a parser/lowering special case. Found by Sapir reviewing `examples/sepia.mapal`. **Fix — the collection-operator law:** data arrives through the wire; the inline block is postfix operator syntax, never an argument; the operator's input tuple corresponds positionally to the block's parameters. Canonical forms: `array -> map { item -> ... }` (array ↔ item) and `(init, array) -> fold { acc, item -> ... }` (init ↔ acc; array ↔ item) — the `(init, array)` order chosen exactly for that positional correspondence with `(acc, item)`. Lowering is unchanged: `Pair(init, array)` then the fold primitive (category-ir §4). Recorded in ADR-0009. Unlike LC-1, this entry **does** change spec text: §10.4's fold line and `examples/sepia.mapal` were patched in Session 01.

### LC-3 — labeled blocks gain a prefix `:` sigil (user-guide §3.5, §8.5) (recorded 2026-06-12, Session 03)

The v0.2 corpus wrote custom-labeled loops un-sigiled (`outer { … }`, `search { … }`, jumps `-> outer;`), which collides with statement-initial struct literals (`Pixel { … } -> p;` — both are `Ident {`) and makes jumps indistinguishable from variable flows without name resolution. ADR-0011 first resolved this with a content scan; reviewing it, Sapir reframed labels as *marks on blocks* (the loop being a side effect of the back-edge — exactly category-ir §4.5's adjacency-edge/SCC view) and proposed a sigil. **Fix (ADR-0012):** a labeled block is `:label { … }`; a jump is `-> :label;` (sigiled both ends; `:` before an identifier was previously illegal everywhere, so the form is conflict-free with one-token lookahead); jumps target lexically enclosing labels only (reducibility — required by E1 trace lowering and the Verilog done-protocol). The `loop` keyword form is unchanged. Labeled blocks remain Core+1 (P0110). This entry **does** change spec text: user-guide §3.5 nested-loops and §8.5 binary-search exhibits were patched in Session 03, each with an inline marker.

### LC-4 — dataflow must be edges: §4.1's Pair-metadata sentence and §5.3's `rhs_const` example contradict the graph analyses (recorded 2026-06-12, Session 04)

category-ir.md §4.1 said the `Pair` morphism's metadata "records *which projections* of the ambient environment to bundle", and §5.3's serialization example fused a constant into a primitive (`"op": {"Mul": {"rhs_const": 2}}`). Both hide value flow inside morphism payloads — invisible to `in_edges`/`out_edges` — contradicting §4.4/§4.5's own diagrams (components drawn as real in-edges), §5.1's merge-point detection (`in_edges.len() > 1`), §9.4 last-use, §9.5 reachability-based parallelism, and §10's lifetime frontier, all of which read adjacency only. Found while designing `mapal-ir` (Session 04). **Fix (ADR-0013):** all dataflow is adjacency edges; product formation is per-slot `Pair { slot, arity }` morphisms, one per component; constants are `Constant`-kind source objects (`value: Some`, per §3.2's existing field) rather than payload metadata or `1 → A` morphisms. Compact object/morphism tables elsewhere in §4 are to be read as eliding the component edges. This entry **does** change spec text: §4.1's metadata sentence and §5.3's example were marked in Session 04 with inline blockquotes.

### LC-5 — `seq` is a statement block, not a fanout of anonymous blocks; user-guide §5.2's canonical example did not parse (recorded 2026-07-17, Session 11)

user-guide.md §5.2 wrote the ordering construct as a fanout of anonymous blocks — `data -> seq { -> { "Step 1" -> log }; … }` — realized in the compiler as `StageKind::Fanout { kind: FanoutKind::Seq }`, the *same* parse node as a parallel fanout, differing only in a field. Two defects: (a) the anonymous block stage `-> { … }` is P0115-rejected (out of Core), so the §5.2 example fails at parse; (b) the shared node forced every parallelism-sensitive tree walk to key on the `kind` field rather than the node kind (a standing gotcha re-warned each session), lower gave `seq` no distinct semantics (`_kind` was dead), and non-`Chain` statements inside any fanout block were silently `filter_map`ped away with no diagnostic. Found designing the S11 `seq` increment; the composite reading also left OQ-C1 (is nested `seq { print }` inside a fanout branch illegal?) needing a conservative pin (CK5). **Fix (ADR-0019):** `seq { … }` becomes a **statement block in stage position** — its own `StageKind::SeqBlock(Block)` node, whose body is the ordinary block production (chains, `x <- e` rebinds, `loop`s, an optional tail chain; guard arms stay illegal — stray/malformed-arm P-codes P0004/P0005/P0106, +P0006 when mixed with statements; syntax DESIGN §14.4); `FanoutKind` shrinks to `Plain | Void`. Headless statements and the tail seed from the seq input; bindings escape to the enclosing scope (the `fanout.mapal` idiom); the seq's value is its tail chain's value (a seq whose chain continues, or that sits in return position, with no tail is an error — L1611); `seq` has **no IR footprint** — its ordering guarantee *is* the effect-token thread that statement-order lowering already produces (ADR-0013 unchanged, interp unchanged; pure statements inside carry no observable order). An effectful `seq` inside a `Plain` branch is then simply an effectful composite morphism in a branch, rejected by ADR-0003/E2 verbatim — CK5 upgrades from pin to theorem and **OQ-C1 is closed** (a pure seq in a branch is trivially legal). Independently, the silent non-chain-statement drop in the remaining `Plain`/`Void` fanout blocks now draws a new diagnostic **P0117**. This entry **does** change spec text: §5.2's prose and canonical example were patched to statement form (`data -> seq { "Step 1" -> log; "Step 2" -> log; "Step 3" -> log; }`) in Session 11, with an inline marker; §8.6 (channels, full-language) and the §5.4 decision table are untouched.

_Future spec fixes discovered during implementation are recorded here, each with its own ADR, per the session protocol (HANDOFF §7.2 step 7)._
