# Next Session (S44)

Written: 2026-07-31 · end of S43 · by: Claude (orchestrator; category-architect skill)
Session log: `sessions/2026-07-31-s43-the-pack-was-serial.md` — **read it**
Previous: S42 (`sessions/2026-07-31-s42-the-constant-that-cost-a-session.md`), S41b, S41.
The performance record that governs S44: **`docs/performance/s43-residency-and-the-thermal-artifact.md`**.

## READ THIS FIRST

**The headline: Mapal beats numpy/Accelerate at N=4096 f32 threaded — 1.139×, disjoint, values
identical, measured same-session.** First time on any matmul cell. **MERGED and pushed** at
`555d058`; the gate was re-run on the merged tree (1032 passed / 0 failed, fmt clean).

**Scope it honestly.** 4096 wins 1.139×; 2048 is **parity** (overlapping); 1024 **loses** 1.21×; one
thread loses ~2× at every size. The claim is "the large-N threaded cell", never "matmul".

**S42's `1864` L1 ceiling is RETRACTED** — a thermal artifact; the same binary reads ~2000 today.
So is `1043 GF/s` as the N=4096 figure (it is 803). Anything downstream of those is suspect.

**Before trusting `benches/emit_sweep_ab.sh`, note it was silently broken until S43.** Under bash it
passed no flags and exited 0 with a clean "159/159". It is fixed and now hard-errors on a failed
emission — which immediately revealed that **3 of its 159 cells have always failed**
(`examples/vector.mapal` does not parse). Every historical "159/159" was 156 real cells + 3 vacuous.

**Every timed run goes through `benches/perflock.sh`.** It is bounded (exit 75 = retry, command did
not run) and refuses to measure through a busy machine.

## S44 opens on — the single-thread gap

### 1. What shipped in S43 (context, not work)

On `main` at `555d058`. Three files, +139/−6, `mapal-ir` and `mapal-rt` untouched.
Plan: `components/backend-llvm/plans/plan-s43-parallel-bpack.md`.

- `func/core.rs::emit_pack_copy` — the `jt` loop's bounds become `self.bulk_bounds(tiles)`; at
  `split_range == false` it reproduces the old literals character-identically.
- `func/drive.rs::emit_task` (packed branch) — a third function `@task{id}_pack(lo, hi, frame)`; the
  wrapper drops the inline pack for a nested `begin(1)/task/launch/finish` ahead of the **unchanged**
  matmul dispatch.

It moved **48 emissions** — only matmul sources, only `rew`/`con`, never `raw`. If you touch this
path again, remember a 159/159 identical result means your change did nothing: gate on **values**.

**Do not "simplify" it into a `begin(2)` + `mapal_par_dep`.** That was tried and rejected:
`complete_slice` schedules dep-unlocked tasks `Placement::Local(lane)`, which would put every matmul
slice on one deque instead of the rank-sorted `Placement::Seed` — silently changing the *matmul's*
placement. Two sequential nested runs are deliberate.

**Still undecided: `nc` blocking** (worktree `a03f9b2318`) — built, swept, gates green, **ships
OFF** because threaded it is parity at best. Merge as a documented lever or discard.

### 2. P1 — the single-thread gap, which is now the honest target

One thread we are ~800 GF/s against numpy's ~1640. **The 1.71× operand-residency win is real,
assembly-verified, and unclaimed** — but `kc`, `nc` and the L1 cascade have all been measured and
refuted as ways to get it:

| lever | 1 thread | threaded |
| --- | ---: | ---: |
| `kc` blocking | +6.1% | −25.5% |
| operand residency (window instrument) | **+71%** | +5% |
| `nc` blocking | +18.7% | parity |

What the machine actually charges for (§4b/§4c of the perf doc): **nothing** for L1-vs-L2; a real
price for falling out of the **16 MB shared L2**; and a **1.571× penalty for crossing ~2k–4k pages**
of TLB reach. The 1.71× is confounded between those last two and **no arm in the design separates
them** — an in-kernel arm was refuted on arithmetic (`inbounds` bounds it to ≤32 pages).

So a design that captures it must change the *reuse structure*, not add another blocking level.
Note the asymmetry that makes this hard: at one thread there is no Amdahl problem to fix, and every
working-set knob has now been swept.

### 3. Measurement rules — S43 added four

