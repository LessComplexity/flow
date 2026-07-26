# ir — suggestions (category-theory derived)

> Improvements deduced from DESIGN.md's categorical model by FRAMEWORK rules. Each cites
> its rule and names the concrete change. Not applied — a backlog for future work.

| # | Rule (§) | Smell found | Proposed change | Payoff |
| --- | --- | --- | --- | --- |
| 2 | §3 (a partial morphism's refused domain) + plan-s28-shapes-ladder §Ceilings | rule-1 refusal (S28): a read affine in raw `k` AND in the derived `(k÷div, k%div)` axes refuses — `TileRead.ksplit?` covers XOR forms only | general ksplit (`ck ≠ 0 ∧ ksplit.is_some()`): widen the walker's coefficient space (6-wide) to record mixed reads | mixed-form k-split sites become recordable; **no measured demand** — the rule-1 refusal is documented and pinned (`tile_refuses_conv2d_mixed_raw_and_derived_k`) |

_(Suggestion 1 — the §5.1 typing-table golden oracle — applied Session 09: `src/validate.rs::typing_table_golden::edge_type_ok_matches_design_5_1`.)_

## Detail

**Consolidation / deduce-don't-store candidates examined — all already ledgered, no change proposed:**

- **`IrError` (builder) vs `IrViolation` (validate)** — a §3 "two objects, same shape" candidate.
  **Ledgered STAY-separate** (categorical-model.md §7; DESIGN §11 "`IrViolation` mirrors `IrError` but
  carries ids instead of build context"). The two carry different data (build context vs post-seal
  ids) and belong to the two independent realizations. Do not merge. Cited, not re-proposed.
- **The parallel `check_*` helpers in `builder.rs` vs `validate.rs`** — a §3/§5 "one-source-of-truth"
  candidate on its face. **Deliberate** (FRAMEWORK §7.2; DESIGN §11): merging them turns
  `seal Ok ⇒ validate empty` into a tautology. This is the one place the framework's own "validate
  twice, honestly" *overrides* consolidation. No change.
- **`topo_order` / `sccs` / `loop_structure` / loop regions / `Operation::Trace`** — §5
  deduce-don't-store. **Already deduced, ledgered D3/D5**: order, SCCs, and loop regions are recomputed
  from adjacency, never stored; `Trace` is unmaterialized ("the trace IS the cycle"). Exactly the §5
  discipline, already applied. No change.
- **`in_edges` / `out_edges` stored adjacency** — §5 "stored copy of a deduced morphism." Stored for
  hot-path navigation *with* a consistency mechanism (single writer `add_edge`, append-only-then-seal),
  which is precisely the §5-sanctioned case. The model lists them as total morphisms, not deduced. No
  change (noted in IMPLEMENTATION.md divergences instead).
- **The two `SourceLoc`s (mapal-ir vs mapal-syntax) and surface-`Ty` vs IR-`Ty`** — **ledgered
  STAY-separate** (D8; categorical-model.md §7). Dependency-direction and Core-subset reasons. Cited,
  not re-proposed.
- **YAGNI scan** — no speculative abstraction found: `Dest`/`FuncKind`/`ObjectKind`/`Operation`
  variants are all live; JSON, mutation API, and bifunctor tagging are explicitly deferred with no
  code (DESIGN §0). Nothing to delete.

Net: one modest, non-ledgered suggestion from this audit (a test oracle for the §5.1 table — applied
Session 09). Every larger consolidation the model invites has already been run and adversarially
verified in the D1–D10 / I-invariant ledger and categorical-model.md §7. (#2 arrived later — the
S28 k-split ceiling, carried over from plan-s28-shapes-ladder §Ceilings, not a finding of this audit.)
