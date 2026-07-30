# 2026-07-30/31 — S42: the constant that cost a session

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Driven by Sapir.
Continues `2026-07-29-s41b-every-za-tile-and-a-refuted-reduction.md`, which opened S42 on k-loop
software pipelining.

## 0. Continuation brief

Current state: **S42's stated P0 was refuted in the first hour, and the session's real output arrived
in its last: the SME gap is operand cache residency, worth ~1.79× — enough to pass Accelerate.**
Along the way one wrong constant (`sme_kc` = 512 instead of the swept 1024) made KC blocking look
like a 1.27× loss and sent six investigations down the wrong road. The genericity work shipped
cleanly: `f32_tiles` derived away, SVL/L1D/L2 detected from sysctl, SME addressing derived from
recorded facts, the A pack reordered. Gate **1031 passed / 0 failed**, fmt clean, **159/159 +
636/636 emissions byte-identical**. Nothing committed.

Next step: **the hierarchical tile→cache mapping (Sapir's direction)** — registers → L1 → L2 → L3,
one tile per level. See `next-session.md` §1, which carries the concrete first move.

Resume command/check: `cargo clean && cargo test --workspace --release` (the clean is not optional —
see §3), then read `docs/performance/s42-sme-roofline.md` §5e.

## 1. Work completed

**A. Genericity (Sapir's directive: "keep stuff generic not hardcoded").** Four changes, all proven
byte-identical:

- **`f32_tiles` DELETED.** ZA holds exactly `sizeof(elem)` tiles at a given width — 4 at f32, 8 at
  f64 — an ISA rule, not a recorded fact, so recording it could only ever make it wrong. Derived in
  `sme_block`. This also closed the S41b P2 defect where a 1..=64 sweep pinned arrangements no ZA
  register file has.
- **Detection.** `--target=native` reads `hw.optional.arm.sme_max_svl_b`, `hw.perflevel0.l1dcachesize`
  and `l2cachesize`. SVL turned out to be a plain sysctl, so no streaming-mode probe and no `+sme`
  build of the emitter is needed. `native` emits **byte-identical IR** to the hand-written
  `apple-m4-sme`, and a test asserts the two agree. Named profiles resolve first, so a hand-written
  profile remains the override and the cross-compilation case.
- **SME b-addressing derived from recorded facts.** Three `t`s and a `1` were standing in for
  `profile.tile_j` and `site.b.clane`, correct only by coincidence on this part. 8/8 SME emissions
  byte-identical, so the substitution is provably pure.
- **A-pack reordered row-outer.** The old k-outer/row-inner order needed `ti·t` simultaneous row
  pointers; blocked, LLVM spilled them into the innermost loop. Scalar float loads 51 → 5. Worth 3%.

**B. KC blocking built** (`PanelWrite::{Store,Accumulate}`, a k-block loop, two emitted kernels).
Values identical to the NEON leg at every size and depth. **Default OFF.**

**C. The measurement campaign** — nine probes, most of them refutations. See §3.

**D. The box** — first cross-machine run since S38. Cross-compiled on the Mac, linked with `gcc`
there. Closed the S29 open item.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| k-loop software pipelining (the stated P0) | **refuted** | +0.1–0.2%, overlapping, 3 sizes, both kernel layouts |
| `f32_tiles` as a recorded field | **deleted** | derivable from the ISA; recording it can only make it wrong |
| detect SVL by running an SME probe | **unnecessary** | it is a plain sysctl (`sme_max_svl_b`) |
| `native` detection shadowing named profiles | **rejected** | named profiles resolve first; hand-written is the cross-compilation case |
| `sme_kc` = L1D exactly (ratio 1) | **WRONG, measured** | 0.785×; the swept optimum is ratio 2 |
| writing `2 * l1d_bytes` into `sme_kc` | **rejected** | a fitted constant in a derivation's clothes; recorded as `panel_l1d_ratio`, a documented policy ratio |
| enable KC by default | **rejected** | +6.1% at 1 thread but **−25.5% threaded**, and threaded ships |
| cap SME lanes to the unit count (2) | **rejected** | more lanes never hurt; every cap is a throughput trade |
| exclude E-cores from SME tasks | **rejected** | they add 1.7%, so excluding them loses 1.7% |
| keep `kc_nest` because "unmeasured on the box" | **retired** | swept on the box; loses at every depth |

## 3. Tests, checks, benchmarks

| Check | Result |
| --- | --- |
| `cargo test --workspace --release` | **1031 passed / 0 failed** (1026 → 1031, +5 profile tests) |
| `cargo fmt --all --check` | clean |
| emission A/B, 53 sources × 3 faces | **159/159 byte-identical** |
| …× generic/apple-m/zen3/cuda-ada | **636/636 identical** |
| SME emission, geometry refactor | **8/8 byte-identical** (4 sizes × packed/unpacked) |
| `--target=native` vs `--target=apple-m4-sme` | **byte-identical** at 1024 and 4096 |
| SME vs NEON values | identical at 512/1024/2048/4096, every KC depth |
| cross-ISA values (M4 vs i9) | **identical** — `74348 -302529` at 4096 |

### The `fmopa` roofline and the unit count

`benches/sme/units.c` — N threads, `fmopa` on registers, **zero memory traffic**:

| threads | aggregate GF/s | per-thread | fastest ms | slowest ms |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 1997.8 | 1998 | 123 | 123 |
| 2 | **3849.6 (1.93×)** | 1925 | 128 | 128 |
| 4 | 4173.1 | 1043 | 129 | 236 |
| 14 | 4076.1 | 291 | 639 | 844 |

**Exactly 2 units, ~2000 GF/s each, ~4100 aggregate.** Flat from 3 threads on, `per_thread × n ≈
4100` throughout. Replaces S41b's inference from scaling ratios. *(The E-core half of that probe is
inconclusive — `BACKGROUND` matched `USER_INTERACTIVE` per thread, which by the probe's own criterion
means the QoS steering did not take.)*

### THE finding — operand cache residency, ~1.79×

`benches/sme/loadcost.c`, compute held exactly constant (4 `fmopa`/iteration in every row):

| operands from | 0 loads | 1 | 2 | 3 | 4 loads |
| --- | ---: | ---: | ---: | ---: | ---: |
| 32 KB buffer (L1-resident) | 1956.7 | 1913.7 | 1928.9 | 1910.0 | **1864.2 (95%)** |
| 64 MB buffer (past L2) | 1915.5 | 941.8 | 841.8 | 755.3 | **760.8 (40%)** |

**Loads cost 5% when they hit L1 and halve throughput at the first miss.** The load *count* is nearly
free ⇒ 1-load-per-`fmopa` is **not** a ceiling, retiring the "4 ZA tiles cap the ratio" theory.

| | GF/s |
| --- | ---: |
| emitted kernel today | 1043 |
| Accelerate 1 thread | 1655 |
| **4 loads, L1-resident** | **1864** |

### The depth sweep — the constant that was wrong

| working set | kc | N=4096 | N=2048 |
| ---: | ---: | ---: | ---: |
| 64 KB | 256 | 0.501× | 0.387× |
| **128 KB ← what `sme_kc` derived** | 512 | **0.785×** | 0.639× |
| **256 KB ← swept optimum** | 1024 | **1.064×** | 0.986× |
| 512 KB | 2048 | 1.027× | 1.000× (unblocked) |

At the corrected depth, 15 alternating runs, values identical:

| N | config | KC off | KC on | | dist |
| ---: | --- | ---: | ---: | ---: | --- |
| 2048 | 1 thread | 18.014 ms | 17.751 | +1.5% | overlap |
| 2048 | threaded | **6.783** | 7.779 | **−12.8%** | disjoint |
| 4096 | 1 thread | 171.179 | **161.360** | **+6.1%** | disjoint |
| 4096 | threaded | **53.485** | 71.796 | **−25.5%** | disjoint |

### Six causes investigated at the wrong depth — all refuted, all wasted

| candidate | verdict |
| --- | --- |
| the loop nest is wrong | no — verified index by index, work counts exact |
| the b layout | 1.065× (`bslice.c`) |
| the read-out code | no — 4 instructions per tile, no spills |
| the streaming-mode ABI (`_body` + `d8–d15`) | 1.0 ms over 131072 calls (`smcost.c`) |
| the pack's memory order | none — same loops in C: 8.46 vs 8.05 ms (`packcost.c`) |
| the pack's register spilling | real, **fixed**, worth 3% |

### The box — i9-14900F, S29's open item closed

24C/32T, 48 KB L1d, **2 MB L2/core**, 36 MB L3, AVX2 (no AVX-512), governor `performance`.
`tile_kc = l2_bytes / 8192` for `zen3` f32. Swept by varying `l2_bytes`, N=4096:

| depth | blocks | 1 thread | threaded (32) |
| ---: | ---: | ---: | ---: |
| unblocked | 1 | **926.759 ms** | **112.919 ms** |
| 2048 | 2 | 0.548× | 0.595× |
| 1024 | 4 | 0.557× | 0.759× |
| 512 | 8 | 0.551× | **0.771×** best |
| 64 | 64 | 0.454× | 0.668× |

**A step function, not a curve** — any blocking costs ~1.8× at 1 thread, ~1.3× at best threaded, at
*every* depth. Unlike the Mac there is no optimum to find.

Incidental: the box's untuned vector path hits **148.3 GF/s at 1 thread** (~71% of its AVX2 FMA
peak), and threaded at 4096 it does **1217 GF/s** against the M4 Pro's **2570** with SME — the laptop
with a matrix unit is **2.1×** the desktop without one.

