# Plan — S42: the SME k-panel, and the KC derivation that is calibrated for the wrong unit

**Status: BUILT, MEASURED, and SUPERSEDED — read this box before the plan.**

The plan was executed. What it predicted and what happened:

| the plan said | what was measured |
| --- | --- |
| kc = 512 derived from L1D | **wrong** — a depth sweep put the optimum at **1024**; 512 measured 0.785× |
| 1.448× at N=4096 (from `kc.c`) | **did not transfer** — +6.1% at 1 thread, **−25.5% threaded** |
| acceptance: the 1t curve stops decaying | partly — 1t 4096 improved, threaded regressed |

**It ships default-OFF behind `--kc`.** It captures ~6.1% of the **79%** that
`docs/performance/s42-sme-roofline.md` §5e later showed is available, so the technique is not wrong —
this flat single-level realisation is. **S43's P0 replaces it with a hierarchical
tile→cache cascade (registers → L1 → L2 → L3), which is Sapir's direction and the correct shape.**

Everything below is the pre-build plan, kept for the model and the hazards, which held up. The
depth derivation in it is superseded by `Sme::panel_l1d_ratio`.

---

**Original status:** written pre-build, S42.
Origin: S42's measurement campaign (`docs/performance/s42-sme-roofline.md`), which refuted the
stated P0 (k-loop software pipelining, +0.1–0.2% overlapping) and produced a measured replacement.
Related: ADR-0032 D4 (backend config = performance tailors) · ADR-0034 (constants are searched, not
written) · `plan-s31-target-profiles.md` (the machine-fact half) · `plan-s31-deduced-blocking.md`
(the precedent: a swept literal replaced by a derivation that reproduces it).

## Why (one paragraph)

`TargetProfile::tile_kc` already *derives* the k-panel depth from a recorded machine fact rather
than hardcoding it — `((l2_bytes / 2) / (nc · elem_bytes)).max(1)` — and on `apple-m` that
derivation returns **4096**, which closes the KC gate (`site.k > tile_kc`) for every K this project
runs. `profile.rs:389` asserts exactly that, deliberately. `APPLE_M4_SME` inherits the number
unchanged (`profile.rs:481`). But that derivation is calibrated against **shared L2 and the NEON
rung's `nc` geometry**, and the SME rung is a different execution site with a different working
set: two packed panels of `kc × (ti·t)` and `kc × (tj·t)` elements. Measured on this part, the SME
optimum is **kc = 512**, a 128 KB two-panel window — and the unblocked case loses **1.448×** at
K=4096. One deduced morphism is serving two placements that have different budgets, which is the
defect; the fix is a second `TrnLoc`, not a second constant.

## The measurement that motivates it

`benches/sme/kc.c`, N=4096, f32, 1 thread, 9 alternating runs, medians, every KC value gated
against an independent scalar `fmaf` reference. Ceiling = 2001.0 GFLOP/s (`fmopa` with zero memory
traffic, interleaved with the variants):

| KC | median ms | GFLOP/s | % ceiling | two-panel working set |
| ---: | ---: | ---: | ---: | ---: |
| 256 | 162.622 | 845.1 | 42% | 64 KB |
| **512** | **124.813** | **1101.2** | **55%** | **128 KB** |
| 1024 | 131.310 | 1046.7 | 52% | 256 KB |
| 2048 | 155.278 | 885.1 | 44% | 512 KB |
| 4096 | 180.679 | 760.7 | 38% | 1024 KB ← what ships today |

**1.448× at the optimum, distributions DISJOINT.** Three properties make it a real cache curve:

1. **Unimodal** — 256 (64 KB) is *worse* than 512, so there is an optimum rather than a trend. A
   monotone curve would have meant "smaller is always better", which is the signature of an
   artifact, not of blocking.
2. **It flattens the size curve.** 1101.2 at N=4096 lands level with 1089.0 at N=1024
   (`pipe2.c`). The decay S41b attributed to arithmetic intensity — and which survived B packing —
   is gone.
