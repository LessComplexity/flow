# plan-s43 — one outer blocking level over the B panel (`nc`) in the SME rung

Status: **DRAFT — written before any code, per FRAMEWORK §6.1**
Written: 2026-07-31 · S43 · authorized by Sapir
Governs: `crates/backends/llvm/src/func/sme.rs::emit_tiled_map_sme`
Component: `backend-llvm` (SME rung). **No `mapal-ir` change (ADR-0032). No `Dat` change.**
Predecessor: `plan-s43-operand-residency-verification.md` §6, row 1 — *"arm 3 dominant ⇒ build
the outer B level first."*

## 0. What this plan builds, and what the evidence retired

The predecessor plan proposed a four-rung GotoBLAS cascade (registers → L1 → L2 → L3). **The
measurements retired three of the four rungs before a line was written**, and this plan is what
survives:

| evidence | what it retired |
| --- | --- |
| `benches/sme/loadlevel.c`: dead flat **~1990 GF/s / 249 GB/s from 32 KB to 8 MB** at a fixed 256 B/iteration operand pattern — no cliff at L1D (128 KB), none at the per-core L2 slice (~3.2 MB) | **the L1 micro-panel rung.** L1-vs-L2 costs nothing on this part, so a rung that buys L1 residency buys a benefit the machine does not sell |
| the only cliff is **shared L2 → DRAM**, opening between 8 M and 12 M and complete by 24 M (16 MB shared L2), DRAM floor ~95 GB/s ≈ 765 GF/s | the target is a **single** working-set threshold, not a hierarchy |
| shipping kernel at 1 thread: **787 GF/s against a 765 GF/s DRAM floor** | the 1-thread kernel is DRAM-bandwidth-bound, not L1-starved |
| window instrument on the emitted `.ll`: **1.71× at 1 thread** (174.596 → 101.854 ms, disjoint) but **≤5% threaded** (54.291 → 51.788 ms, inside the 6% noise floor) | **the prize is a 1-thread prize on today's evidence.** Threaded is what ships (rule 16) |

⇒ **One level. Keep the B working set under the ~8–12 MB knee.**

**A mechanism this plan does NOT claim.** The 1.71× is confounded between cache reach and TLB
reach (`hw.pagesize` = 16384; B spans 4096 pages per i-step, the arm-3 window collapses it to 1).
`nc` fixes both, which is why it is worth building before that separation lands — but no sentence
here attributes the win to either. When the separation lands, this document gets a row, not a
rewrite.

## 1. The categorical model

The rung today asserts one operand location and has never sized the B stream against it.

| Atom | Today | After this plan |
| --- | --- | --- |
| `Trn` | `sme_panel` — the `ti·tj` outer-product accumulation over k | **unchanged, byte-for-byte.** `module.rs::sme_panel` is not touched. This is a placement change, not an algorithm change |
| `Loc` | one implicit operand location sized by `Sme::panel_l1d_ratio` | the measured chain **ZA → L1D/L2-slice (flat, ~1990 GF/s) → shared L2 (16 MB) → DRAM (~765 GF/s)**. Only the last edge has a cost |
| `DataLoc` | `b` materialised once, at DRAM, with a reuse distance of `|B|` | `b`'s `DataLoc` is **re-sized**: the live extent per `jc` block is `nc·k·sizeof`, chosen to sit on the shared-L2 side of the one cliff |
| `Trm` | the implicit DRAM→core refill of the whole of B, once per i-step | one `Trm` per `jc` block instead of one per (i-step × panel) — **the transmission count is what `nc` buys** |
| `TrnLoc` | `emit_tiled_map_sme` placed once per task | unchanged. The `jc` loop lives **inside** the task body, so every thread walks the same `jc` block at the same time and the block is shared in L2 rather than replicated |

**Coherence Law 1 (placement honesty)** is the law being repaired: the rung's inputs were never
"materialised at, or delivered to, its location" — B was delivered from DRAM every time. `nc` is
the smallest `DataLoc` resizing that makes the statement true for the one stream that violates it.