### The stale-cache trap

12–18 tests failed under `--workspace` and passed under `-p <crate>`; the failing set moved between
runs. Cause: **521 artifacts in `target/release/deps` still contained the pre-S34
`/Personal/Flow` path**, baked into `env!("CARGO_MANIFEST_DIR")` at compile time. `cargo clean`
(282,153 files, 17.9 GiB) fixed it. **Not a code regression** — but it cost real time and will
recur for anyone with a pre-rename cache.

## 4. Live handoff state

| Type | Handle | State | Inspect | Cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` @ `f01fb73` | **ahead 1, UNPUSHED**; 5 modified + 14 untracked | `git status -sb` | Sapir's call |
| commit | `f01fb73` (another process: interp `Rc`, suite 450s → 28s) | committed, unpushed | `git log --oneline -1` | `git push` |
| git identity | `Sapir Shemer <lesscomplexity@gmail.com>` | **set** | `git config user.email` | — |
| network | github.com | reachable (200) | `curl -sS -m5 https://github.com` | — |
| worktree ×3 | `…-Personal-**Flow**/…/{pre-s38,wt,pre}` | **all prunable** (pre-rename paths gone) | `git worktree list` | `git worktree prune` — safe |
| machine | Arch box `100.81.226.103` i9-14900F | up, governor `performance`, **no clang** | `ssh 100.81.226.103 nproc` | owned box, nothing to stop |
| artifact | box `~/mapal-s42/` | **107 MB, 40 files** — objects, binaries, `libmapal_rt.a`, `sw.sh`, `an.py` | `ssh … 'ls ~/mapal-s42'` | **delete when done** |
| file | `oainotes.md` | untracked, deliberately uncommitted | — | Sapir's call |
| data | session scratchpad | probe binaries, hashes, sweep logs | — | session-local, will vanish; everything load-bearing is in the tree |

