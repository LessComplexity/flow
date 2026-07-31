# Plan S44 — the L1 micro-panel rung: one real gap, priced against the only walls this machine has

Status: **PLAN, written before the emitter change** (FRAMEWORK §6.1), reconciled in §10 afterwards.
Machine: Apple M4 Pro (10P+4E, 2 SME, L1D 128 KB / 128 B lines, shared L2 16 MB, page 16 KB).
Worktree `agent-aa6edc589b1483b51` off `6d6302c`.
Governing record: `docs/performance/s43-residency-and-the-thermal-artifact.md`.
Results land in `benches/results-s44/l1-micro-panel.md` **the moment they are taken**, not at the end.

Design by a Fable subagent working read-only against the source; the instrument work in §6 (`tblock.c`,
`stride_ab.sh`) ran concurrently and is folded in as §7's pre-registered predictions.

## 0. The prior, and why it is not the whole story

S43 measured that **this machine charges nothing for L1-vs-L2**: flat ~1990 GF/s / 249 GB/s from a
32 KB buffer to an 8 MB one, no cliff at L1D, none at the per-core L2 slice. The only priced walls
are shared-L2 → DRAM (8–12 MB) and TLB reach (~2k–4k pages, worth 1.571×). For the SME matmul rung
the L1 micro-panel **already exists** — it is `kc` — and sizing it to L1D exactly measured **0.785×**
against an optimum at 2× L1D. `nc` told the same story. ⇒ The honest expectation is that an
L1-sized working set does not pay on this part.

**But S43 priced CAPACITY, and it deliberately held one variable out.** `tlbreach.c` placed its
chunks at odd multiples because "a power-of-two stride puts every chunk in the same cache set and
measures conflict misses — the classic trap that would have produced a confident wrong answer."
That trap was avoided, correctly, and therefore **never measured**. Set-index conflict is the one
cache mechanism on this machine with no number against it. S44's design is built around it.

## 1. Per-rung definition of "L1 micro-panel", with the arithmetic that IS the result

| rung | source | the micro-panel, concretely | working set today | verdict |
| --- | --- | --- | ---: | --- |
| matmul SME | `func/sme.rs`, `TargetProfile::sme_kc` | k-panel depth s.t. two packed operand panels fit `l1d_bytes × panel_l1d_ratio` | swept: ratio 1 (=L1D) → **0.785×**; optimum ratio 2 → 1.064× | **BUILT, REFUTED.** Nothing to build |
| matmul NEON | `func/packed.rs`, `EmitOpts::kc_nest` | k-panel vs half-L2 | swept on two machines, loses at *every* depth — "a step function, not a curve" | BUILT, REFUTED; standing P1 to delete |
| FIR 64-tap | `func/window.rs::emit_tiled_map_blocked_1d` | per 64-lane block the `x` window is `(K−1)·ck + TI·TJ` = 127 elems = **508 B**; +w 256 B +acc 256 B ≈ 1 KB | **1/128 of L1D** by construction | no working set exists to constrain. The arithmetic is the result |
| conv2d 3×3/1026² | `func/conv.rs` | per output row, 3 image rows = `3·1026·4` = **12312 B** | **10.6× under L1D** | no-op by construction |
| saxpy | `func/bulk.rs::emit_map`, captures=0 | none — every array touched once at unit stride, each line consumed 32/32 | 3 live lines | meaningless; **structurally excluded** by the §3 gate |
| reduce | `func/bulk.rs::emit_fold` | none — reordering a float fold changes the value | 1 live line | **illegal by construction** ⇒ the null arm |
| gather | `func/bulk.rs::emit_map`, captures=1 | reorder is legal but cannot localise data-dependent reads | random over 4 MB | no benefit possible ⇒ the **overhead-pricing arm** |
| **transpose** | generic `emit_map`, captures=1, **no fold ⇒ no `TileSite`** | **the blocked traversal of §3** | one column sweep = 1024 distinct lines = **131072 B = L1D exactly** | **THE ONE REAL GAP. Build and measure** |

Two corrections the design found against the brief it was given:

1. **The transpose's reuse distance is not "the whole 4 MB array" — it is exactly 128 KB at side
   1024.** One column sweep touches 1024 distinct 128 B lines before any line is re-used at +4 B.
   That is a knife-edge coincidence of size, and it *strengthens* the negative capacity prior: the
   whole problem (a+b = 8 MB, 512 pages) sits inside both S43 walls. **At side 1024, capacity cannot
   be the mechanism.**
2. ⇒ Either the mechanism is something S43 never priced (set conflict, or exposed latency on a
   4096 B-stride walk), or there is no effect. §6 settles which.

## 2. Where the geometry comes from — and the `mapal-ir` report Sapir asked for

Verified in source, not assumed:

- `TileSite` (`mapal-ir/src/algo.rs::tile_site`) **requires a fold in the map body** (`let fold_id =
  fold_id?`). A fold-less body returns `None`. The transpose map has no fold ⇒ **no site, ever**.
- `ElemSrc`/`ElemPlan` records what `out[i]` *is*, but has **no representation of a captured array
  read's index expression**; `ElemSrc::Apply` records "call this body", not its address structure.
