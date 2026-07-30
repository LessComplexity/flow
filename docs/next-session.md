# Next Session (S43)

Written: 2026-07-31 · end of S42 · by: Claude (orchestrator; category-architect skill)
Session log: `sessions/2026-07-31-s42-the-constant-that-cost-a-session.md` — **read it**
Previous: S41b (`sessions/2026-07-29-s41b-every-za-tile-and-a-refuted-reduction.md`), S41, S40b.
The performance record that governs S43: **`docs/performance/s42-sme-roofline.md` §5e.**

## READ THIS FIRST

**Gate: 1031 passed / 0 failed · fmt clean · 159/159 + 636/636 emissions byte-identical.**
Nothing shipped moved an emitted byte for any pre-existing profile.

**Work is UNCOMMITTED on `main` @ `f01fb73`** (5 modified, 14 untracked). `f01fb73` itself —
another process's `perf(interp): Rc the array payload — suite 450s -> 28s` — is **committed but
UNPUSHED**. Sapir's call on both.

**BEFORE TRUSTING A RED GATE ON THIS MACHINE, RUN `cargo clean`.** S42 lost time to 12–18 phantom
failures caused by **521 stale `target/` artifacts** still baking the pre-S34 `/Personal/Flow` path
into `env!("CARGO_MANIFEST_DIR")`. Tests that resolve files through it get ENOENT, and `insta`
resolves snapshot paths the same way. `-p <crate>` recompiles and passes; `--workspace` reuses the
stale binary and fails. Symptom: the failing set moves between runs.

## S43 opens on — the finding S42 ended with

### 1. P0 — operand cache residency, and a HIERARCHICAL tile→cache mapping (Sapir)

**The gap is not instructions, not scheduling, not silicon. It is where the operand bytes come
from,** and it is worth **~1.79×** — enough to pass Accelerate.

`benches/sme/loadcost.c` holds the compute exactly constant (four independent `fmopa` into the four
f32 ZA tiles, every iteration) and varies only how many operands come from memory:

| operands from | 0 loads | 1 | 2 | 3 | 4 loads |
| --- | ---: | ---: | ---: | ---: | ---: |
| **32 KB buffer (L1-resident)** | 1956.7 | 1913.7 | 1928.9 | 1910.0 | **1864.2 (95%)** |
| **64 MB buffer (past L2)** | 1915.5 | 941.8 | 841.8 | 755.3 | **760.8 (40%)** |

**Loads cost 5% when they hit L1 and halve throughput at the FIRST miss.** So the load *count* is
nearly free and 1-load-per-`fmopa` is not a ceiling — which retires the "4 ZA tiles cap the ratio"
theory S42 spent hours on.

| | GFLOP/s |
| --- | ---: |
| the emitted kernel today | 1043 |
| Accelerate, 1 thread | 1655 |
| **4 loads, operands L1-resident** | **1864** |

**Sapir's direction, and it is the shape of the work:**

> *"something with the kc is wrong, it should be cache resident and thus make operations faster, but
> seems like it is not on L1 correctly making a lot of cache reloads on L1 and using L2 instead which
> sounds wasteful. The geometric facts about tiling option should map into cache operations
> hierarchically — L1 → L2 → L3 — each residing a bigger tile, allowing to swap tiles as efficiently
> as possible from one cache to another."*

That names the defect exactly. **Today the SME rung has ONE blocking level** (`kc`, sized against one
cache via `Sme::panel_l1d_ratio`). A flat single block cannot be resident at every level; it is sized
for one and thrashes the rest. The classic answer (GotoBLAS/BLIS) is a **cascade**, one tile per
level, each amortising the level above:

```
registers  the 2x2 ZA block          ti·t x tj·t          (have it)
L1         the A micro-panel         ti·t x kc
L2         the A block               mc   x kc
L3 / DRAM  the packed B panel        kc   x nc
```

`mc`/`nc` do not exist in the SME rung at all today — only `kc`. The NEON rung has `nc`
(`nc_tiles`) and `tile_kc`, so the vocabulary is already in `TargetProfile`; what is missing is the
**per-level derivation** and the nest that uses it.

