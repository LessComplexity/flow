# 2026-07-25 — S30: tile accumulators as vector SSA values

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Follows
`2026-07-25-s29b-kc-diagnosis.md`, which found that the KC nest's 3× loss was a failed
register promotion rather than memory traffic. Sapir: "do the phi rewrite for the tile
accumulators", then "see how it affected other runs/algorithms and bigger sizes, validate
the disasm/IR, and where/when does KC pay off if any."

## 0. Continuation brief

Current state: **the phi rewrite is in, green, and measured across the matrix.** The KC
leg's hot loop is now instruction-for-instruction the baseline's, and the KC penalty fell
from 2.6–3.5× to 5–14%. `kc_nest` stays default OFF — but now because the *traversal*
loses on this machine, not because its kernel was spilling.
Next step: the box leg (zen3, 512 KB/core L2), which is finally a fair test of the
(jc, kc, ic) order; and conv2d row blocking, whose mechanism this session measured.
Resume command/check: `docs/performance/matmul/s29.md` (S30 addendum at the end).

## 1. Work completed

- `plan-s30-vector-accumulators.md` written pre-build (model-first, §6.1).
- Implemented in `crates/backends/llvm/src/func.rs` (emitter-local; flow-ir untouched, per
  ADR-0032): `tile_vec_llt`, `emit_vec_splat`, `emit_tile_b_vector`, `emit_tile_vec_step`,
  `emit_tile_vec_k_loop`, `emit_tile_vec_out_ptrs`, `emit_tile_trio_vec`,
  `emit_tile_kc_trio_vec`, plus gated early returns in `emit_tile_trio` / `emit_tile_kc_trio`.
- Measured the full before/after matrix (before = a pristine `git worktree` at the S29
  commit, so the comparison is a real A/B and not a memory of one).