**What `nc` does NOT repair, stated up front.** The A stream's `DataLoc` is `ap`, 512 KB, already
inside the flat region — `loadlevel.c` says moving it costs nothing. That is why there is no `mc`
in this plan (§7).

## 2. The nest, before and after

Today (`func/sme.rs::emit_tiled_map_sme`, `--kc` off):

```text
for i0 in (i_lo..i_hi).step(ti·t = 32):
    pack ap[k][i]                                  # 32 × k, hoisted out of j
    for j0 in (0..c).step(tj·t = 32):
        mapal_sme_panel(ap, b-panel(j0), &out[i0·c + j0], bn, bj, c, k)
```

After, when `nc` is on and legal:

```text
for jc0 in (0..c).step(nc):                        # NEW — outermost, inside the task
    for i0 in (i_lo..i_hi).step(32):
        pack ap[k][i]                              # unchanged code, run c/nc times
        for j0 in (jc0..jc0+nc).step(32):          # bounds are the only edit
            mapal_sme_panel(...)                   # call site unchanged
```

**`jc` must be OUTSIDE the i loop or it is a no-op.** B's reuse distance is across i-steps; a `jc`
loop nested inside `i` visits exactly the same addresses in exactly the same order. This is worth
writing down because the inside placement is the one that costs nothing and buys nothing, and it
is the easy mistake.

**No `PanelWrite::Accumulate`, no value change.** The k axis is not split, so every output block
is still written exactly once, by `Store`, in a permuted order. Floating-point results are
**bit-identical** by construction — which is why §5's value gate is a real gate and not (as in the
window instrument) an inverted one.

**Composition with `--kc`.** `jc` is emitted outermost, so the order is `jc → k0 → i → j`,
GotoBLAS's `jc → kc → ic`. The `k0` diamond (`Store` on the first k block, `Accumulate` after)
stays correct: within one `jc` block the k loop still runs to completion over that block's
outputs. Both levers default OFF; the composition is emitted and tested, not measured.

## 3. The cost, derived before it is measured

`jc` outside `i` means **`ap` is re-packed once per `jc` block**. That is the whole price and it
is not small. At N=4096, f32, 1 thread:

| term | unblocked | `nc` blocked, `c/nc = B` blocks |
| --- | ---: | ---: |
| pack work (`packcost.c`: 8.46 ms per full sweep) | 8.5 ms | **8.5·B ms** |
| B DRAM traffic | `\|B\|` per i-step ⇒ ~8 GB | `\|B\|` per `jc` block ⇒ 64 MB |
| A DRAM traffic | 64 MB | 64 MB · B |
| out DRAM traffic | 64 MB | 64 MB |

At 95 GB/s the unblocked DRAM term is ~86 ms of a 171 ms run. Blocked at `nc = 512` (B = 8):
DRAM falls to ~7 ms and the pack rises to ~68 ms.

⇒ **Predicted 1-thread net at `nc=512`: 171 − 79 + 60 ≈ 152 ms, ~1.13×** — an order of magnitude
less than the window instrument's 1.71×, and the difference is *entirely* the pack multiplier.
The window instrument paid nothing for its residency; `nc` pays `c/nc` packs for it.

**This is the honest prediction and it is recorded before the run.** If the sweep comes back near
it, the finding is "the residency is real and the pack multiplier eats most of it", which selects
the `mc` rung (§7) rather than vindicating this one.

Threaded, both terms shrink but not equally: the pack is bandwidth-bound and parallelises (~0.6 ms
per sweep across 14 cores), while the DRAM saving is already mostly collected by the threads
sharing B in L2 — which is exactly what the ≤5% threaded window result says. **Predicted threaded
net: between −10% and +3%, i.e. most likely inside the noise floor or a small loss.**

## 4. The lever, and why it is `Option<u64>` and not a derivation

`nc` is **swept, not derived, and the type says so**: `EmitOpts::sme_nc: Option<u64>`. `None` is
off and is the default. There is no constant in the shipped emitter to be wrong.