3. **It wins despite the ZA read-modify-write** that blocking forces (below). The crossover
   `next-session.md` §3 warned about is real and lands in blocking's favour at K=4096.

**Bounded by Sapir's rule 16:** this is a standalone C kernel, so **1.448× is a floor on the shape,
not a target figure**. It has none of the other optimizations to compose with and says nothing
about threaded scaling. The acceptance criteria below are therefore integrated and threaded, and
this plan is not "done" on a probe number.

## Categorical model

**No `Dat` change and no `mapal-ir` change.** Blocking permutes *when* a partial sum is computed,
never the operand chain inside a cell — but unlike S31's row blocking it **does** change the
k-summation order per cell (from one `Σ_{k=0..K}` inside ZA to `Σ_blocks (Σ_{k∈block})` with the
block partials landing in `c`). Under the contract face that is a legal re-association and the
existing NEON KC rung already relies on it; it is recorded here because it is the one composition
rule this change touches, and the differential suite is the enforcement.

### The atoms

| Item | Kind | Model |
| --- | --- | --- |
| `SmeUnit` | `Loc` | the streaming matrix unit. **Distinct from the NEON core**: its own accumulator (ZA), its own effective working-set budget. Already implied by S41b's finding that the unit is *shared, not per-core* (NEON scales 8.61× across P-cores, SME 2.23×) |
| `ApPanel`, `BpPanel` | `Dat` + `DataLoc` @ `SmeUnit` | the two packed operand windows. Today `ApPanel` is `ti·t × K`; after this change `ti·t × kc` |
| `kc_sme : (Loc, Ty, Block) → ℕ` | `Trn`, **deduced** | the k-panel depth **for this placement**. New; see below |
| `panel_store : (ApPanel, BpPanel) → CBlock` | `Trn` @ `SmeUnit` | today's kernel: reduce over k, **store** ZA. Precondition `seed == 0` |
| `panel_acc : (ApPanel, BpPanel, CBlock) → CBlock` | `Trn` @ `SmeUnit` | **new**: reduce over k, then `read.horiz` + `fadd` + store. The price of blocking |
| `first : Block → 𝔹` | `Trn`, deduced | `k0 == 0`. Selects `panel_store` vs `panel_acc` — two parallel realisations over one contract (FRAMEWORK §4.4, strategy shape) |

### The deduced morphism, and why it must be per-`Loc`

FRAMEWORK §4.2: *`runsAt` is a relation, not a function.* "How deep may a k-panel be" is **one
transformation placed at two locations** — the NEON core and the streaming matrix unit — and the two
placements have different budgets and different panel geometry. A single total
`kc : Ty → ℕ` is exactly the unsound single-valued `runsAt` the framework names. So:

```
kc_sme(elem) = (l1d_bytes / (elem_bytes(elem) · (ti + tj) · t)).max(1)
```

with `l1d_bytes` a **new recorded machine fact** (per-`Loc`, ADR-0032: the backend profile learns
machine facts, `mapal-ir` never does). On this part:

```
(ti + tj)·t = (2 + 2)·16 = 64 elements per k
64 · 4 B    = 256 B per k
131072 / 256 = 512          ← reproduces the measured optimum exactly
```

**128 KB is the M4 P-core L1D.** That is the same discipline `tile_kc` follows and the same
discipline `plan-s31-deduced-blocking.md` set: a literal that was swept once is replaced by a
derivation that *reproduces it* on the machine it was swept on, and produces a defensible number
on a machine nobody has swept. `f64` scales off the same fact by element width — no second
constant, which is the S41b lesson about `f32_tiles` applied again.

**Gate:** block only when `site.k > kc_sme(elem)`. At K ≤ 512 the nest disables itself by
derivation, exactly as `tile_kc` does on `apple-m` — and that is the right behaviour, because the
probe shows blocking is a **loss** at small K where the panel already fits.

### Coherence check (FRAMEWORK §4.5)