**Nothing is running.** No background job, no rented machine, no server, no port.

## 5. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | **Hierarchical tile→cache mapping (L1→L2→L3)** | `next-session.md` §1 | **First verify the emitted kernel really misses L1**, then design the cascade with a per-level derivation | 1t GF/s moves off 1043 toward 1864 |
| P0 | commit/push S42 and `f01fb73` | §4 | Sapir's call | `git status -sb` clean |
| P1 | delete or justify `kc_nest` | `lib.rs::EmitOpts` | lost on every machine, swept | gone, or has a written reason |
| P1 | executing SME value check in `cargo test` | `benches/sme/README.md` | wire into `differential.rs` with `-march=armv8-a+sme2` | the suite runs an SME binary |
| P1 | f16/bf16 rung — 2× MAC density | `next-session.md` §3 | plan first; it is a new face, not a tailor | `svmopa_za32_f16_m` emitted and value-gated |
| P2 | box scratch `~/mapal-s42` (107 MB) | §4 | delete when box work is done | gone |
| P2 | 3 prunable worktrees | §4 | `git worktree prune` | only `main` listed |
| P2 | does the `fmopa` port saturate at 4 chains | `s42-sme-roofline.md` §1 | needs a clock measurement | answered or dropped |

## 6. Architecture / model changes

