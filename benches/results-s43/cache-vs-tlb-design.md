# S43 — separating cache reach from TLB reach

Worktree: `.claude/worktrees/agent-a9c8b56e24b5dee89`. Machine: Apple M4 Pro, `hw.pagesize` = 16384.
**Nothing here is committed. This file is written incrementally — every result lands here the
moment it is taken, before it is interpreted.**

## STATUS

- [x] Loop nest read and confirmed from the emitter (§2) — this is the load-bearing fact
- [x] `benches/sme/tlbreach.c` written (§3)
- [x] `benches/sme/resid_ab.sh` reworked to 9 arms + asserted `and` counts + permutation test (§4)
- [ ] `tlbreach.c` built and swept (§6) — NOT RUN
- [ ] kernel B-window sweep run (§7) — NOT RUN
- [ ] verdict (§8) — NOT REACHED

**§0 and §1 contain the only numbers in this file, and both are quoted from elsewhere (the task
brief and `loadlevel.c`'s header). §§2-5 are design. Nothing has been measured by me yet.**

## 0. The question

A 1.71× effect was measured by masking the two k-derived operand offsets in `@mapal_sme_panel`
so its 4 loads wrap inside a window of chosen size. N=4096, 1 thread, 21 cycles, absolute ms:

| arm | window | median ms | vs control |
|---|---|---:|---:|
| 1 control | real (A 512 KB, B 2×256 KB) | 174.596 | — |
| 2 | A→16 KB, B real | 172.608 | 1.012× overlaps |
| 3 | A real, B→2×16 KB | 101.764 | **1.716× disjoint** |
| 4 | both→16 KB | 101.854 | **1.714× disjoint** |
| 5 | both→128 KB | 129.782 | **1.345× disjoint** |

Shrinking a window shrinks the byte footprint **and** the page footprint together, so
"operand cache residency" and "operand page (TLB) residency" predict this table identically.

## 1. What was ALREADY wrong with the framing — the axis was mis-mapped

Read the emitter before designing anything. `crates/backends/llvm/src/func/sme.rs:102-128` states
the nest, and `module.rs:149-259` the kernel:

```
for i0 in (0..rows).step(ti·t):          # ti·t = 32 rows  →  128 i-steps at N=4096
    pack ap[k][i]                        #   512 KB, one pack per i-step
    for j0 in (0..c).step(tj·t):         # tj·t = 32 cols  →  128 j-panels per i-step
        mapal_sme_panel(ap, b+panel(j0), &out[i0·c+j0], bn, bj, c, k)
```

Inside the kernel (`module.rs`), per k iteration: `%aoff = mul %k, 32` (A advances 128 B),
`%boff = mul %k, %bn` with `bn = t = 16` floats (B advances 64 B), two B streams `%bj = t·k`
= 256 KB apart. So **per panel call**: A 512 KB, B 2×256 KB. Real max offsets are
`aoff ≤ 4095·32 = 131040` and `boff ≤ 4095·16 = 65520`.

**The window is per CALL. The thing that decides residency is the per-I-STEP footprint**, because
that is the reuse distance: the whole of B is re-swept on every one of the 128 i-steps.

| B window (per stream, per call) | B footprint per i-step = 128 panels × 2 streams × window |
|---|---|
| 16 KB (mask 4095) | **4 MB** |
| 32 KB (mask 8191) | **8 MB** |
| 64 KB (mask 16383) | **16 MB** |
| 128 KB (mask 32767) | **32 MB** |
| 256 KB (real) | **64 MB** |

Map the existing arms onto `loadlevel.c`'s own variant-(b) curve, which is the kernel's load shape:

| kernel arm | B footprint | `loadlevel.c` (b) at that footprint | kernel measured |
|---|---:|---:|---:|
| arm 4 | 4.5 MB | ~1990 GF/s (flat region) | 1349 GF/s |
| arm 5 | 32 MB | ~1150 GF/s (interpolated 24M→64M) | 1059 GF/s |
| control | 64 MB | 833 GF/s | 803 GF/s |

**The claimed probe/kernel contradiction may be an artefact of comparing the per-call window
(48 KB / 384 KB) against the probe's working-SET axis.** Re-mapped onto footprint, probe and
kernel are monotone-consistent, with the kernel below the probe by a gap that shrinks as the
memory wall dominates — exactly what a fixed overhead (pack ≈ 8 ms + ZA read-out + stores)
predicts. §7 tests this by sweeping the parameter instead of asserting it.

**This does not resolve cache-vs-TLB.** Footprint and page count still co-vary. §3 does that.

## 2. Why an in-kernel bytes-vs-pages arm is IMPOSSIBLE — and it is worth writing down

The natural design ("hold bytes constant, spread them over more pages, in the kernel") was
evaluated and **rejected on arithmetic**. Any offset transform must keep `inbounds`, i.e. produce
offsets ≤ the real max. A bit-spread `o' = (o & LOW) | ((o & HIGH) << S)` that puts each chunk on
its own page reaches at most:

- B: `boff ≤ 65520` floats = 256 KB ⇒ **≤ 16 pages**
- A: `aoff ≤ 131040` floats = 512 KB ⇒ **≤ 32 pages**

16 or 32 pages is inside any L1 DTLB. So the in-kernel instrument can only probe 1..32 pages and
**cannot reach a TLB effect at all**. A rotation of the chunk index is worse: it is a bijection, so
it holds bytes *and* pages constant and only changes visit order.

⇒ The separation has to be done standalone, where there is no allocation bound. Rule 3 still
applies (a probe prices, it does not settle) — but what this probe settles is a **property of the
machine** (its TLB reach in pages), and that fact is what adjudicates the kernel's confound.

## 3. The design — `benches/sme/tlbreach.c`

**Hold bytes touched constant; vary page span.**

One iteration touches one 256 B chunk (4 loads at +0/+64/+128/+192, 2 cache lines) and does 4
`fmopa` — byte- and flop-identical to `loadcost.c`'s row and `loadlevel.c`'s variant (a), so
numbers are directly comparable to both. Chunk `j` sits at byte `j · 256 · M`. Sweeping the
multiplier **M** at a fixed chunk count **N**:

```
bytes touched = N · 256                      ← CONSTANT along an M sweep
pages touched = min(N, floor((N-1)·M/64) + 1) ← rises up to 64×
span          = N · 256 · M
```

At N = 4096 the working set is **1 MB at every M**. `loadlevel.c` measured this machine dead flat
from 32 KB to 8 MB, so 1 MB is free of capacity effects *by that instrument's own reading*.
Anything that happens as M drives the page count 64 → 4096 is therefore **translation**.

### Why M is ODD (keep this — it is the part of the design most easily lost)

A power-of-two stride puts every chunk in the **same cache set** and measures conflict misses, not
residency. This is the classic trap and it would have produced a confident wrong answer. Odd
multiples of 256 rotate the set index. `M ∈ {1,3,5,9,17,33,65,129,257}`.

For `M ≥ 64` every chunk lands on its own page, so **M = 65 / 129 / 257 hold both bytes AND pages
constant and vary only the span** — the built-in control for any residual conflict or DRAM-row
effect. If those three differ, something span-related is live and the page reading is suspect.

### Why both visit orders

Raising M destroys spatial locality as well as page locality, so a plain M sweep confounds the TLB
with the hardware prefetcher. `rev` visits the same chunk set in **bit-reversed index order**
(`__builtin_bitreverse64` → one `rbit`), which is maximally prefetch-hostile at *every* M. Reading
the M sweep along `rev=1` holds prefetch hostility constant and moves only the page count. The
`rev=0, M=1` cell is the sequential stream and is the calibration cell.

### Identical code in every cell (rule 15)

`n`, `reps`, `stride_f`, `rsh`, `rev` are all **runtime arguments of one function**, so a cell
change moves register contents and nothing else. Only the rev=0/rev=1 paths may differ if the
compiler unswitches; the M sweep within a fixed order cannot.

### Cells

`lgn ∈ {7,10,12,14}` → N ∈ {128, 1024, 4096, 16384} → bytes {32 KB, 256 KB, 1 MB, 4 MB}.
Cells whose span exceeds a 320 MB cap are skipped and **said** to be skipped.

| bytes | M | pages | span |
|---|---|---|---|
| 1 MB | 1 | 64 | 1 MB |
| 1 MB | 3 | 192 | 3 MB |
| 1 MB | 5 | 320 | 5 MB |
| 1 MB | 9 | 576 | 9 MB |
| 1 MB | 17 | 1088 | 17 MB |
| 1 MB | 33 | 2112 | 33 MB |
| 1 MB | 65 | 4096 | 65 MB |
| 1 MB | 129 | 4096 | 129 MB |
| 1 MB | 257 | 4096 | 257 MB |

Pure **byte** axis at ~constant pages, from the same table: (256 KB, 1024 pg), (1 MB, 1088 pg),
(4 MB, 768 pg) — spans 16/17/12 MB, close enough to be a real control.

### Gates before any number is read

1. **Calibration.** (32 KB, M=1, seq) IS `loadcost.c`'s 4-load row = `loadlevel.c`'s 32 KB variant
   (a) ≈ 1990–2005 GF/s. Off by more than a few % ⇒ the index arithmetic is costing and the sweep
   is measuring *that*. Printed, not assumed.
2. **Assembly.** `clang -S`, confirm 4 `ld1w` + 4 `fmopa` in the loop and that `stride_f` arrives
   in a register (not folded).
3. **Drift.** rep0 vs rep(n-1) on cell 0.
4. **Pre-fault.** Every live cell's chunks are touched before timing; a page fault inside a timed
   region would be measured as translation cost, which is the quantity under test.

### Decision rule, declared before the run

- 1 MB row, rev order, GF/s **flat** (within ~6%) from 64 to 4096 pages
  ⇒ **TLB reach ≥ 4096 pages ⇒ TLB is NOT the kernel's mechanism**; the 1.71× is cache/DRAM.
- 1 MB row **falls with a knee at a page count P\*** that reproduces at other byte counts
  ⇒ TLB reach = P\* pages. Then check whether the kernel arms straddle P\* (control 4096 pg
  per i-step, arm 5 2048, arm 4 256) and apportion.
- M = 65/129/257 differ at constant bytes and pages ⇒ span effect is live, reading is suspect.

## 4. The in-kernel companion — `resid_ab.sh` reworked

Bugs fixed as instructed, plus the sweep the original lacked:

- **`and` count is now ASSERTED per arm**, not printed. `EXPECT_AND = {0:0, 1:0, 2:1, 3:1, 4:2,
  5:2, 6:1, 7:1, 8:1}` — `K=4096` and `bn=16` reach the kernel as constants so a 2⁴⁴−1 mask is
  provably dead and is folded; 4095/8191/16383/32767 are live. An arm whose count differs FAILS.
- **Verdict is a two-sided permutation test on the difference of medians** (20000 shuffles) plus a
  bootstrap 95 % CI on the median ratio, replacing the min/max range test a single sample could flip.
- **`winmask.py` now rejects masks below 2⁵−1 = 31**, which would break the 32-float A stride
  alignment and measure split-load cost instead of residency.
- **New arms 6/7/8** give a 5-point B-window sweep 16/32/64/128/256 KB with A unmasked. Those five
  arms carry exactly ONE surviving `and` each, so they are mutually byte-identical except one
  immediate — the cleanest comparison in the design.

## 5. Families evaluated and NOT run, with reasons

- **In-kernel constant-bytes/vary-pages** — impossible, §2. Refuted on arithmetic before any build.
- **PMU counters via `xctrace`** — deferred behind §3/§7. The mandatory calibration (do
  streaming-mode SME loads even reach core L1D/DTLB counters — the SME unit is shared and sits
  outside the core) costs a day if it fails, and §3 answers the same question with a wall clock.
  Would only be reached if §3 came back ambiguous.
- **Huge pages** — not available: Apple Silicon's base page is already 16 KB and
  `VM_FLAGS_SUPERPAGE_SIZE_2MB` is x86-only.

## 6. RESULTS — `tlbreach.c`

### Gate 2 (assembly) — PASSED, and better than the design assumed

`clang -O2 -march=armv8-a+sme2 -S`, inside `walk`:

| instruction | count |
|---|---:|
| `ld1w` | 4 |
| `fmopa` | 4 |
| `rbit` | 1 |
| `mul` | 1 |
| `csel` | 1 |
| loop blocks | 5 |

**One loop body, not two.** The compiler did NOT unswitch on `rev`; it kept the `csel`. So
*every cell in the sweep — both orders included — executes a bit-identical instruction stream*,
and a cell change moves register contents only. That is stronger than §3 claimed.

### Timings

**NOT YET RUN. NO NUMBERS EXIST YET.** (Queued behind `perflock.sh`; another agent is building.)

## 7. RESULTS — kernel B-window sweep (`resid_ab.sh`, 9 arms)

**NOT YET RUN. NO NUMBERS EXIST FOR THIS SECTION.**

## 8. VERDICT

**NOT REACHED. No verdict may be quoted from this file until §6 and §7 carry real numbers.**

The only numbers in this file so far are the ones handed to me in the task (§0) and the ones read
out of `loadlevel.c`'s own header comment (§1). Everything else is design.