- `reuse.rs` is arithmetic over already-recorded `TileRead` coefficients — nothing to cash, because
  no read is recorded for this map.

⇒ **The backend cannot learn "this map's read is a transpose of width W" from any existing record.**

| option | what | verdict |
| --- | --- | --- |
| (a) a `mapal-ir` fold-less move-site record | extend `tile_site`'s family: map body = a single proven `Index` whose address is affine in the derived axes `(t÷C, t%C)`. `tile_split`/`TileAffine` **already do exactly this one level up** | **the shippable home** — "geometry comes from the record" (profile.rs header). **OUT OF SCOPE: the session's hard constraint is `mapal-ir` untouched. This is the report to Sapir.** Worth building only if §7 confirms |
| (b) a backend-local body-graph recognizer | pattern-match Div/Mod/Mul/Add/Index in the emitter | **rejected** — new graph analysis in the emitter, against "the emitter never re-derives graph analysis" |
| (c) **flag-carried geometry** | `--move-panel=W:B`, W supplied by hand | **chosen for S44.** An instrument, not a structure — but unlike S43's `winmask.py` it measures the transform *in the real pipeline* (real instruction stream, real task slicing). Given the negative prior, a refutation here means (a) is never built: the cheapest possible outcome |

## 3. The transform

Site: `emit_map`'s generic (non-`TileSite`) path — the path transpose and gather take inside
`@taskN_slice`. Both harness legs run the parallel host (`MAPAL_PAR=1` is a *runtime* lever on the
same binary), so `split_range == true` and the bounds are `%lo`/`%hi`.

**Eligibility gate**, in this order, `None` short-circuiting first:
`move_panel = Some((w, b))` ∧ no `TileSite` consumed ∧ `captures ≥ 1` ∧ body contains **no `Fold`**
∧ `n % w == 0` ∧ `n / w ≥ 2`.

- `captures == 0` maps have no cross-element data reach — provably nothing to gain (excludes saxpy
  and every generator map).
- a `Fold` in the body means the per-element working set is the fold's business and belongs to the
  existing rungs (excludes every raw-face matmul / FIR / conv2d / attn site).
- folds themselves are **never** reordered — value identity.

**Emitted structure** (per-element body text byte-identical, emitted by the same code path):

```text
r0 = ceil(lo / W); r1 = hi / W                       -- runtime slice -> whole-row range
head: flat loop  t in [lo, min(r0*W, hi))            -- partial first row of the slice
mid:  for cb in 0..W step B:
        for r in r0..r1:
          for c in cb..min(cb+B, W):  visit t = r*W + c
tail: flat loop  t in [max(r1*W, min(r0*W, hi)), hi)  -- partial last row
```

- **Legality:** the three ranges partition `[lo,hi)`, every `t` is visited exactly once, and the body
  is pure w.r.t. memory (reads captured arrays, writes `out[t]` once). Values are bit-identical by
  construction. *Trap order within a slice can change; the parallel path already makes trap order
  nondeterministic across slices, and every measured program is trap-free. Stated, not hidden.*
- **What it does at the transpose:** reads become `B` interleaved sequential row-streams of `a`
  (live set ≈ B lines + 1 write line); writes become B-element contiguous bursts.
- **`B = W` is the identity permutation** — the same visit order as OFF, through the nest structure.
  It is the sweep's built-in **structure-overhead control**, the analogue of S43's N=1024 vanish
  check.
- Rule 2 obligation: `clang -S -O2 -march=armv8-a+sme2`, OFF vs B=32; confirm the nest survives and
  LLVM has not interchanged it back.

## 4. The flag

- `EmitOpts::move_panel: Option<(u64, u64)>`, default `None`. The doc states the type-honesty split
  explicitly (the `Sme::panel_l1d_ratio` precedent): **W is program geometry fed by hand — the
  instrument's concession, retired by the §2(a) record if this ever ships; B is swept policy, never
  derived.**
- Note recorded in the doc: the L1-capacity derivation of B (`128 KB / 4 KB row = 32`) and the
  **line** derivation (`128 B / 4 B = 32`) **coincide at f32**. Indistinguishable at one element
  width, so neither is claimed; the sweep decides.
- `examples/emit.rs`: `--move-panel=<W>:<B>`, malformed-form error like `--target=`.
- **B lives on the flag, not in `TargetProfile`** — deliberately, so a refuted instrument leaves no
  constant behind.

## 5. Value gates, all **before any timing**

1. Full-stdout equality OFF vs ON for every (shape, size, B) cell.
2. `transpose_16` with `--move-panel=16:4` and `16:8` under several slice counts, forcing mid-row
   slice boundaries through head/mid/tail — the partition-correctness test.
3. A text-level pin in the golden-`.ll` test: flag OFF emits today's text; flag ON contains the nest.
4. **Byte-identity**: `benches/emit_sweep_ab.sh`, baseline binary vs new binary, **no flags** — 0
   diffs. Expect **156 real cells + 3 known `examples/vector.mapal` parse failures**.
   **Rule 23 injection**: re-run *with* `--move-panel=...`; the gate must flag exactly the predicted
   cells. A gate that cannot fail is not a gate.