**No `mapal-ir` change. No `Dat`/`Trn` change.** Everything is backend `Loc` facts and derivations.

**One new atom distinction, and it is Sapir's.** The SME rung treats the operand window as a single
`DataLoc` sized against one cache. The measurement says a matrix unit's operands live in a
**hierarchy** of locations — register file, L1, L2, L3 — and a tile at each level. In FRAMEWORK terms
that is not one `DataLoc` but a **chain of `DataLoc`s over one `Dat`**, each with its own extent, and
the `Trm`s between them are the tile swaps. Today's single `kc` collapses that chain to one link,
which is why it can be resident at one level and thrash the rest. Building the cascade is the S43 P0.

**`Sme` gained and lost fields.** `f32_tiles` removed (ISA rule, derived). `l1d_bytes` added inside
`Sme` rather than beside `l2_bytes`, so a profile cannot declare a matrix unit without its
working-set budget — FRAMEWORK §4.5 law 3 (placement totality) enforced by the type. `panel_l1d_ratio`
added and **explicitly documented as a policy ratio, not a machine fact**, joining `acc_vecs_per_row`
and `nc_tiles` in the category ADR-0034 would search.

**`runsAt` as a relation, again.** `sme_kc` exists because one deduced k-depth cannot serve two
placements with different budgets — the NEON core and the streaming matrix unit. Two `TrnLoc`s over
one question.

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `performance/s42-sme-roofline.md` | **new** — the whole campaign; §5e is the finding, §5c/§5d rewritten after the depth sweeps, §5/§0 corrected, rules 17/18 added, §8 lists every retraction |
| `components/backend-llvm/plans/plan-s42-sme-kc-blocking.md` | **new** — written pre-build; superseded by results, see §8 of the perf doc |
| `benches/sme/README.md` | S42 probe table, the two traps, the standalone-probe caveat |
| `next-session.md` | rewritten for S43 on the hierarchical mapping |
| `crates/backends/llvm/src/profile.rs` | `panel_l1d_ratio` doc carries both depth sweeps |
| `crates/backends/llvm/src/lib.rs` | `kc_nest` doc carries the box sweep; S29's open item marked closed |
| `crates/backends/llvm/src/func/sme.rs` | the six refutations, the two retractions, and the corrected-depth table at the decision site |
| this log | new |

## 8. Files changed

`crates/backends/llvm/src/{profile,lib,module}.rs` · `crates/backends/llvm/src/func/sme.rs` ·
`benches/sme/{bp,bslice,kc,loadcost,mm4p,mv,packcost,pipe2,roofline,smcost,units}.c` ·
`benches/sme/sme_pack_ab.sh` · `benches/sme/README.md` · docs as §7.

## 9. Method notes earned

- **Sweep the parameter before judging the optimization.** One constant — `sme_kc` = 512 — produced
  four "KC loses" write-ups and six wasted investigations. The sweep that overturned it was the
  simplest experiment available and was run last. (Rule 17.)
- **A probe that neutralises part of a kernel must be checked in the emitted code.** Forcing `K=1`
  left the ZA read-out intact and produced two confident wrong attributions, both retracted.
  (Rule 18.)
- **When a hand-written version is faster than the emitted one, the defect is ours — say so and go
  find it.** Sapir's correction. It was the turn that broke a loop of three "KC is dead" conclusions,
  and it was right: same structure, same machine, 124.8 ms hand-written against 225.9 emitted.
- **Give absolute milliseconds, not ratios alone.** Sapir, twice. `0.547×` is ambiguous about
  direction and hides the magnitude; the `×` column also let a wrong GFLOP/s conversion (783 where
  626 was correct) survive until Sapir checked the arithmetic.
- **A synthetic roofline is not a competitor.** ~4100 GF/s is `fmopa` with no memory at all; nothing
  reaches it and nothing should. Quoting "62% of the roofline" as if it were a gap to Accelerate
  conflates a machine-utilisation figure with a competitive one — caught by Sapir.
- **`cargo clean` before trusting a red gate on a renamed repo.** Absolute paths are baked into test
  binaries at compile time; a stale cache fails in a way that moves between runs.
