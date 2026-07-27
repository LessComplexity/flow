# Next Session (S39)

Written: 2026-07-27 · end of S38 · by: Claude (orchestrator; category-architect skill)
Session log: `sessions/2026-07-27-s38-trap-order-is-source-order.md`.
Previous block: S36/S36b/S36c/S36d, S37/S37b (`sessions/2026-07-27-s3[67]*.md`).

## READ THIS FIRST

**The gate is GREEN — 981 passed, 0 failed** — for the first time since S33. LLVM differential
**37/37 in 403.93 s** at `-O0`/`-O2`, `cargo fmt` clean, zero pending snapshots. Committed on `main`,
**not pushed**.

## Where things stand (≤6 lines)

**Trap order is now source order.** `topo_order` breaks ties on `(loc.start, loc.end, insertion
index)` instead of arena insertion order, because trap order is observable and insertion order is a
property of the compiler. The S37 P0 (`open_inline`) is closed. The bug was **wider than the plan
said**: the same cause let a trap swallow output written before it — PRE prints nothing, POST prints
`111\n222\n`, both exit 101, and the 1,280-run sweep is blind to it by construction because it
discards stdout on trapping runs. `SourceLoc` is now a **semantic** attribute, not debug metadata.

## FIRST commands

```sh
git log --oneline -3                                    # expect the S38 commit on main
git status --short                                      # expect empty
cargo test --workspace --release --no-fail-fast 2>&1 | grep -E "FAILED|test result"
git worktree list                                       # 3 stale entries — prune
```

## S39 focus

### 1. P0 — the GPU leg, via LLVM NVPTX (Sapir's call, taken on evidence)

An 8-agent audit recommended keeping the CUDA C emitter. Sapir challenged it; the audit's own
most load-bearing claims were flagged unverified, and a 15-minute probe **refuted them**
(LLVM 22.1.8, `llc -march=nvptx64 -mcpu=sm_80`):

| | |
| --- | --- |
| smem | `addrspace(3)` → `.shared .align 4 .b8 tileA[1024]`, `st.shared.b32` |
| kernel marking | `ptx_kernel` calling convention → `.visible .entry` |
| **tensor cores** | `llvm.nvvm.mma.m16n8k16.row.col.f32.f32` → **`mma.sync.aligned.m16n8k16.row.col.f32.f16.f16.f32`**; **804** mma/wmma intrinsics, incl. MXFP block-scale |
| `<16 x float>` | 16 scalar `fma.rn.f32` — the *correct* GPU lowering: per-thread register blocking, not SIMD lanes |

Probe files kept: `…/scratchpad/nvptx_probe.ll`, `mma_probe.ll`.

**Model-first (§6.1): write the plan before any code.** The open design question is Sapir's own
framing — *the GPU may need graph facts supplied differently, and that is the point*: it turns
ADR-0033 D4(b) ("which machine fact does the record not carry") from an assertion into a result. One
instance is already known: `<TJ x elem>` means SIMD lanes on CPU and per-thread registers on GPU —
one record field, two readings.

Do NOT re-derive these, they are settled:
- **NVPTX violates nothing.** FRAMEWORK §4.2 sanctions one `Trn` at two `Loc`s as "different code …
  the strategy shape", and §5 lists backends among pluggable variants. The earlier "two emitters are
  a §3/§5 drift farm" framing was the orchestrator's and was **wrong**.
- **NVPTX does not build the smem rung for you.** `grep '__shared__|__syncthreads|dim3'
  crates/backends/cuda/src/` returns **zero** — the rung is greenfield in either language.
- **CUDA C cannot express ADR-0032 D1's per-region precision lattice** (`-fmad` is one TU-wide nvcc
  flag). That gap is why NVPTX was always going to be forced.
- **Cheap check before writing emission:** does `tile_plan` recognize a site on the IR the GPU path
  receives? `backends/llvm/tests/tile_sites_pin.rs` pins **raw=0, rewritten=1** — no site is
  recognized before `rewrite` runs, while `golden_cu.rs` emits from `lower_src` alone. If recognition
  is entirely downstream of the rewriter, the blocker is **lowering**, not emission.
- **Unverified, 20 minutes to close:** `TileRead.clane` semantics were never traced into `tile_plan`'s
  recognizer. If a condition assumes lane-contiguity *in a register*, that is a genuine record defect
  and it lands mid-leg.

### 2. P1 — three obligations plan-s38 leaves open

- **Inlining must stamp spliced morphisms with the call-site position** (plan §6.1). A callee's
  morphisms carry the *callee's* locs, which can sort earlier than the call site, so a trap inside an
  inlined body can still move. The pinned counterexample has an **empty** helper and does not
  exercise it. Needs its own counterexample.
- **`SourceLoc` is a semantic attribute now** (plan §6.2) — deserves an ADR.
- **`mapal_par_trap` is RESUMABLE; `mapal_trap` is not** (`-> !`, exits 101). Every "post-trap state
  is unobservable" argument is valid only for the noreturn form. Today only `parallel_matmul_cap` has
  a par_trap site and its guard outcome is identical — but a store into a caller-visible frame slot
  reordering around a par_trap makes the deferred path observable. One targeted test.

### 3. P1 — the oracle cannot witness pre-trap output

`interp::run` derives `output` from the IoToken's accumulated log and only on `Done`
(`mapal-interp/src/lib.rs:55`): interpreted output is a *value* that dies with an abort, compiled
output is a side effect that survives. So `expect_native`'s `(None, 101)` is **forced**, not lazy, and
any test in that class must pin its expectation as a literal. Decide whether the interp should record
output as an effect instead — or document the divergence as permanent.