## 6. The instrument that prices the mechanism first — `benches/shapes/tblock.c`

Rule 3 says a probe prices, it does not settle. But it can also **select which rung is worth
building**, and here it does. One loop body, `bs` and `lda` runtime arguments, both buffers written
before timing, 1 s warm, arms round-robin, a null control measured back-to-back inside every cell.

Two axes in one rotation:

- `bs` — square blocking of the traversal (what the emitter rung would do).
- `pad` — the read array's **row stride**, `lda = side + pad`, traversal untouched.

**`pad` is the mechanism test.** If padding alone recovers the win, the effect is set-index conflict
and not working-set size. If only `bs` recovers it, the framing three sessions were built on is
right after all.

## 7. Thresholds and predictions — declared before the timed runs

**CONFIRM** per cell: ON median ≥1.10× faster than OFF with disjoint min/max, null arm flat, identity
arm (B=W) overlapping OFF. **REFUTE**: every B overlaps-or-loses with nulls flat. **VOID**: the null
moves disjointly. Nothing under 6% is a result on this machine.

**The predictor**, written down before the runs: a walk with byte stride `S` advances the L1D set
index by `(S/128) mod 128` and reaches `128 / gcd(·, 128)` distinct sets when `128 | S`.

| shape | byte stride | sets | prediction |
| --- | ---: | ---: | --- |
| saxpy, reduce | 4 | 128 | no win |
| gather | data-dependent | ~128 | no win; prices nest overhead only |
| FIR | 4 | 128 | no win |
| conv2d (1026 wide) | 4104 | 128 | no win |
| **conv2d at a 1024-wide image** | **4096** | **4** | **should COLLAPSE** — a live landmine |
| matmul SME | packed, unit stride | 128 | no win; **explains `kc`'s 0.785×** |
| **transpose 1024** | **4096** | **4** | **big win** |
| **transpose at an ODD side** | not a multiple of 128 | 128 | **fast unblocked, blocking buys nothing** |

Emitter-cell predictions: transpose 1024 1t best-B ≥1.2× disjoint; transpose 1024 par at the noise
floor; B=W ≈ OFF everywhere; gather 0.95–1.0× overlapping.

## 8. What is NOT claimed

- No claim that any of this ships. The flag is default-OFF; the shippable rung needs the §2(a)
  record, which is a **report**, not this session's work.
- No claim about matmul beyond the existing record. S44 adds only the byte-identity proof that the
  new flag cannot touch it.
- Trap order within a slice is reordered under the flag (unobservable in trap-free programs).
- Threaded sub-6% movements are reported as **bounds**, never numbers.

## 9. Work order

1. Base gates: `cargo test --workspace --release` (expect 1032/0), `cargo fmt --all --check`.
2. Instrument first (§6) — it selects whether the rung is worth building at all.
3. Emitter change behind `move_panel`; golden pin.
4. Byte-identity, no-flag: 0 diffs. Rule-23 injection with the flag: exactly the predicted cells.
5. Value gates (§5).
6. Assembly check (§3).
7. Timed sweeps, every number appended as it lands.
8. Reconcile §7 line by line.

## 10. RECONCILIATION — scored against the measurements

Full tables in `benches/results-s44/l1-micro-panel.md`. Summary of where the plan was right and
where it was wrong:

| plan claim | outcome |
| --- | --- |
| transpose is the one real gap in the ladder | **held** — 2.56× (side 1024) / 3.19× (side 2048), disjoint, control clean |
| at side 1024 capacity cannot be the mechanism (§1 correction 2) | **held, and it was the key move.** The winning block is 32 rows × 128 B = **4 KB, 1/32 of L1D** |
| the mechanism is set-index conflict, not working-set size (§6) | **held.** Padding alone, fully unblocked, recovers 2.59× of the 2.71× blocking gets; `{bs=16, pad=16}` is *worse* than either alone, so it is **one defect with two treatments**, not two effects |
| transpose at an odd side runs fast untreated | **held, predicted in advance** — side 1025 unblocked 0.386 ms vs side 1024 unblocked 0.820 ms, **2.12×, disjoint**, and blocking then *hurts* (bs=16 = 0.623×) |
| **conv2d at a 1024-wide image should collapse** | **REFUTED — 1.018×, overlapping.** The predictor was over-general |
| `kc`'s 0.785× is the same predictor at unit stride | **held as an explanation**, not independently tested this session |

**The correction conv2d forced.** Stride alone predicts nothing. The rule that survives is:

> **A power-of-two stride hurts only when the traversal touches a small part of each line AND needs
> many lines live at once.** Pressure = *lines live at once* ÷ (*sets reachable* × *ways*).
> Transpose: 1024 live lines into 4 sets × 8 = 32 slots ⇒ **32× oversubscribed**. conv2d s1024:
> 96 live lines, but each image row is 32 contiguous lines — **exactly the set stride** — so the
> three rows tile sets `[s,s+32)`, `[s+32,s+64)`, `[s+64,s+96)` and never collide ⇒ **0.09×**.

That correction is the session's main result and it is worth more than the confirmation it replaced.