This is measurement rule 4 taken literally. The alternative — a `Sme::b_panel_l2_ratio` policy
ratio feeding a `TargetProfile::sme_nc(elem, k)` derivation — is what `panel_l1d_ratio` looks like,
and it is the **right** shape *once a number has been swept and won*. Writing it first would put a
derivation's clothes on a constant nobody has measured, and would force a rebuild per sweep point
for no gain. The upgrade path is explicit:

- **wins threaded** ⇒ add the policy ratio + derivation in a follow-up, gated on reproducing the
  swept optimum, and default the lever ON;
- **loses threaded** ⇒ the lever ships OFF carrying the measured table, exactly as `kc_nest` does.

Legality gate, emitted only when all three hold — otherwise not one byte moves:

```text
nc % (tj·t) == 0        whole panels only (the rung has no partial-panel path, clause 7)
c % nc      == 0        no ragged final block (a remainder is a real shape; not built)
nc < c                  otherwise the loop is a no-op and byte-identity is the better answer
```

## 5. Gates — every one runs before a timing is read

1. **Byte-identity, proved not asserted.** `benches/emit_sweep_ab.sh` before/after, 159 emissions,
   hashes identical. `nc` defaults to `None`, so every profile — including `apple-m4-sme` — emits
   the same bytes it did.
2. **Value identity.** `benches/sme/sme_ab.sh`'s pattern: NEON leg vs SME leg vs SME+`nc` leg, at
   every N and every `nc` in the sweep, values compared **before** any timing is printed. Unlike
   the window instrument, a value mismatch here is a **defect**, not an expected artifact.
3. **Assembly (rule 15/18).** `clang -S` the `nc`-on module and confirm the `jc` loop exists as a
   real outer loop and that `@mapal_sme_panel`'s body — 4 `ld1w`, 4 `fmopa`, full 16-row × 4-tile
   read-out — is unchanged from the `nc`-off module. A transformation not visible in the assembly
   is not a variant.
4. **`cargo test --workspace --release`**, after `cargo clean` if a failing set moves between runs.

## 6. The falsifiable hypothesis

Declared before the first timed run. N=4096 f32 (`benches/matmul/matmul4096_cap_f32.mapal`),
`--rewrite --contract --target=apple-m4-sme`, packed, `--kc` off, alternating runs, medians,
values gated first, every timing through `benches/perflock.sh`.

> **H** — There exists an `nc` in {256, 512, 768, 1024, 2048} whose **threaded** median at N=4096
> is **≥6% below** the `nc`-off threaded median (53–54 ms ⇒ ≤50.5 ms) **with disjoint
> distributions.**

**Confirmed** ⇒ `nc` ships ON at that value, and the follow-up replaces the swept literal with a
derivation. **Refuted** ⇒ `nc` ships **OFF** with the table recorded in `EmitOpts::sme_nc`'s doc
comment, and the 1-thread column is reported separately as what it is: a one-thread-only effect,
the same verdict `kc_nest` already carries. 6% is this machine's standing noise floor (rule 6);
byte-identical binaries have measured −5.9%…+1.2% apart, so anything smaller is not a result.

**No third outcome.** "Promising, needs more runs" is not admitted.

Reported for every cell: **absolute milliseconds, min/median/max, and an explicit overlap
statement.** 1-thread (`MAPAL_PAR=1`) and threaded (pool default) are separate tables; the
**threaded one decides the default** (rule 5 — `kc_nest` is default-OFF at +6.1% 1t / −25.5%
threaded for exactly this reason).

Sweep, never a single point (rule 4 — the rule that cost a previous session a day when `sme_kc`
returned 512 and four write-ups concluded "KC loses" against a curve whose optimum was 1024):
`nc ∈ {256, 512, 768, 1024, 2048}`, spanning B working sets of 4, 8, 12, 16 and 32 MB against a
16 MB shared L2 and a knee measured between 8 M and 12 M. The knee is **inside** the swept range
by construction, so a curve with no optimum is itself a reportable finding.

