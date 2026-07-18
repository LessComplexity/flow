# Next Session

Written: 2026-07-18 · end of Session 13 · by: Claude (Fable 5 orchestrator; Opus workflow agents; category-architect skill)

## Where things stand (≤5 lines)

**P5 COMPLETE — M2 REACHED (S13).** `flow-backend-llvm` + `flow-rt` shipped: full textual-LLVM emitter, differential-tested against the oracle on real clang (10 examples + 320-case testgen sweep, raw **and** rewritten IR; native loop-driven matmul `8\n136\n`; traps = exit 101; ~80× native over interp at sepia N=4096). **ADR-0021** landed the same session: `c[i] <- x` array update, pipeline-wide. Sapir ratified ADR-0013 (+IN6 float-÷0 amendment), 0016, 0020, RW2. `flow_ir::loop_plan` is now the single loop-attribution predicate (BL7). Full detail: `sessions/2026-07-18-s13-array-update-p5-llvm.md`.

## ⚠ Live infrastructure (found at S13 close — needs Sapir's eyes)

**Two vast.ai RTX 4090 instances are RUNNING and billing** (found by the end-of-session check at 2026-07-18; S12/S13 logs said "no instances", so these appeared outside the recorded sessions — presumably rented by Sapir for P6, possibly forgotten):

| # | ID | Model | Util at check |
|---|---|---|---|
| 1 | 45170851 | RTX 4090 | 0% |
| 2 | 45170852 | RTX 4090 | 12% |

Inspect: `vastai show instances` · stop: `vastai destroy instance <ID>` (Sapir's call — not stopped by the orchestrator). If intentional for P6: S14 can use one directly and should destroy the other (P6 needs a single box).

## Test state: ALL GREEN

`cargo test --workspace`: **558 passed, 0 failed** (199 syntax · 106 ir · 139 lower · 29 check · 44 interp · 27 rewrite · 13 backend-llvm · 1 flow-rt). fmt + clippy clean.

## Do next (ordered, smallest-first)

1. **P6 backend-cuda (M3)** — nothing written yet (STATUS stub only). Flow: DESIGN model-first (ADR-0020 contract; map-kernels via nvcc; host-side prints — E2 keeps effects out of kernels; the H↔D `Trm` makes the physical pair real for the first time) → adversarial design review (the S12/S13 pattern kills blockers pre-code) → implementation workflow → orchestrator line-by-line review. **GPU leg:** nvcc absent locally — rent the vast.ai RTX 4090 box (memory `vast-ai-gpu-access`; `vastai show instances` currently empty). The backend-llvm differential harness + testgen port directly (closed-mode, raw+rewritten, exit-101).
2. (Small, mechanical) migrate rewrite's `is_canonical`/`exit_of` onto `flow_ir::loop_plan` (open item P3 — S12 pins prove equivalence; one predicate, three consumers).
3. (Optional headroom, any session) backend-llvm suggestions (in-place Update via last-use; array-fill/heap lowering → restores perf N=262144; `-O2` differential row; `frem` parity pin) · rewrite suggestions (#5 `reoperand` → laws L-b/L-c; #7 precise DCE).
4. P7 Verilog (verilator installed) after P6; M5 CLI last.

## Open questions for Sapir

- **Lower §16 OQ1–OQ8** — still open: these are *questions*, the S13 blanket ratification covered decisions only. Answer individually when convenient.
- Nothing else pending — the S12 ratification stack is fully closed (ADR-0013+IN6, 0016, 0020, RW2, ADR-0021).

## Gotchas / warnings (things that will waste the next session's time)

- **All S08–S12 gotchas stand** (guard-first driver; `Name` carries no string; CK/LD/RW ledgers no-relitigate; Fanout+SeqBlock walker rule; per-merge attribution; testgen via `#[path]`, not a library export).
- **New S13:** loop attribution lives in **`flow_ir::loop_plan`** — use it, never re-derive (backend-llvm `loops.rs` + interp consume it; rewrite still has its own copy, open item P3). The emitter's `walk()` skips driver-owned morphisms by **plan membership ∪ SCC incidence** — SCC incidence alone double-emits exit-only chains (a duplicated exit-arm `Print` breaks R1; pinned by `exit_only_payload_emitted_once`).
- **LLVM text ABI:** parameter attrs go **after** the type in call args (`i8 zeroext %v`); `zeroext` on every i8/i1 flow-rt param at declare AND call sites. This was invalid-LLVM in the first draft and only the u8 differential caught it — keep that test.
- **Alloca-slot stack ceiling (BL1):** whole arrays live in frames; huge-N shapes need `ulimit -s hard` (see `perf_baseline.rs:run_big_stack`). Perf N capped at 4096 — the array literal (no array-fill in Core) makes clang -O2 time explode at large N.
- **Differential harness rules:** oracle runs BEFORE `rewrite()` (IR taken by value); closed-mode testgen only (open `i32 → i32` has no native observable); `Unit → i32` entries get the result-printing wrapper; Diverged programs skip.
- **CUDA design seeds (from S13 decisions):** flow-rt links into the host side unchanged (prints/traps host-side); wrapping ints no-`nsw` transfers; the capability matrix `array update` row is `planned` for cuda — naive per-thread copy is correct, in-place is headroom.

## Commands (build/test/bench invocations that currently work)

```sh
cargo test --workspace                                        # 558 green
cargo test -p flow-backend-llvm --test differential           # the M2 line (needs clang; ~4 min)
cargo test -p flow-backend-llvm --test golden_ll              # 13 .ll snapshots, fast
cargo test -p flow-backend-llvm --test perf_baseline -- --ignored --nocapture   # sepia numbers
cargo test -p flow-interp --test update_pipeline              # loop-driven matmul oracle pin
cargo test -p flow-rewrite --test property                    # R1 battery (PROPTEST_CASES=2000 deep)
vastai show instances                                         # ⚠ 2 RTX 4090s RUNNING at S13 close — see warning above
git log --oneline -3                                          # S13 commit(s)
```