### 4. P1 — the oracle clones captured arrays per fold step

Unchanged from S37. `eval.rs:288` `caps.clone()` deep-copies a 73,440-element array 293,760 times;
`differential_tiled_matmul_kc_c540` takes 374 s of the suite's 395 s. Fix is `Rc` + copy-on-write on
`RValue::Array` (46 sites). **Plan it first**, and **the prompt must forbid agents writing into the
repository** — in S37 a planning workflow dropped five `zz_*.rs` files under `crates/*/tests/` and
began migrating the live tree.

### 5. P2 — cheap, already located

- `llvm/src/func.rs:342 packing_site` → rename `packed_layout_admits`, move next to
  `packed_type`/`packed_buffer`. It is a CPU packed-**format** decision wearing a legality
  predicate's name (its own comment admits it) — the only ADR-0032 leak four audits found.
- Two real §5 duplications, both language-independent: ≈28 lines of the type-erasure remap are
  **character-identical** between `llvm/src/ty.rs` and `cuda/src/ty.rs` (the CUDA copy's comment says
  "llvm rule, verbatim"), and the mapal-rt ABI is hand-declared twice (a `declare` block vs a 56-line
  C++ prelude).
- **16 goldens were never panel-adjudicated** — round 3 was stopped. They pass both mechanical sweeps
  (0 of 38 changed tiling/guards/attributes/traps; effect sequence unchanged). Resume:
  `Workflow({scriptPath: …/verify-s38-golden-round3-wf_1f377034-f4d.js, resumeFromRunId: "wf_1f377034-f4d"})`.
- `elem_plan` headroom, `shape-ladder-v2.md` republish, M4 Pro table on an idle Mac — all unchanged
  from S37.

## Perf: emission order IS performance-relevant (the pre-registration is refuted)

i9, 3 passes × 101 alternating, both faces, values byte-identical on all 7 shapes every pass.
Medians only — saxpy's and gather's **minima are unusable** (saxpy's min swung 0.40–0.63 ms between
passes; its median held to 4 significant figures).

- **saxpy 1t +5.3%, replicated three times.** The third pass is a byte-identical rebuild, so it
  doubles as the noise floor (reduce 0.00%, transpose +0.1%, gather −0.9% ⇒ ±1%).
- **conv2d 1t −3…−8% (faster), par +3…+7% (slower).** Opposite signs by thread count.
- **mm1024 +2.6% on conformance, flat (+0.15%) with FMA.** The regression is **face-dependent** —
  running the FMA leg was Sapir's catch and prevented publishing a one-face regression.
- **Mechanism deliberately NOT isolated** (Sapir). Vector-instruction counts are byte-identical
  pre/post (199/199, 295/295, 583/583) so it is scheduling, not codegen. Two candidates remain
  unseparated: `%Frame` member order (derives from graph object order, which `replay.rs:1029` derives
  from `topo_order`) and task interleaving. The latter fits conv2d's sign flip; the former does not.
  **The "%Frame is the cause" claim was withdrawn as over-stated.** S36c's `%Frame` alias barrier
  stays refuted — different claim, and vectorization is unchanged.

**Incidental but publishable-facing: `--contract` is a no-op on 4 of 7 ladder shapes.** saxpy,
reduce, transpose and gather emit **byte-identical IR in both faces**, because contraction flags are
applied only in tile kernels and those four are not tile sites (S35). The README's two-face columns
are, for those rows, one binary measured twice.

## Things that are NOT open any more

- **A′ is refuted, not deferred.** 62.2% of raw lowered objects (484/778 over 14 examples) change
  position; only 9 of 36 functions are already in source order, all 3–9 objects. The deviation is
  systematic — an object's `loc` is the *operator token*, not the sub-expression extent, so
  `(x > 0)` puts `>` at 483 and its operand `0` at 485, with the constant created first. "Make
  lowering create objects in source order" churns every golden in the tree rather than A's 38.
  **Do not re-price it.**
- **Approach B is unsound** and was withdrawn in S37 — see plan §3.1.
- The S36c/S37 refutations stand: the `%Frame` alias barrier, and "halve the differential cross
  product".

## Measurement rules (S37's six, plus S38's four)

1. Interleave the two binaries in one pass, or do not report.
2. ≥50 alternating runs before claiming a sub-10% difference on a sub-millisecond cell.
3. Absolute ms on both sides, and name the baseline commit (Sapir).
4. State which face — **and run both**: mm1024's regression exists only with FMA off.
5. **Ask which statistic is stable before quoting a delta.** saxpy's min moved +4%, +9%, −29% across
   three passes; its median moved +5.2%, +5.3%, +5.3%.
6. **A byte-identical rebuild is a free noise-floor control.** Use it.
7. **State a mechanism only when it is isolated.** Correlation plus "not codegen" does not name a cause.
8. A probe reproduces a pattern, not the compiler's output.

## Standing direction (Sapir — unchanged)

- Compute-only legs; numpy in every verdict table; scale everything up.
- Parallel-first by construction.
- **Backend-genericity contract (ADR-0032):** a rung is either a generic graph fact in a mapal-ir
  query or emitter-local cashing with zero mapal-ir change. mapal-ir never learns machine facts.
- **Three questions, three owners:** *is it legal* is mapal-ir's; *store or recompute* and *does it
  blow the budget* are the backend's.
- **Query, not rewrite:** record that something *could* be skipped; never delete it.
- Type system = precision contracts; backend config = performance tailors.
- Compile time decides the SIZES, runtime decides the ASSIGNMENT.
- Nothing goes in the README that a default build does not deliver.
- **Proof over suggestion** — a change arrives with the measurement of what it did.
