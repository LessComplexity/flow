# Component: interp

Status: tested
Last updated: 2026-06-15 · Session 08
Spec references: interp/DESIGN.md (increment 1, M1) · ADR-0002 (E1 fueled/Elgot loop semantics) · **ADR-0016 (guard-first loop branch evaluation)** · ADR-0013 (traps: int div/mod-zero, OOB Index; float IEEE) · ADR-0015 (print/println) · ir/DESIGN §5.1/§7/§8/§13 · category-ir.md §2.6–2.8/§11.4.
Depends on: ir (only — borrowed `&CategoryIr`, never mutated; + `slotmap` for the env/buffer `SecondaryMap`s). Tests also use lower + syntax (dev-deps) for the `parse→lower→run` pipeline.
Depended on by: rewrite, backend-llvm, backend-cuda, backend-verilog, cli (the differential-test oracle).

## What works

The fueled reference interpreter (THE ORACLE) — `parse → lower → run` for all six `examples/*.flow` produces the exact pinned output:
`abs "7\n"` · `sum_to_n "55\n"` · `pipeline "f(10) = 25\n"` · `fanout "36\n12\n"` · `fir "5.375\n"` · `sepia "4080\n"`. Plus the committed `countdown` fixture → `"5\n4\n3\n2\n1\n0\n"`.

- **Value domain (§1):** `RValue = Scalar(flow_ir::Value) | Tuple | Struct | Array | Token(String) | Unit`; `Outcome = Done | Diverged | Trapped(TrapKind)`; internal `Result<RValue, Abort>` with `?`, lifted to `Outcome` at the boundary.
- **Evaluator (§2/§3):** env (`SecondaryMap<ObjectId, RValue>`) + topo walk; per-slot `Pair` product assembly into Tuple/Struct/Array; all Core ops (arith at operand width, Div/Mod int-trap & float-IEEE, Neg fneg, comparisons with **IEEE NaN ordering**, And/Or/Not, Phi, Proj, Call, Map, Fold, Index, Print{newline}, Output).
- **Guard-first loop driver (§4 / ADR-0016):** the decide/exit cone is evaluated first, the guard is read, and the continue-branch (next-state) is evaluated only when the guard continues — so `fir`'s `coeffs[k]` is never indexed at `k=4` (no spurious `IndexOob`) and `countdown` prints `0` on its exit step. The 55-contract holds by execution.
- **Effects (§5):** world token = `RValue::Token(String)`; `print` raw, `println` appends `\n`; effect order = dataflow order (E2 structural, no scheduler). Floats render via Rust shortest round-trip (`4080.0→"4080"`, `5.375→"5.375"`).
- **Fuel / divergence / traps (§6):** global `u64` budget decremented per morphism; `0 ⇒ Diverged` (returns, never hangs). `Trapped(DivZero)`/`Trapped(IndexOob)`; float `1.0/0.0 ⇒ Done(inf)`.
- **Entry & API (§7/§8):** `run(&ir, budget) -> RunResult{outcome, output}` (seeds `Token("")` for an `IoToken` entry, `Unit` otherwise); `eval_call(&ir, f, arg, budget) -> Outcome`. No `Display` (C3).

## What does not / known issues

- Out-of-M1 loop shapes (multi-merge SCC, >1 `LoopBack`, ≠1 attributed `LoopExit`) are surfaced via `assert!` in `derive_plan`, not a returned error variant (`Outcome` has no error case; DESIGN §9 classes these as `unreachable!`-class, and lower OQ7 never generates them). Not user-reachable through the supported pipeline.
- `derive_plan` recomputes the per-merge layout (topo/SCC/decide-cone fixpoint) on every `run_loop` invocation rather than caching once per merge — a minor perf cost only for a looping fn called repeatedly (none in the six examples). Deferred (YAGNI; profile-driven).
- Per-iteration reset clears staging buffers, relying on each product's slot feeders living in a single phase (true at M1 — no straddling product across decide/advance). Not asserted in code; revisit if a future loop shape splits a product's slots across the guard.
- Integer overflow uses `wrapping_*` (IN7, out of M1 scope; no example overflows) — pinning UB-vs-wrap-vs-trap is a later `flow-check`/backend ADR.

## Invariants enforced (and where in code)

- **Totality (C-interp-1/E1):** every run halts with `Done`/`Diverged`/`Trapped`; budget decrement per `eval_morphism` (`eval.rs`); divergence threaded as `Abort::Diverged` via `?`, lifted in `eval_call`/`run` (`lib.rs`).
- **Guard-first (ADR-0016):** `loops.rs` `run_loop` evaluates `decide_order` → reads `cond` from `exit_route@1` → evaluates `advance_order` only on continue.
- **Determinism (C-interp-3/E2):** `SecondaryMap` + `Vec` only, no `HashMap`; Map/Fold iterate array order (`eval.rs`). Tested by running each example twice.
- **No IR mutation:** the IR is borrowed `&` throughout; the library depends on `flow-ir` (+ `slotmap`) only.

## Test coverage (golden / property / differential / skipped+why)

27 tests, all green (workspace 393 total). `tests/acceptance.rs` (10): six example goldens + countdown + `eval_call(sum_to_n,10)==Done(I32(55))` + `fir4(...)==Done(F32(5.375))`. `tests/traps.rs` (8): int div/mod-0, Index `i=n`/`i=-1`, float `1.0/0.0=Done`, **`nan_ordering_is_ieee`** (Lt/Gt/Le/Ge/Eq false, Neq true on NaN), in-bounds/nonzero sanity. `tests/divergence.rs` (3): budget boundary on `sum_to_n` + constant-`true`-guard loop ⇒ `Diverged` (returns). `tests/determinism.rs` (6): each example byte-identical across two runs. Differential (backend-vs-oracle) lands with the backends (P5+).

## Performance notes (numbers + bench name + date; regressions flagged)

`benches/interp_scale.rs` (criterion) — baseline Session 08 (2026-06-15): `deep_loop/1000` 323 µs, `deep_loop/10000` 3.20 ms; `large_map/1000` 862 µs, `large_map/10000` 8.56 ms (≈ linear in step count). First baseline; no regressions tracked yet.

## Open questions (→ ADR candidates)

- IN6 float ÷0 (IEEE, no trap) still wants the one-line ADR-0013 amendment (integer-trap / float-IEEE) before it is normative across backends.
- Multi-merge / multi-back-or-exit loop SCCs are out of M1 (§4 scope); lifting waits on lower OQ7.
- `flow-check` owes the §9 assumptions (Return exclusivity IN3, `seq` effect legality E2, full typing/E3).
