# Next Session

Written: 2026-07-18 · end of Session 12 · by: Claude (Fable 5 orchestrator; Opus workflow agents; category-architect skill)

## Where things stand (≤5 lines)

**P4 COMPLETE (S12).** `flow-rewrite` shipped: plan+replay over sealed IR (rebuild-through-builder, ir §17), const fold + CSE + DCE + map fusion, fixpoint driver, R1 property harness over the new **testgen** random-program generator (P5–P7 will reuse it). Two interp **P0s found & fixed** (multi-hop loop-invariants; two-sequential-loops — both via Sapir's matmul exploration). **P5 prep is banked**: ADR-0020 (emission contract + `flow-rt`) and backend-llvm DESIGN are written; verilator + icarus installed; vastai CLI verified for P6. Full detail: `sessions/2026-07-18-p4-rewrites.md`.

## Test state: ALL GREEN

`cargo test --workspace`: **511 passed, 0 failed** (192 syntax · 102 ir · 128 lower · 29 check · 37 interp · 23 rewrite). fmt + clippy clean.

## Do next (ordered, smallest-first)

1. **P5 backend-llvm (M2)** — DESIGN.md is written and review-ready (`components/backend-llvm/DESIGN.md`): implement `crates/flow-rt` (ADR-0020 §2) + the alloca-slot emitter + differential harness (10 examples + testgen, raw + rewritten IR) + sepia perf baseline. Suggested flow: adversarial design-review pass on the DESIGN first (S12 did this for rewrite and it killed 4 blockers pre-code), then the sequenced-TDD workflow, then orchestrator line-by-line review.
2. (Optional, small) suggestions #6 (lower `ChainCtx::RetValue` uniformity) and #7 headroom items (precise DCE / constant dedup) — any session.
3. P6 CUDA (vast.ai RTX 4090 route, backend-cuda/STATUS.md) after P5; P7 Verilog (verilator installed) after; M5 CLI last.

## Open questions for Sapir

- **New (S12):** RATIFY or amend **ADR-0020** (emission contract: flow-rt runtime, exit-101 traps, no-`nsw` wrapping) — P5 builds on it. RATIFY **rewrite R1/RW2** (traps ⊥-identified, fuel-insensitive oracle equality) — the P4 "interpreter-equal" pin; ADR if contested. **Array-update design note** (`notes/array-update-design.md`): recommendation = pure `Update` op + `c[i] <- x` desugar onto mut-rebind, in-place via E3 last-use (the answer to your S12 question; Core+1 ADR candidate).
- **Carried:** RATIFY ADR-0016 (guard-first loops); ADR-0013 review; IN6 float ÷0 amendment; lower §16 OQ1–OQ8.

## Gotchas / warnings (things that will waste the next session's time)

- **All S08–S11 gotchas stand** (guard-first driver; `LineIndex<'a>`; `Name` carries no string; check runs no typing pass; CK/LD/RW ledgers no-relitigate; Fanout+SeqBlock walker rule).
- **New S12 loop-walker rule:** `topo_order` **defers LoopEnter** — every non-merge-gated morphism precedes its loop header (ir §13). Anything walking loops must attribute exits/body **per-merge-SCC, never the per-fn union** (two sequential loops per fn are legal + supported; the union pattern panicked interp and mis-skipped rewrite — both pinned). Backends: emission order = the same `topo_order`, so invariants-before-header transfers for free.
- **Rewrite plan laws P1–P3** (DESIGN §1.2) are load-bearing: plans key non-SCC `Temporary` objects only; alias preserves SCC membership; fusion needs loop-free bodies. Three review blockers converged there — do not relax casually.
- **testgen lives at `crates/flow-rewrite/tests/testgen/mod.rs`** — consumed cross-crate via `#[path]` include (the lower `tests/common` pattern), NOT a library export (HANDOFF §9 placement).
- **`vector.flow` is out-of-Core** (generics sketch) — the example set for harnesses is the 10 in-Core files.
- **`RewriteResult` is Debug-only** (CategoryIr not Clone); `rewrite` takes the graph **by value** — differential harnesses must run the oracle on the input *before* rewriting.
- **P6 CUDA:** nvcc absent locally; vast.ai CLI verified (no instances running). Order stays P5 → P6.

## Commands (build/test/bench invocations that currently work)

```sh
cargo test --workspace                                   # 511 green (192+102+128+29+37+23)
cargo test -p flow-rewrite --test property               # R1 battery (PROPTEST_CASES=2000 for a deep run)
cargo test -p flow-interp --test loop_invariants         # the S12 P0 pins (matmul4, two-loops)
cargo bench -p flow-rewrite                              # rewrite_scale (chain/grid, ~linear)
cargo run -p flow-lower --example dump_ir -- examples/sepia.flow   # Mermaid dump
git log --oneline -3                                     # S12 commit(s)
```
