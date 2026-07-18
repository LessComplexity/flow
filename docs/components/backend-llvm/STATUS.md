# Component: backend-llvm

Status: tested
Last updated: 2026-07-18 · Session 13 (P5/M2 — full text emitter + `flow-rt` + differential harness; loop CFG on `flow_ir::loop_plan`)
Spec references: ADR-0020 (backend emission contract) > category-ir.md §8.1 (`F_LLVM`) + §8.5 (piecewise correctness). Binding model: DESIGN.md (L1–L4, §2–§4, BL1–BL8). Supporting: ADR-0016 (guard-first loops), ADR-0021 (Update), architecture.md §4.1.
Depends on: ir, interp (dev), rewrite (dev), flow-rt Depended on by: cli

## What works

**P5/M2 complete (S13).** Text emitter `emit(&CategoryIr) -> Result<String, EmitError>` (`src/lib.rs`) realizing `F_LLVM` as a `.ll` String, plus the shared `crates/flow-rt` runtime (7 print externs + `flow_trap` exit-101) it links against.

- **Op table (§2)** — every Core op emits (`func.rs`): wrapping int `add/sub/mul` (no `nsw`/`nuw`); signed `Div`/`Mod` with zero-guard + `MIN/-1` guard (Div ⇒ `MIN`, Mod ⇒ `0`, `wrapping_div`/`wrapping_rem` parity); u8 `udiv`/`urem`; IEEE floats at width (`fadd`..`frem`, `fneg`); int/float compares (`fcmp` ordered, `une` for `!=`); `select`-Phi (strict, both cones execute — BL2); `Pair`/`Proj` as GEP store/load; `Call` direct; counted-loop `Map`/`Fold`/`Zip`/`Enumerate`; `Print` via flow-rt at its topo position.
- **Loop CFG (§3)** on `flow_ir::loop_plan` (BL7 — the one-source-of-truth per-merge attribution predicate; `loops.rs:emit_loop`): guard-first quartet (`entry`/`header`/`advance`/`exit`/`after`), decide cone every iteration incl. the exit one, advance cone unreachable on exit. Two sequential loops per fn work; an exit-only computed payload is emitted exactly once (driver-owned, `walk()`).
- **Token erasure (L4)** — arity-0 (no slot), arity-1 (bare component, plain store/load), arity-≥2 (`{…}` struct + GEP), remap derived on demand from the ty (`ty.rs`).
- **`Update` (ADR-0021)** — same index guard as `Index`, `llvm.memcpy` source array → fresh target slot (via `getelementptr null` sizeof), dynamic GEP + element store (`func.rs:emit_update`).
- **`zeroext` ABI** — every `i8`/`i1` param carries `zeroext` in the `declare` lines and at every call site; the attr goes **after** the type in call args (`i8 zeroext %v`) — the S13 fix for the invalid-LLVM attr-before-type form that clang's own bug-hunt caught.
- **Determinism (L2)** — names from per-fn ordinals + a rising counter, never slotmap bits; emit twice is byte-equal.
- **Differential harness** — emit → `clang <prog>.ll libflow_rt.a -o prog` → run (time-boxed) → compare per L1; examples raw + rewritten, closed-mode testgen sweep (≥256 cases × raw+rewritten, fanned across 8 threads), u8 ABI, matmul loop-driven Update, traps exit-101. Render parity to interp is pinned in flow-rt's unit table.

## What does not / known issues

- **Nested loops (multi-merge SCC) ⇒ `EmitError::Unsupported { "nested loops" }`** (L3/BL6; `lib.rs:emit` gate) — the same scope boundary as interp M1 and rewrite RW8; lifted across the toolchain in one increment or not at all.
- **Alloca-slot stack ceiling (BL1)** — whole array aggregates live in the frame. The sepia perf shape at N = 262144 holds ~9 MB of `[Pixel; N]` allocas, over the 8 MB default; the perf test raises it via `ulimit -s hard` (`perf_baseline.rs:run_big_stack`). Examples/testgen arrays are tiny and never approach it. In-place Update / heap lowering is the recorded headroom that lifts the ceiling.
- **Perf N capped at 4096** — the array literal is N `Pair` stores (Core has no array-fill); at N = 262144 clang `-O2` needs >25 min CPU on the ~1M-line module and 65536 is still minutes (observed S13). 4096 already shows the scale story unambiguously; an array-fill primitive / heap lowering restores large N.
- **Diverged testgen programs are skipped** in the differential (no finite native observable); open-mode `i32 → i32` testgen is excluded (no native `@main` analog, BL8).