1. **Placement honesty** — `panel_acc` reads `CBlock` at `SmeUnit`. `CBlock` is materialised there
   by the preceding block's store, so the `DataLoc` exists. ✓ (This is the law that makes the
   `seed == 0` precondition necessary today and *sufficient* after: only the `first` block may
   assume an implicit zero.)
2. **Transmission well-typing** — no new `Trm`; both kernels are same-`Loc`. ✓
3. **Placement totality** — `kc_sme` needs `l1d_bytes` on every profile that has an `Sme` block.
   Enforce structurally: put `l1d_bytes` **inside the `Sme` struct**, not beside `l2_bytes`, so a
   profile cannot declare a matrix unit without declaring its budget. ✓
4. **Dependency mediation** — unchanged. ✓
5. **Composition soundness** — unchanged. ✓
6. **`runsAt` is a relation** — this plan exists to satisfy it. ✓

## The shape of the change

Three sites, in this order, one commit each.

### Step 1 — the machine fact and its derivation (`profile.rs`)

Add `l1d_bytes: u64` to the **`Sme`** struct; add `TargetProfile::sme_kc(&self, elem) -> Option<u64>`.
Tests: `sme_kc(F32) == 512` on `APPLE_M4_SME` (the derivation reproduces the sweep); `f64` falls out
by width; a profile with `sme()` but no `l1d_bytes` does not compile. **No emission change** —
`sme_kc` has no caller yet, so the sweep must be 159/159 and 636/636 byte-identical.

### Step 2 — the accumulating kernel (`module.rs::sme_panel`)

A second emitted function, **not** a flag on the existing one:

```
@mapal_sme_panel      -- unchanged, byte-for-byte: zero ZA, reduce, store
@mapal_sme_panel_acc  -- zero ZA, reduce, then read.horiz + fadd + store
```

**Why two symbols rather than an `i1 %first` parameter.** Adding a parameter changes the emitted
call at every existing SME site, so today's SME emission stops being byte-identical for no gain;
and it puts a branch inside the read-out loop (`t` iterations × `ti·tj` tiles). Emitting the second
function **only when the KC nest fires** keeps the unblocked path bit-identical to what ships today,
which is the property that makes the change reviewable. The two bodies share their k loop and
differ only in the read-out, so factor the k loop into one generator taking a read-out closure —
one source of truth for the shared structure, the variation confined to one declared seam
(FRAMEWORK §5). Do **not** copy the k loop twice.

### Step 3 — the k-panel loop (`func/sme.rs::emit_tiled_map_sme`)

```text
for k0 in (0..k).step(kc):                        -- new outer loop
    for i0 in (0..rows).step(ti·t):
        pack ap[kk][i] = a[base + ci·(i0+i) + ck·(k0+kk)]     -- kk in 0..kc, not 0..k
        for j0 in (0..c).step(tj·t):
            (k0 == 0 ? sme_panel : sme_panel_acc)(ap, b-panel(j0,k0), &out[…], bn, bj, c, kc)
```

Four consequences to get right, each already visible in today's code:

- **The `ap` alloca shrinks** from `ti·t·k` to `ti·t·kc` — 512 KB → 128 KB at K=4096, f32, 2×2.
  `func/sme.rs:163-165` carries a `ponytail:` marker predicting exactly this upgrade; that marker
  is discharged by this step and should be deleted with it, not left to rot.
- **The b panel gains a `k0` offset.** Packed: panel `j0/t` starts at `(j0/t)·k·tile_j` and the
  k rows are `t` apart, so the block start is `+ k0·t`. Unpacked: `+ k0·b.ck`. Both arms of the
  existing `match &packed` (`func/sme.rs:295-319`) need the offset; the packed arm's `b_cols` stays
  `t·k` (the *whole* panel stride) — **not** `t·kc` — because the panels themselves are not
  re-blocked. This is the easiest thing in the plan to get wrong.
- **`k >= t` becomes `kc >= t`** in the predicate, since the kernel now reduces over `kc`.
- **Slice quantum unchanged.** Blocking is inside a task's row range; `slice_sizing` still hands
  `ti·t·c` and clause 7 still holds, so S41b's alignment proof is untouched. Confirm, do not assume.