Replication at N=2048 (B = 16 MB whole, so the knee bites weakly and every win must shrink — a
built-in consistency check).

## 7. Deferred, with reasons

- **`mc` (the A-panel row block).** It is what removes §3's pack multiplier: pack a `mc`-row block
  of A **once**, then walk `jc` inside it, and every row of A is packed exactly once no matter how
  many `jc` blocks there are. It is deferred because it needs an A buffer of `mc·k·sizeof` —
  4 MB at `mc=256`, k=4096 — and the SME rung's `ap` is an `entry_alloc` inside a **task** function,
  which keeps its `alloca`s by design (`heap_ok` is false for tasks). Sizing that buffer is a
  heap-lowering change to the parallel path, not a loop change, and it should be paid for by a
  measurement that says the pack multiplier is what is binding. **§3 predicts exactly that
  measurement**, so this is the next rung if `nc` lands where predicted.
- **A serpentine j sweep** (reverse j on alternate i steps). Retains the tail of B across the
  turnaround at zero pack cost — worth ~25% of B traffic on a 16 MB L2 against a 64 MB B. Cheaper
  than `nc` and strictly weaker; it is the fallback if `nc`'s pack multiplier proves fatal and
  `mc` is not authorized.
- **The L1 micro-panel rung** — retired by `loadlevel.c`, not deferred. It should not be built on
  this part.
- **`Sme::panel_l1d_ratio` / `sme_kc` / `kc_nest`** — untouched. Deleting `kc_nest` remains its
  own P1.
- **f16/bf16** — out of scope; the rung is f32-only by deliberate gate.

## 8. Reconciliation — what the code became, and what the machine said

*Written after the build and the sweep. Results and raw series:
`benches/results-s43/nc-blocking.md` + `nc-4096-*.log`.*

### 8.1 Where the design held

| §  | planned | built |
| --- | --- | --- |
| §2 | `jc` outermost, inside the task body, i loop untouched | exactly that — `func/sme.rs::emit_tiled_map_sme`, one `jc_ctr` slot and three labels, both `None` when off |
| §2 | `module.rs::sme_panel` not touched | not touched. The sweep harness re-checks it every run: **1 distinct kernel hash across all arms** |
| §2 | values bit-identical | yes — every arm matched the NEON leg before any timing was read |
| §4 | swept, not derived; `Option<u64>`; illegal widths are no-ops | `EmitOpts::sme_nc`, gate `nc > 0 ∧ nc < c ∧ nc ≡ 0 (mod tj·t) ∧ c ≡ 0 (mod nc)`, one test per rejected width |
| §5 | byte-identity proved | **199/199** emissions unchanged (159 generic sweep + 40 under `apple-m4-sme`), re-verified after every edit |
| §5 | assembly gate | `@mapal_sme_panel` byte-identical in the `.s` (441 lines, 2 `ld1w` + 4 `fmopa` both arms); the task-slice fn gains **3 backward branches and 2 labels** ⇒ the `jc` loop is real in machine code |
| §5 | full gate | `cargo test --workspace --release`: **1034 passed, 0 failed**, 1 ignored |

### 8.2 §6's hypothesis: REFUTED. `nc` ships OFF.

N=4096 threaded, machine exclusive, 15 cycles, control drift 0.23%:

| `nc` | B per i-step | pages | median ms | vs off | |
| ---: | ---: | ---: | ---: | ---: | --- |
| off | 64 MB | 4096 | **54.1472** | 1.000× | — |
| ctl | 64 MB | 4096 | 54.0213 | 1.002× | control, overlaps |
| 128 | 2 MB | 128 | 81.9267 | 0.661× | disjoint LOSS |
| 256 | 4 MB | 256 | 65.0842 | 0.832× | disjoint LOSS |
| **512** | **8 MB** | **512** | 57.9958 | **0.934×** | disjoint LOSS |
| 1024 | 16 MB | 1024 | 55.3078 | 0.979× | overlaps |
| 2048 | 32 MB | 2048 | 54.2902 | 0.997× | overlaps |