## Invariants enforced (and where in code)

L1 oracle parity (`Done` ⟺ exit 0 + stdout byte-equal; `Trapped` ⟺ exit 101 — `differential.rs:expect_native`/`assert_parity`); L2 determinism (`golden_ll.rs:determinism_emit_twice_byte_equal`); L3 capability gate (`lib.rs:emit` `loop_plan(...).is_some()` per merge, `golden_ll.rs:nested_loop_is_unsupported`); L4 token erasure (`ty.rs:lower_ty` returns `None`, no slot allocated in `func.rs:emit`); guard-first decide/advance split (`loops.rs:emit_loop` on `loop_plan`); strict `select`-Phi (`func.rs:emit_morphism` `Phi` arm, BL2); type-directed index guard (`func.rs:guard_index`/`load_index`).

## Test coverage (golden / property / differential / skipped+why)

- **golden (`tests/golden_ll.rs`, 7 tests):** `golden_examples` (10 insta snapshots) + `golden_micro_{arith,update,two_loops}` (3 snapshots) + `determinism_emit_twice_byte_equal` + `nested_loop_is_unsupported` (L3 pin) + `exit_only_payload_emitted_once` (driver-ownership pin) = 13 golden `.ll` snapshots.
- **differential (`tests/differential.rs`, 6 tests):** `differential_examples_raw_and_rewritten` (the M2 line — 10 examples × raw+rewritten), `differential_two_sequential_loops` (→ `"20\n"`), `differential_traps_exit_101` (div0 + OOB Update), `differential_u8_index_and_print` (the u8 `zext`+guard + `i8 zeroext` ABI — the sole compile-and-run cover for that class), `differential_testgen_closed_sweep` (≥256 closed cases × raw+rewritten), `differential_matmul_loop_driven_update` (loop-carried `mut c` + `c[t] <- v`, → `"8\n136\n"`). All skip-with-reason when `clang` is absent (never a faked pass).
- **render parity (`crates/flow-rt`, 1 test):** `render_parity` table — f64/f32 (`4080.0`, `5.375`, `-0.0`, `NaN`, `inf`, MAX), u8 (0/127/128/255), bools, Str, i32/i64 extremes, each byte-equal to `flow_interp::render`.
- **perf (`tests/perf_baseline.rs`, 1 test, `#[ignore]`):** `sepia_perf_baseline`.

## Performance notes (numbers + bench name + date; regressions flagged)

`sepia_perf_baseline` (2026-07-18, S13, idle machine, arm64 -O0/-O2 vs interp; `cargo test -p flow-backend-llvm --test perf_baseline -- --ignored --nocapture`):

| N | interp | native -O0 | native -O2 |
|---|---|---|---|
| 16 | 1.53 ms | 4.60 ms | 4.61 ms |
| 4096 | 387.36 ms | 4.84 ms | 4.82 ms |

Native time is process-spawn-dominated (flat ~4.8 ms across N); at N = 4096 the compute gap is ~**80×** in native's favor and grows with N. First baseline; no regressions tracked yet. Larger N blocked on the array-literal module size (bullet above).

## Open questions (→ ADR candidates)

- `-O2` differential row (DESIGN §8) — cheap add once `-O0` differentials are green; catches LLVM-level UB accidentally relied on.
- `frem` vs Rust `%` parity for float `Mod` (DESIGN §8) — pin with a differential case; if `frem` diverges, call `fmod` from flow-rt instead.
- Nested-loop emission increment (with interp + rewrite, BL6).
