# interp — suggestions (category-theory derived)

> Improvements deduced from DESIGN.md's categorical model by FRAMEWORK rules. Each cites
> its rule and names the concrete change. Not applied — a backlog for future work.

| # | Rule (§) | Smell found | Proposed change | Payoff |
| --- | --- | --- | --- | --- |
| S1 | §5 deduce-don't-store / one-source-of-truth | `derive_plan` recomputes `build_in_scc`, `ir.loop_structure(f)` and `ir.topo_order(f)` on every loop entry, though `eval_fn` already computed `in_scc` and DESIGN §2 says the in-SCC set is "precomputed once." | Thread the `in_scc` set (and, ideally, the one `topo_order`) from `eval_fn` into `run_loop`/`derive_plan` instead of rebuilding; adopt the §2 object→merge `SecondaryMap` so `derive_plan` needs no `loop_structure` rescan. | One computation per activation, code matches the model's "precomputed once", removes a full `ir.morphisms()` scan per loop entry. |
| S2 | §5 one-source-of-truth for shared structure | The 5-way numeric-width dispatch `(I32,I32) \| (I64,I64) \| (U8,U8) \| (F32,F32) \| (F64,F64)` is forked across `arith`, `num_lt`, `num_le`, and `as_int` (4 sites). Adding a numeric width means editing all four in lockstep with nothing forcing it — the drift smell. | Confine the width enumeration to one seam (a single `numeric_binop!` macro or a `for_each_numeric` dispatcher the four ops feed into). | Adding/removing a scalar width touches one place; the arms cannot silently diverge. |

## Detail

**S1 (the strongest).** DESIGN §2 is explicit: "The in-SCC object set is precomputed once."
The code computes it once in `eval_fn` (`crates/mapal-interp/src/eval.rs:build_in_scc`) for the
incidence test, then `crates/mapal-interp/src/loops.rs:derive_plan` calls `build_in_scc` *again*,
plus `ir.loop_structure(f)` (to find the SCC) and `ir.topo_order(f)`, and scans all
`ir.morphisms()` to locate the `LoopExit`. This is recomputation, not a stored copy, so it is
not a *correctness* defect (deduce-don't-store is satisfied — nothing drifts). But it is the
efficiency face of the same rule: the one source (`eval_fn`'s computed structure) is discarded
at the driver boundary and rebuilt. Passing it through is the one-source-of-truth move and
closes the code↔model gap recorded in `IMPLEMENTATION.md` divergence 3. Scope note: at M1 loops
are entered rarely, so the payoff is model-alignment first, cycles second — reasonable to defer.

**S2.** This is a genuine forked-structure smell but with a YAGNI caveat: the scalar type set is
frozen for M1 (spec-frozen `mapal_ir::Value`), so the "third caller / future width" trigger for
abstraction has not actually arrived. Two ~10-line match functions are *not* worth a macro today
by FRAMEWORK §5's own "three similar lines beat a premature abstraction." Flag it, don't apply it
— revisit only if a numeric width is ever added (then the macro earns its place immediately).

**Considered and rejected (cite the ledger, do not re-litigate):**

- **`Abort` vs `Outcome`** *looks* like a §3 translator (`Abort::into_outcome` converts between
  two error shapes). It is not: `Result<RValue, Abort>` is `Done(RValue) ⊕ Abort` with the
  `Done` payload in the `Ok` slot; the separation buys straight-line `?`-propagation through the
  per-op happy path. **Ledgered** at DESIGN §1 / IN2. Stays split.
- **`RValue` mirrors `mapal_ir::Ty`** (scalar/tuple/struct/array) but is a genuinely richer object
  (adds `Token`, `Unit`, and holds *values* not *types*). Not a consolidation candidate — the
  surface-`Ty`/IR-`Ty` and value/type distinctions are already adjudicated in
  `docs/architecture/categorical-model.md` §7.2. Stays.
- **The two `SourceLoc`s, `IrError` vs `IrViolation`, surface-`Ty` vs IR-`Ty`** — all resolved as
  justified differences in `docs/architecture/categorical-model.md` §7.2; not interp's to
  re-open.

No further unledgered smells found: the value domain, the single `eval` `Trn`, and the
guard-first driver are already the reduced forms (IN1–IN8, ADR-0016).