No arm clears the 6% bar; the refutation is **monotone** — every arm that shrinks the working set
loses, and loses more the more it shrinks it. Reproduced arm-for-arm by an independent earlier clean
run.

**`nc` = 512 is the cell that settles it.** 8 MB / 512 pages puts B per i-step *inside* the capacity
knee (8–12 MB) **and** inside TLB reach (~2k–4k pages) — the exact configuration
`docs/performance/s43-residency-and-the-thermal-artifact.md` §4b prescribes. Both walls cleared, and
it still loses 7% disjointly. §3's cost model is therefore confirmed, not merely lucky: the `c/nc`
re-pack costs more than the residency buys, because 14 threads sweeping `jc` in lockstep already
share B in the 16 MB L2 and have little residency left to buy.

**Not a failed build: a measured statement** that the outer B level, *paid for with a re-pack*, is
not worth its price threaded on this part. §7's `mc` rung is the design that removes that price,
and the evidence now selects it rather than merely allowing it.

### 8.3 What the plan got materially wrong: the optimum is ABOVE the knee, not at it

The 1-thread leg is unimodal with a real optimum, and it is not where §3 put it (clean run,
control drift 0.20%):

| `nc` | B per i-step | pages | 1-thread median | |
| ---: | ---: | ---: | ---: | --- |
| 128 | 2 MB | 128 | 445.74 ms | 0.384× |
| 256 | 4 MB | 256 | 268.10 ms | 0.638× |
| 512 | 8 MB | 512 | 181.66 ms | 0.942× |
| **1024** | **16 MB** | **1024** | **144.16 ms** | **1.187×, disjoint** |
| 2048 | 32 MB | 2048 | 162.15 ms | 1.055× |
| off | 64 MB | 4096 | 171.07 ms | 1.000× |

§3 sized `nc` to put B **under** the 8–12 MB capacity knee, and
`s43-residency-and-the-thermal-artifact.md` §4b independently prescribes ≤512 to clear the TLB wall
at ~2k–4k pages as well. **Both prescriptions name arms that LOSE.** The winner puts B at 16 MB /
1024 pages — past the capacity knee, only just inside TLB reach.

**The walls size the benefit; `c/nc` sizes the cost, and the cost is steep.** Each halving of `nc`
doubles the A re-pack: 4 → 8 → 16 → 32 sweeps at ~8.5 ms each is +34 → +68 → +136 → +272 ms against
a 171 ms baseline. The optimum is where the marginal costs cross, and that is the *largest* `nc`
that still cuts the reuse distance meaningfully — one whole shared L2, not one knee's worth. A
design that sizes a block against a wall and stops has priced only half the trade.

**A plan that had tested only its own predicted value (512 — the width BOTH walls prescribe) would
have concluded "`nc` loses 6%" and been wrong by 25 percentage points at one thread.** That is
measurement rule 4's lesson repeating verbatim — the same error `sme_kc` made at 512 — and the only
reason it did not land again is that §6 committed to a sweep.

### 8.4 Two accidents worth keeping

- **`nc` = 768 was never measured.** 4096 is not a multiple of 768, so the legality gate rejected
  it and that arm emitted `off`'s bytes — an accidental second zero-effect control, which came in
  at 1.003× (1 thread) and 1.004× (threaded). The harness now says so out loud; an arm silently
  measuring the baseline reads as "`nc` had no effect at this width", which is a different claim.
- **The legal widths at c=4096 are exactly {32…2048}**, so the sweep covers the whole legal space
  above 256. There is no unmeasured point between the optimum and its neighbour.

## 9. Cost

One `Option<u64>` on `EmitOpts`, one parameter threaded through `FnEmit::new`/`emit_parallel`, one
`--nc=N` flag on the `emit` example, one loop in `emit_tiled_map_sme`, one emission test. No
`mapal-ir` change, no `module.rs` change, no profile field. Plus one sweep harness and 2 sizes ×
2 thread widths × 6 arms of measurement.