- Validated the result in the disassembly of every kernel, not just the one that was fixed.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Accumulator element type | `<TJ x elem>` — the TILE width, not a machine width | LLVM legalizes it per target (4× `q` on NEON), so the emitter stays target-independent and `TILE_J` keeps its one meaning |
| Gate for the vector path | `main && rows == TILE_I && bound == TILE_J` | The plan's stated gate (`main && bound == TILE_J`) is **insufficient**: `emit_tile_row_split_j` and the 1-D rung both reach the trio with `main` set at `rows == 1`. Caught during the build; the plan's rule 2 was the intent and this is what implements it |
| Remainder/boundary/TI=1 rungs | keep the memory form | Their lane count is genuinely runtime; masked vector ops are the upgrade. Verified byte-identical, so they are a true negative control |
| KC park/reload | became one vector load/store per subrow | Leaving them as scalar lane loops through the acc scratch would have re-created the exact aliasing that stopped the promotion in the first place |
| Vector access alignment | explicit `align 4` / `align 8` everywhere | The ABI alignment of `<16 x float>` is 64 and `j0` offsets are arbitrary — this is a miscompile, not a slowdown, if it is wrong |
| `kc_nest` default | **stays OFF** | Now for a measured reason: with both legs fair the traversal still loses at every size and the deficit grows with N |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test --workspace --release` | **72 suites, 0 failed** (run independently after the agent's own run) | R1 holds — the differential suite is byte-equality vs untiled and vs the interp oracle at -O0/-O2, both nests, f32/f64, FLOW_PAR splits |
| matmul before/after, FLOW_PAR=1, min-of-3 | KC-on: 1024 f32 **59.90 → 21.72**, f64 **158.35 → 45.18**, 2048 f32 **488.54 → 176.30**, 4096 f32 **4096.98 → 1564.14**. KC-off: 20.18 → 20.64, 41.62 → 42.33, 167.88 → 163.53, 1332.20 → 1373.74 | 2.6–3.5× on the broken leg; the shipping leg unchanged within noise |
| `str q…,[sp]` in the tiled task | KC-on **92 → 0**; KC-off 0 → 0 | the accumulators no longer touch the stack |
| `ccmp` runtime alias checks | KC-on **8 → 0** | no address left to disambiguate, so LLVM stopped versioning the loop against a scalar fallback |
| hot-loop anatomy (disasm) | matmul KC-off and KC-on both **51 instrs / 32 fmla / 4 vec loads / 0 vec stores** (FMA:mem 8.00); fir 62/32/8/0 (4.00); conv2d 93/36/24/4 (1.29) | the two matmul legs are now the same kernel; **no accumulator anywhere spills** |
| fir/conv2d emission before vs after | **byte-identical** (`cmp`) | the unconverted rungs are a genuine negative control |
| attn_256 | emission changed (matmul-shaped rung, 16 vector phis), stdout identical | takes the new path correctly; too small to time meaningfully |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / cleanup |
| --- | --- | --- | --- |
| branch | `main` | S30 committed on top of S29's four commits | `git log --oneline -6` |
| worktree | `scratchpad/before` @ `3154524` | the A/B baseline tree | `git worktree remove` when done — REMOVED at close |
| artifacts | `scratchpad/matrix/*` (.ll + binaries, both trees) | disposable — every number is in s29.md's S30 addendum | delete anytime |
| other session | `VISION.md`, ADR-0033…0036, thesis-review note, `docs/suggestions.md` | still uncommitted, still not mine | leave alone |

## 5. Open items

| Priority | Item | Doc/code reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | The box leg — now a fair test of the traversal | suggestions #16; s29.md S30 addendum | on-demand zen3: `kc on/off × {1024,2048,4096} × {f32,f64}` via `--kc` | `kc_nest` default settled with a number from the machine it was designed for |
| P1 | conv2d row blocking | suggestions #11 (mechanism now measured) | TI over output rows — 6 image rows serving 4 output rows instead of 3 serving 1 | conv2d 1024 FMA:mem well above 1.29; the 3.4× loss closes |
| P2 | Masked vector remainder tiles | plan-s30 Ceilings | masked `<TJ x elem>` ops for runtime-`tj` tiles | remainder tiles leave the memory form too |
| P2 | An automated spill gate | plan-s30 Tests | the IR-level proxy landed (phi/load/store + acc-GEP counts in the goldens); a disasm gate is AArch64-specific | a regression of this class fails at build time, not at benchmark time |

## 6. Architecture / model changes

- **`DataLoc` (backend-llvm):** the tile accumulator moves from the stack frame to the
  register file *by construction*. Same `Dat`, same per-cell morphism chain, same k order
  — R1-neutral, which the differential suite enforces rather than assumes.
- **A methodological fact worth keeping.** The S29 regression happened because the fast
  form was *granted* by an optimizer heuristic rather than emitted. The emitter's blanket
  "every object is an alloca" rule is a correctness convenience that silently delegates
  performance to LLVM's ability to undo it — and that ability is not monotone in the
  amount of unrelated code nearby. Where the emitter knows the size at compile time and
  controls every access, it should emit the value form. This is the narrow, justified
  carve-out; suggestions #10's whole-emitter version remains deferred.

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/components/backend-llvm/plans/plan-s30-vector-accumulators.md` | new (model-first, pre-build) |
| `docs/performance/matmul/s29.md` | S30 addendum: the before/after matrix, the hot-loop table, where KC pays off (nowhere here, and the deficit grows), conv2d's measured mechanism |
| `docs/components/backend-llvm/suggestions.md` | #16 struck as APPLIED (codegen half) with the numbers, and the traversal question restated as what remains; #11 gains its measured mechanism |
| `docs/components/backend-llvm/STATUS.md`, `docs/STATUS.md` | S30 headers |
| `docs/next-session.md` | S31 queue |
| this log | new |

## 8. Files changed

Bench (added after the first draft of this log, on Sapir's request for a full comparison):
`benches/matmul/gen_flow_capture.py` now brackets the kernel map with `time` and prints
`iter ms=`; the eight 512–4096 × f32/f64 cap sources regenerated; new
`benches/matmul/matmul_ab.sh` — the full CPU comparison harness (flow conf/fma × par/1t,
cpp/rust 1t/mt, numpy 1t/threaded). **FLOW_PERF is now retired for matmul as well as
shapes** — the queued second half of plan-time-builtin item 7. Results in
`docs/performance/matmul/s29.md`: Flow beats every naive-class baseline at every size and
both widths with the margin GROWING in N (11× → 192× over cpp-mt at f32; 38× → 153×
single-thread), while Accelerate/AMX numpy stays 3–9× ahead on matmul alone — and Flow is
4–16× ahead of numpy on fir and conv2d, where no hand-written BLAS kernel exists.

Code: `crates/backends/llvm/src/func.rs`. Tests: `crates/backends/llvm/tests/golden_ll.rs`
+ 3 deliberately re-pinned snapshots (`tile_nest_shape`, `tile_nest_shape_f64`,
`tile_nest_shape_kc`); the 1-D and conv snapshots did NOT move. Docs: as §7.