## Acceptance — integrated and threaded, per rule 16

A probe number does not close this. Required, in order:

1. `cargo test --workspace --release` green, and `benches/emit_sweep_ab.sh` **159/159 + 636/636
   byte-identical** for every pre-existing profile after *each* step. Step 1 must move nothing.
2. **Value identity in the real emitter** — SME leg vs NEON leg vs interpreter oracle, at
   512/1024/2048/4096, square and non-square, `k` not a multiple of `t`, non-zero base, transposed
   A, packed and `--no-pack`, arena path, under ASan. Blocking re-associates the k sum, so this is
   the gate that matters most and it is the one thing the probe cannot check.
3. **`benches/sme/sme_pack_ab.sh`-style A/B in the emitter**, KC on vs off: ≥21 alternating runs,
   medians alongside minima, explicit overlap check, absolute ms with the baseline commit named.
4. **Threaded, not only 1t.** S41b measured the matrix unit as *shared* (SME scales ~2.2× across
   cores against NEON's 8.6×), so a per-core cache win may or may not survive contention for one
   unit. **Report both, and do not quote a threaded number whose distribution overlaps.**
5. **Scale up** — 4096 is where the prize is; 512 is where the probe shows a *loss*, so the gate
   closing by derivation must be verified to actually close there.

**Done when:** the 1t GF/s curve stops decaying at 4096 (716 → ~1000+), the emission sweep is
unmoved for pre-existing profiles, and the threaded number is reported with its distribution.

## Hazards, recorded so they are not "cleaned up"

1. **The re-association is the risk, not the loop.** Every other hazard here is mechanical; this one
   changes floating-point results per cell. It is legal on the contract face and the NEON KC rung
   already does it — but the SME rung's differential is currently **hand-run and not repeatable from
   `cargo test`** (`benches/sme/README.md`), which is a P1 debt this plan now *depends* on. Consider
   promoting "executing SME value check in `cargo test`" to a prerequisite rather than a sibling.
2. **`seed == 0` is load-bearing and now subtler.** Today the precondition is "the fold identity is
   a true zero" because the kernel stores. After blocking, only the `first` block may store; a
   future change that lets a non-zero seed through must route it to `panel_acc` for block 0 or the
   seed is silently dropped.
3. **`b_cols` is `t·k`, not `t·kc`.** See step 3.
4. **The `--kc` flag already exists** (`emit` usage line) and drives the NEON KC nest. Decide
   explicitly whether SME blocking rides that flag or its own gate; riding `--kc` couples two
   independent decisions, and the NEON one is *measured-off* on `apple-m`.
5. **Streaming mode may not see L1D the way the derivation assumes.** The 128 KB fit is a strong
   coincidence with the P4-core L1D, but it is **one data point at one N with power-of-two KC only**.
   Before trusting the derivation, sweep non-powers-of-two around 512 and repeat at N=2048 — if the
   optimum tracks `l1d_bytes` it is a machine fact; if it tracks N it is not.

## Open questions

- **Does the ceiling story hold?** `docs/performance/s42-sme-roofline.md` §1 withdraws the
  cycle-level derivation (latency 4 / issue 1-per-cycle) as unproven — it is one equation with two
  unknowns and the measured ratio exceeds the model's own limit. Blocking's headroom above 55% of
  ceiling depends on which reading is true. Needs a clock measurement, which no probe here does.
- **Does KC compose with the A pack?** The A pack is ~13% of N=1024 and is a scalar
  element-at-a-time transpose-gather (`func/sme.rs:239-260`). Blocking makes it `kc`-deep and
  re-runs it per k-block, so it is re-touched `K/kc` times — the *same* total elements, but the
  plan should confirm it does not become the new dominant term at 4096.
- **Is `l1d_bytes` the right fact, or is it "half of L1D", or an L2-per-core slice?** The
  derivation reproduces 512 from L1D exactly, which is suggestive, not conclusive (hazard 5).