**Do this before writing any of it:**
1. **Verify the diagnosis in the emitted kernel** — that operands really are missing L1. `loadcost.c`
   proves the *mechanism* on a synthetic loop, not that our kernel suffers it. Counting the working
   set says it should (§5e), but S42's lesson is that counting is not measuring.
2. **Sweep every level you add** (rule 17 below). One point per level is how S42 lost a day.
3. Ground each level's size in a **detected** cache fact where one exists — `native` already reads
   L1D/L2 — and record anything swept as a **policy ratio**, never as a fake derivation.

### 2. What is already built and where it stands

**KC blocking: BUILT, correct, default OFF.** `--kc` enables it. Values identical to the NEON leg at
every size and depth. At the swept depth (kc=1024): **+6.1% at 1 thread / −25.5% threaded** at
N=4096. It ships off because threaded is what ships. It captures **6.1% of the 79% §5e says is
there**, which is the case for rebuilding it hierarchically rather than deleting it.

**Do not re-litigate these — S42 measured and refuted all of them:**

| candidate | verdict |
| --- | --- |
| k-loop software pipelining / unrolling | +0.1–0.2%, overlapping, 3 sizes, both layouts |
| folding 4 loads into 2 (`ld1w x2`) | 1.018× — and now explained: loads were never the bottleneck |
| the b layout (whole-k slice vs kc-deep repack) | 1.065× |
| the streaming-mode ABI (`_body` + `d8–d15` spills) | 1.0 ms over 131072 calls |
| the pack's memory order under blocking | none — same loops in C: 8.46 vs 8.05 ms |
| the pack's register spilling | was real, **fixed**, worth 3% |
| capping SME lanes to the unit count | refuted — more lanes never hurt |
| per-core-class (P vs E) placement | no win — E-cores add 1.7%, excluding them loses 1.7% |
| `kc_nest` on the box | swept: a step function, ~0.55× at 1t, ~0.77× threaded, **every depth** |

### 3. P1 — queue

- **`kc_nest` (the NEON/AVX rung) has now lost on every machine available**, swept not spot-checked.
  Either write down a machine that justifies it, or **delete the lever** (`lib.rs::EmitOpts`).
- **An f16/bf16 rung is the one instruction-density lever that exists.** `svmopa_za32_f16_m` /
  `bf16` accumulate into 32-bit ZA at **2× the MACs per instruction** (i8 at 4×). For f32 there is
  exactly one form and no multi-vector variant, so f32 has no density win — checked in `arm_sme.h`,
  not assumed.
- Executing SME value check in `cargo test` (`tests/sme_rung.rs` is `str::contains` only; the
  differential harness shells out without `-march=armv8-a+sme2` and would SIGILL).
- Whether the `fmopa` port saturates at 4 chains — **unresolved**, needs a clock measurement.
- Predication for non-multiple-of-32 shapes; the S40 coverage debts; the ADR for "guards gate the
  flow"; inlining stamping spliced morphisms with call-site position.

### 4. Measurement rules — S42 added three, all earned by failing

12 before; S42 adds:

> **14. Warm the clock before timing SME, and interleave the variants.** The same binary measured the
> roofline at 1.852 ms cold and 1.069 ms warm — 1.73× on identical code. Best-of-N over a cold window
> is still cold.

> **15. A transformation you cannot find in the assembly is not a variant.** `mm4p.c`'s rotate arm
> was inverted by LLVM at `-O2` and `-O3` and measured base against base.

> **17. Before concluding a parameterised optimization does not pay, SWEEP the parameter.** `sme_kc`
> returned 512; four write-ups concluded "KC loses"; the sweep showed 512 was two steps down a curve
> and 1024 wins. Six causes were investigated at the wrong depth — all wasted.

> **18. When a probe neutralises part of a kernel, verify in the emitted code that the part is gone.**
> Forcing `K=1` shrank the k loop but left the ZA read-out intact, so it counted 131072 read-outs as
> pack cost and produced two confident wrong attributions.

And rule 16 (Sapir, S42) has its worked example: `kc.c` predicted **1.448×**; the emitter delivered
+6.1% at one thread and −25.5% threaded, and the probe's optimum depth (512) was not the emitter's
(1024). *A standalone probe cannot settle what an optimization is worth inside the real pipeline.*

## FIRST commands