> **19. Re-run the baseline binary before trusting a published table.** S42's L1 ceiling was thermal
> drift *during* the run; the 64 MB row of the same table still reproduces exactly. A table can be
> half-valid. A number never re-taken has never been checked.

> **21. Name every mechanism that predicts your table, not just the one you were testing.** Cache
> reach and TLB reach predicted the residency arms identically. An experiment that cannot distinguish
> two mechanisms has established neither.

> **22. A sweep needs a control arm that should NOT move.** A zero-load arm tracked the 4-load arm
> exactly down a fake cliff — drift on the swept axis. **Rep-outer interleaving is not sufficient**:
> every rep walks the axis in the same order, so a within-rep droop survives best-of-N. Measure the
> null arm back-to-back inside each cell and read the ratio.

> **23. A gate that cannot fail is not a gate.** Verify the instrument reports a failure you
> *injected* before trusting its pass. Two independent silent-pass paths lived in this repo's
> byte-identity gate; one was live.

And rule 4 (sweep, never one point) earned it twice: both cache walls prescribed `nc` ≤ 512, and
`nc`=512 **lost** at both widths while 1024 won by 18.7%. **Walls size the benefit; re-sweeps size
the cost.**

## FIRST commands

```sh
cargo test --workspace --release                    # 1032 passed / 0 failed on main
./benches/perflock.sh ./benches/matmul/numpy_ab.sh target/release/examples/emit 4096 15  # the headline
git worktree list                                   # 4 agent worktrees + 3 prunable pre-rename
```

## Live state at S43 close

| Type | Handle | State | Inspect | Cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` @ `555d058` | **pushed, in sync, clean** (only `oainotes.md` untracked) | `git status -sb` | — |
| worktree | `agent-a718d8faeee0ea4b4` | parallel B pack — **already merged to `main`** | `git -C … status -s` | discardable |
| worktree | `agent-a03f9b23183f1440c` | `nc` blocking, ships OFF, gates green | " | merge or discard |
| worktree ×2 | `agent-a00e8357…`, `agent-a9c8b56e…` | probe-only; sources already copied to `main` | " | discardable |
| worktree ×3 | `…-Personal-**Flow**/…` | prunable, pre-rename paths | `git worktree list` | `git worktree prune` — **only after the agent worktrees resolve** |
| machine | Arch box `100.81.226.103` | up, i9-14900F, **no SME** | `ssh … nproc` | owned box |
| artifact | box `~/mapal-s42/` | **107 MB, still there** | `ssh … 'du -sh ~/mapal-s42'` | delete when done |
| file | `oainotes.md` | untracked, deliberately uncommitted | — | Sapir's call |

**Nothing is running.** No background job, no server, no port.

## Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | **single thread is ~2× behind numpy** | §2 | needs a reuse-structure change, not another blocking level | 1t GF/s moves off ~800 |
| P1 | decide `nc` blocking's fate | §1 | merge as a documented lever or discard | resolved |
| P1 | `examples/vector.mapal` does not parse | READ THIS FIRST | 3 of 159 gate cells have always failed | it parses, or leaves the sweep |
| P1 | delete or justify `kc_nest` | `lib.rs::EmitOpts` | unchanged from S42 | gone, or has a written reason |
| P1 | executing SME value check in `cargo test` | `benches/sme/README.md` | unchanged from S42 | the suite runs an SME binary |
| P2 | box scratch `~/mapal-s42` (107 MB) | live state | delete when box work is done | gone |
| P2 | 3 prunable pre-rename worktrees | live state | prune after agent worktrees resolve | only `main` listed |
| P2 | f16/bf16 rung (2× MAC density) | S42 §5e | plan first; it is a new face | `svmopa_za32_f16_m` emitted |

## Standing direction (Sapir — unchanged)

- Compute-only legs; numpy in every verdict table; scale everything up.
- Parallel-first by construction — **threaded is the configuration that ships.**
- Backend-genericity contract (ADR-0032): mapal-ir never learns machine facts.
- **Keep stuff generic, not hardcoded** — and when a constant is swept rather than derived, say so.
- Query, not rewrite: record that something *could* be skipped; never delete it.
- Compile time decides the SIZES, runtime decides the ASSIGNMENT.
- Nothing goes in the README that a default build does not deliver.
- Proof over suggestion — a change arrives with the measurement of what it did.
- Speak simply, base claims on empirical results. **Give absolute ms, not only ratios.**
