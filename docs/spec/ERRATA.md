# Flow — Spec Errata

This file records the accepted corrections to the v0.2 specification corpus. **The Flow specification is frozen at v0.2 plus this file.** No further design change touches the v0.2 documents directly; all change flows through ADRs and is recorded here when it corrects spec text.

**Authority order (highest wins; per HANDOFF §2.2):**

1. Accepted ADRs in `docs/decisions/` (including the bootstrap ADRs encoding errata E1–E5)
2. `category-ir.md` v0.2 (formal semantics)
3. `user-guide.md` v0.2 and `architecture.md` v0.2 (tie: user-guide for language behavior, architecture for compiler structure)
4. `getting-started.md` v0.2
5. `CHANGES.md` (rationale, not normative)
6. `flow-language-design.docx` (historical)

**Erratum → ADR mapping:**

- E1 → ADR-0002
- E2 → ADR-0003
- E3 → ADR-0004
- E4 → ADR-0005
- E5 → ADR-0006

The textual patches for E1–E5 were applied to the spec files on 2026-06-11 (Session 01). Affected sections in `category-ir.md` (§2.1/2.7/2.8/8.3) and `user-guide.md` (§3.6, §5.4, §6.5) carry an inline marker pointing back to this file and the corresponding ADR.

---

## E1 — Flow-Cat cannot be both "total" and traced-cartesian

**Defect.** `category-ir.md` §2.1 defines morphisms as pure **total** functions; §2.8 claims Flow-Cat is traced with monoidal product = categorical product. Jointly inconsistent: a traced cartesian category is equivalent to one with a Conway fixed-point operator (Hasegawa 1997), and total functions lack fixpoints in general (`not : Bool → Bool` has none). Unbounded loops are exactly where partiality enters (`loop { -> loop; }` is legal and diverges).

**Fix.**

- Loops/iteration live in the Kleisli category of the partiality (divergence) monad — the same §2.6 machinery already used for I/O and errors. The total core of Flow-Cat has no trace; the traced structure exists on the partial extension (least-fixpoint / Elgot-iteration semantics).
- `category-ir.md` §8.3 must be rewritten: Clocked-Cat's trace is **guarded** (register = unit delay ⇒ always productive ⇒ total; Mealy-machine semantics). `F_Verilog` therefore maps an _iteration_ trace to a _guarded_ trace — different traced structures. "F commutes with Tr" is not free; it is a theorem with content, mediated by a **done-signal protocol**: _the iteration terminates in n steps with value v ⟺ the circuit asserts `done` at cycle n with output v_. This theorem is the project's most publishable single result; state it precisely, discharge it informally now, mechanize later.

**Implementation impact.** Interpreter loop semantics are partial: all loop evaluation carries a fuel/step-limit in tests; divergence is a defined outcome, not a hang. The Verilog FSM for any lowered loop implements the done protocol (`valid_in / busy / done / result` handshake).

## E2 — Parallel effects rule (replaces "executor decides")

**Defect.** `user-guide.md` §5.4 row 4: "Independent + effectful → Executor decides (may parallelize with non-deterministic order)." This makes program meaning scheduler-dependent, contradicting both "no data races by construction" and the functorial-correctness story (if the denotation is "whatever the executor did," there is nothing for a functor to preserve).

**Fix.** Effectful morphisms are **not permitted in parallel fanout**. Effects either (a) sequence via `seq`, or (b) communicate via channels with **Kahn process network semantics** — blocking reads, unbounded FIFOs — under which determinism independent of scheduling is a theorem (Kahn 1974). The streaming/FPGA subset later adopts synchronous-dataflow restrictions (Lee & Messerschmitt 1987) for static schedules and bounded buffers. Channels are out of Flow-Core scope (§4), but the rule is fixed _now_ so the effect checker is built right the first time.

## E3 — Memory-model guarantee is scoped

**Defect.** "No use-after-free / double-free / leaks / races, with zero annotations" is claimed for the whole language. Whole-program region inference at general-purpose scope (closures, channels, cyclic structures) is historically treacherous (cf. Tofte–Talpin region pathologies); cycles already punt to refcounting.

**Fix.** State the guarantee as **proven for the first-order, non-cyclic dataflow core** (which contains Flow-Core entirely) and **open for the full language**. `user-guide.md` §6.5 is amended accordingly. Implementation benefit: the Flow-Core lifetime engine is simple (stack/static allocation for fixed-size data; last-use frontier for arrays) and can be exactly right.

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

user-guide.md §10.4 showed `array -> fold(0, { acc, item -> acc + item })` — the only place in the corpus where an inline block sits inside call parentheses, occupying an argument position. This contradicts HANDOFF §4.1 (the collection-operator block is **not a first-class value**), is inconsistent with `map`'s postfix-block form and the tuple-input call convention (`(v, lo, hi) -> clamp`), and would force a parser/lowering special case. Found by Sapir reviewing `examples/sepia.flow`. **Fix — the collection-operator law:** data arrives through the wire; the inline block is postfix operator syntax, never an argument; the operator's input tuple corresponds positionally to the block's parameters. Canonical forms: `array -> map { item -> ... }` (array ↔ item) and `(init, array) -> fold { acc, item -> ... }` (init ↔ acc; array ↔ item) — the `(init, array)` order chosen exactly for that positional correspondence with `(acc, item)`. Lowering is unchanged: `Pair(init, array)` then the fold primitive (category-ir §4). Recorded in ADR-0009. Unlike LC-1, this entry **does** change spec text: §10.4's fold line and `examples/sepia.flow` were patched in Session 01.

_Future spec fixes discovered during implementation are recorded here, each with its own ADR, per the session protocol (HANDOFF §7.2 step 7)._