```sh
cargo clean && cargo test --workspace --release   # see the stale-cache warning above
git status --short                                # S42 uncommitted
benches/emit_sweep_ab.sh target/release/examples/emit /tmp/now.hashes   # 159 emissions
git worktree list                                 # 3 prunable, pre-rename paths
```

## Live state at S42 close

| Type | Handle | State | Inspect | Cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` @ `f01fb73` | **ahead 1, UNPUSHED**; 5 modified + 14 untracked | `git status -sb` | Sapir's call |
| git identity | `Sapir Shemer <lesscomplexity@gmail.com>` | set | `git config user.email` | — |
| network | github.com | reachable (200) | `curl -sS -m5 https://github.com` | — |
| worktree ×3 | `…/-Volumes-LessComplex-Personal-**Flow**/…` | **all prunable** — pre-rename paths | `git worktree list` | `git worktree prune` — safe |
| machine | Arch box `100.81.226.103`, i9-14900F | **up**, governor `performance`, 24C/32T, 2 MB L2/core, 36 MB L3, AVX2 (no AVX-512), **no clang** | `ssh 100.81.226.103 nproc` | owned box |
| artifact | box `~/mapal-s42/` | **107 MB, 40 files** — cross-compiled objects, binaries, `libmapal_rt.a`, `sw.sh`, `an.py` | `ssh … 'ls ~/mapal-s42'` | **delete when done** — Sapir's call |
| file | `oainotes.md` | untracked, deliberately uncommitted | — | Sapir's call |
| data | session scratchpad | probe binaries, hashes, sweep logs | — | **session-local, will vanish**; everything load-bearing is in the tree |

**Nothing is running.** No background job, no server, no port.

**Cross-compiling for the box** (it has `gcc` but no `clang`; the emitted `.ll` has no target triple):

```sh
cargo build -p mapal-rt --release --target x86_64-unknown-linux-gnu
clang -O2 -target x86_64-unknown-linux-gnu -mavx2 -mfma -c emitted.ll -o out.o
scp out.o target/x86_64-unknown-linux-gnu/release/libmapal_rt.a 100.81.226.103:~/mapal-s42/
ssh 100.81.226.103 'cd ~/mapal-s42 && gcc -O2 out.o libmapal_rt.a -lpthread -ldl -lm -o bench'
```

## Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | **Hierarchical tile→cache mapping (L1/L2/L3)** | §1, `s42-sme-roofline.md` §5e | First VERIFY the emitted kernel really misses L1; then design the cascade with a per-level derivation | 1t GF/s moves off 1043 toward 1864 |
| P0 | commit/push S42 + `f01fb73` | live state | Sapir's call | `git status -sb` clean |
| P1 | delete or justify `kc_nest` | `lib.rs::EmitOpts` | it has lost on every machine, swept | the lever is gone or has a written reason |
| P1 | executing SME value check in `cargo test` | `benches/sme/README.md` | wire into `differential.rs` with `-march=armv8-a+sme2` | the suite runs an SME binary |
| P1 | f16/bf16 rung (2× MAC density) | §3 | plan first — it is a new face, not a tailor | `svmopa_za32_f16_m` emitted and value-gated |
| P2 | box scratch `~/mapal-s42` (107 MB) | live state | delete when the box work is done | gone |
| P2 | 3 prunable worktrees | live state | `git worktree prune` | `git worktree list` shows only `main` |
| P2 | does the `fmopa` port saturate at 4 chains | §5b/§1 | needs a clock measurement | answered or dropped |

## Standing direction (Sapir — unchanged)

- Compute-only legs; numpy in every verdict table; scale everything up.
- Parallel-first by construction — **threaded is the configuration that ships.**
- Backend-genericity contract (ADR-0032): mapal-ir never learns machine facts.
- **Keep stuff generic, not hardcoded** — and when a constant is swept rather than derived, say so
  (`Sme::panel_l1d_ratio` is the S42 example of doing this honestly).
- Query, not rewrite: record that something *could* be skipped; never delete it.
- Compile time decides the SIZES, runtime decides the ASSIGNMENT.
- Nothing goes in the README that a default build does not deliver.
- Proof over suggestion — a change arrives with the measurement of what it did.
- Speak simply, base claims on empirical results. **Give absolute ms, not only ratios.**
