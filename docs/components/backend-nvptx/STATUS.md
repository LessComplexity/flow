# Component: backend-nvptx

Status: **planned — no code.** The component exists because a ratified plan grounds it
(FRAMEWORK grounding rule: a component is real when it maps to code *or an explicit plan*), not
because a crate exists. There is no `crates/backends/nvptx/`, and per plan-s41 §2 there may never
be one — the packaging choice is deliberately open and the default for the leg is a `Machine`
discriminator inside `backends/llvm`.

Last updated: 2026-07-29 · **S41 — plan ratified, step 1 of 8 built and gated.**
Spec references: ADR-0033 (second-consumer proof obligation, D3/D4) · ADR-0032 (backend-genericity
contract, D1/D3/D4/D5) · ADR-0020 (backend emission contract — the differential duty) ·
FRAMEWORK §4.2 (placement is a span), §4.5 law 1 (placement honesty), §7.6 (the physical aspect).
Depends on: ir (`tile_plan`, `elem_plan`, `guard_plan`), backend-llvm (the shared emitter).
Depended on by: nothing yet.

## Where to dig

| Doc | What it holds |
| --- | --- |
| [`plans/plan-s41-the-nvptx-leg.md`](plans/plan-s41-the-nvptx-leg.md) | **the whole component today** — model, the §2.2 gates, the seam decision, toolchain, build order, tests, done-bar |
| `../../../benches/sme/README.md` | the SME probe artifacts and the measured ceiling |
| `../backend-cuda/STATUS.md` | the emitter NVPTX replaces (163 tests green, rung 0, exempt in Gate A until this lands) |

## The model, in one line

The GPU tile kernel is **the same `Trn`** as the CPU tile kernel at a **second `Loc`** — a new
`TrnLoc` row over one transformation (FRAMEWORK §4.2). Nothing in `Dat` or `Trn` changes: no new
`Operation`, no new `Ty` variant, no `validate()` change, **no `mapal-ir` edit**. What is new is
physical: `Loc` gains `SM`/`Gmem`/`Smem`, `Trm` gains `h2d`/`d2h`/`g2s`/`s2r`/`launch`, and the
packed panel gets a second `DataLoc` at `Smem`.

The load-bearing consequence: **`__syncthreads` (`llvm.nvvm.barrier0`) is not an idiom to
remember — it is the morphism that completes `g2s`.** Reading the staged panel before it is
Coherence Law 1 failing (§7.6's read-before-DMA race), so the framework names the missing
transmission before the differential does.

## What is built

- [x] **Step 1 — the machine class.** `Machine::{Cpu, Gpu(Gpu)}` + `TargetProfile::gpu()` + the
      `CUDA_ADA` profile, in `crates/backends/llvm/src/profile.rs`. Gated: 159/159 emissions
      byte-identical, 9/9 profile tests. Every pre-S41 profile pinned `Cpu`; `cuda-ada`'s
      CPU-shaped fields pinned as inherited placeholders, not GPU measurements.
- [x] **The §2.2 gates** (the durable part — these outlive the leg):
      **Gate A**, `crates/mapal-ir/tests/consumer_coverage.rs` — ADR-0033's hand-run grep,
      automated; every backend is `Required` or `Exempt` with a reason *and an end condition*; an
      unlisted backend directory fails; a stale exemption fails; and a self-check keeps the gate
      from passing vacuously. **Gate B**, appended to
      `crates/backends/llvm/tests/tile_sites_pin.rs` — the record's field values snapshotted, plus
      proof it is a pure function of the graph.

## What is not built

- [ ] Steps 2–4: the device module preamble (`nvptx64-nvidia-cuda`, `ptx_kernel`, `addrspace`),
      the thread-index prologue, the naive kernel, the smem rung. **No hardware needed for any of
      them** — `llc -march=nvptx64` is on the dev Mac.
- [ ] Step 5: host launch glue (driver API) replacing the four `mapal_par_*` sites for the GPU
      class. Those four now live in one file (`func/drive.rs`) after the S41 split.
- [ ] Steps 6–8: box bring-up (repo checkout only — CUDA 13.3.1 is already installed at
      `/opt/cuda`), the hardware differential, the one compute-only measurement.
- [ ] `guard_plan` on the GPU. S40 recorded "NVPTX inherits gating from scratch" and S39 that a
      gate has four realizations of which warp divergence is one. **Deliberately out of the S41
      leg** — a gate on a matmul tile site is not what this leg tests.

## Needs work / open questions

| Item | Detail |
| --- | --- |
| **The seam is undecided by design** | plan §2: packaging (own crate vs `Machine` discriminator) is a default, not a verdict. Branch budget ~6 outside the four known sites, and **any** branch inside `emit_morphism` is the trip signal. §8.5 judges it on built code. |
| ADR-0033 D4(a)/(b) | which `TileRead` fields the smem emitter consumes, and which facts it needed that the record does not carry. Predicted: warp size, smem capacity, bank geometry, launch shape — all per-`Loc` capabilities, none of them `mapal-ir` facts. A **negative** answer is a successful discharge (D4). |
| `<TJ x elem>` is one field with two readings | S38's finding: SIMD lanes on CPU, per-thread registers on GPU. Unresolved whether that ambiguity is benign or a genuine record gap. |
| Hardware verification debt | inherited from backend-cuda: no CUDA change since S23 has hardware verification. The box GPU now makes that cheap rather than a rental. |
