# S44 — the L1 micro-panel: it is conflict, not capacity, and it is the first optimization that grows threaded

Date: 2026-07-31 · Machine: **Apple M4 Pro**, 10 P + 4 E, 2 SME units.
`hw.cachelinesize` = **128** · L1D 128 KB (1024 lines) · shared L2 16 MB · page 16 KB.
Baseline `6d6302c`, worktree `agent-aa6edc589b1483b51`. Plan:
`components/backend-llvm/plans/plan-s44-l1-micro-panel.md`. Every number:
`benches/results-s44/l1-micro-panel.md`. Instruments: `benches/shapes/{tblock.c, stride_ab.sh,
movepanel_ab.sh, transpose_vs_baselines.sh}`. Mutex: `benches/perflock.sh`.

S44 asked whether an L1-sized working set helps, hurts, or does nothing — for matmul **and for every
other shape in the ladder**, none of which had ever had a working-set constraint of any kind.

## 0. The headline

| | verdict |
| --- | --- |
| does sizing a working set to L1 pay? | **no — and that was never the question.** The winning block is **4 KB, 1/32 of L1D** |
| what actually pays | **breaking L1 set-index CONFLICT** under a power-of-two stride |
| how S43 missed it | S43 priced **capacity** (L1-vs-L2 = free, 32 KB → 8 MB) and `tlbreach.c` used **odd strides on purpose** to keep conflict out. The one cache mechanism with no number against it |
| shapes where it pays | **transpose, and only transpose** — 2.71× standalone, **1.578× emitted at 1 thread, 1.993× emitted threaded (disjoint)** |
| shapes where it cannot | saxpy, reduce, gather, FIR, conv2d, matmul — with arithmetic, not assertion (§2) |
| the predictor | `pressure = lines_live / (sets × ways)`. **Predicts sign and ordering; NOT magnitude** |
| tested predictively, 4 times | odd side ✓ · conv2d at a 1024-wide image ✗ **(killed the first version)** · side 128 ✓ · side 544 ✓ |
| the rung | `EmitOpts::move_panel`, default OFF — **a permutation of the loop counter, not a loop nest** |
| threaded behaviour | **the win GROWS with thread count** — the first optimization in this project that does, and the reason is structural (§5) |
| the ladder boundary | **transpose beats the C++ leg 3.53× threaded, disjoint.** The README's "transpose and gather still go to C++" now holds for gather only |

## 1. The mechanism, settled by an arm that changes nothing about the traversal

`benches/shapes/tblock.c`: one loop body, `bs` (block) and `lda` (read-array row stride) both runtime
arguments, both buffers written before timing, 1 s warm, arms round-robin, a null control measured
back-to-back inside every cell. Assembly-verified (rule 2): the only versioned inner loop is guarded
on **`lda == 1`**, and every arm runs `lda ≥ 1024`, so one loop body serves them all.

side 1024, 15 cycles, 1 thread, **control spread 1.006× — clean**, values identical every arm:

| arm | median ms | vs reference |
| --- | ---: | ---: |
| unblocked, unpadded | **0.820** | 1.000× |
| **blocked** bs=24 | **0.303** | **2.706×** |
| **padded** pad=16, **traversal untouched** | **0.317** | **2.587×** |
| both, bs=16 + pad=16 | 0.344 | 2.384× — **worse than either alone** |

**Padding the read array's row stride by one to thirty-two elements, with the traversal completely
unblocked, recovers 2.59× of the 2.71× that blocking gets.** The iteration order was never the
problem. And the two levers **do not compose** — which is what proves it is *one* defect with two
treatments rather than two effects.

The arithmetic: at side 1024 the read stride is `1024·4 = 4096 B = 2¹²`. With 128 B lines the set
index advances `4096/128 = 32` sets per read, so over a 128-set L1D the walk lands on
`128/gcd(32,128) = 4` distinct sets. Capacity is 1024 lines; the walk can use `4 × ways`.

## 2. The predictor, and the shape ladder as its evidence

**First version, and it was wrong:** *"blocking pays iff the number of reachable sets is small."*
It retrodicted all seven shapes. Then it was tested.

**`conv2d` at a 1024-wide image killed it.** Two Mapal sources identical except the image row stride
(`conv2d_s1024.mapal` / `conv2d_s1026.mapal`, same 1022² output, same 9 taps). Gate before timing:
the emitted `.ll`s differ on 332 lines raw but **0 lines once every integer literal is normalised** —
same instructions, same rung, same 9-tap unroll. Predicted: a cliff at stride 1024. Measured:
**0.3692 vs 0.3627 ms — 1.018×, overlapping. No cliff.**

**The corrected rule counts lines, not sets:**

> **A power-of-two stride hurts only when the traversal touches a small part of each line AND needs
> many lines live at once.** `pressure = lines_live / (sets_reachable × ways)`.

| shape | lines live | sets | slots @8-way | pressure | predicts | measured |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| **transpose 1024** | **1024** | 4 | 32 | **32×** | big win | **2.71× ✓** |
| conv2d, stride 1024 | 96 — but each image row is 32 contiguous lines, **exactly the set stride**, so the three rows tile sets `[s,s+32)`, `[s+32,s+64)`, `[s+64,s+96)` and never collide | 128 | 1024 | 0.09× | no win | **1.018× ✓** |
| FIR (64 taps, 508 B window) | 4 | 128 | 1024 | 0.004× | no win | ✓ |
| saxpy, reduce | 1 | 128 | 1024 | ~0 | no win | ✓ |
| gather | data-dependent | ~128 | 1024 | ~0 | no win | ✓ |
| matmul SME | packed panels, unit stride, lines consumed 32/32 | 128 | 1024 | ~0 | no win | `kc` at exactly L1D = **0.785×** ✓ |

**The negative matmul prior and the positive transpose result are the same rule at two strides.**
Packing linearises both operand panels to unit stride — **the pack was already the conflict fix** —
so there was never a collapse left for `kc` to remove, which is why sizing `kc` to L1D lost.

### Then it was made quantitative and tested three more times

| test | prediction | result |
| --- | --- | --- |
| transpose at side **1025** (odd ⇒ no collapse) | fast unblocked, blocking buys nothing | **✓ 2.12× faster unblocked than side 1024, disjoint; blocking then HURTS (bs=16 = 0.623×)** |
| side **128** (power of two, pressure **0.5**) | no win | **✓ 1.000×** — but 4 µs cells at the timer floor, and both arrays = L1D exactly. Weak, and said so |
| side **544** (`gcd(17,128)=1`, pressure **0.53**, 1.1 MB/array) | no win, off the floor, no confound | **✓ 1.19× best, and bs=8 HURTS at 0.830×** |

**Side 128 + side 544 together are what separate the candidate rules.** Side 128 *does* have a set
collapse (32 of 128 sets) and still shows no win — so it is **not** "power-of-two strides are bad"
and **not** "a collapse is bad". It is pressure, and only pressure.

**And the magnitude half is refuted: the relationship saturates.** Pressure 0.5 → 1.00×, 2 → 2.00×,
8 → 2.09×, 32 → 2.71×, 128 → 3.19×. Pressure 2× already buys two thirds of what 128× buys, because
once the walk is oversubscribed *at all* every line reuse is lost and amplification jumps to its
ceiling; more pressure cannot lose the same reuse twice.

> **`pressure` predicts the SIGN and the ORDERING of a blocking win, never its magnitude. Below 1
> there is nothing to win; above ~2 the win is already near its ceiling.**

An absolute number makes the point better than any ratio: untreated side **544** runs at
**26.9 GB/s**; untreated side **512** — one power of two away — runs at **15.2 GB/s**. 1.77× apart,
with no treatment on either.

## 3. The rung — a permutation of the counter, not a loop nest

`EmitOpts::move_panel: Option<(u64, u64)>`, default `None`; `--move-panel=<W>:<B>`.
`crates/backends/llvm/src/func/bulk.rs::FnEmit::move_panel_index`, +73 lines.

```text
p = ((rb·CB + cb)·B + dr)·B + dc      -- the loop counter, decomposed
t = (rb·B + dr)·W + cb·B + dc         -- the index it stands for
```

**That shape is the entire correctness argument, and it is why the diff is 60 lines instead of 300.**
`perm` is a bijection of `[0, n)`; the parallel slices partition the counter; so their images
partition the outputs. Every element is visited exactly once, by exactly one worker, values
bit-identical — and **`%lo`/`%hi` need no handling at all.** A blocked *nest* would have needed a head
and a tail arm for the partial rows at every slice boundary.

Every divisor is a compile-time constant, so `-O2` turns the `udiv`/`urem` pairs into shifts. The
rung **declines** (returns the counter unchanged) unless the panel divides the geometry both ways.

**`w` is program geometry supplied by hand, and that is a reported limitation, not a design.** The
emitter cannot derive it: `mapal_ir::algo::tile_site` **hard-requires a fold in the map body**, and a
transpose-shaped map has none, so no record carries its 2-D width; `ElemSrc` has no representation of
a captured read's index expression. Deriving it means **a fold-less move-site record in `mapal-ir`**
— scoped in §6 as the open question, deliberately not built here.

## 4. Measured in the real pipeline, 1 thread and threaded

`benches/shapes/movepanel_ab.sh`, `-O2 -march=armv8-a+sme2`. Gates first: **values identical to OFF
at every arm**, and **every arm's emission differs from OFF**. That second gate fired on my own arm
list — B=24 emitted text byte-equal to OFF because `1024 % 24 ≠ 0`, and it was voided rather than
tabled as "no effect", which is exactly the failure that would have made every null here meaningless.

| arm | 1 thread min/med/max | threaded min/med/max |
| --- | --- | --- |
| **off** | 0.8046 / **0.8996** / 1.9391 | 0.2264 / **0.2890** / 0.3848 |
| B=8 | 0.5606 / 0.5736 / 1.0734 | 0.1285 / 0.1673 / 0.2694 |
| **B=16** | 0.5591 / **0.5700** / 0.9672 → **1.578×** | 0.1304 / **0.1450** / 0.1924 → **1.993× DISJOINT** |
| B=32 | 0.5899 / 0.6244 / 0.9070 | 0.1357 / 0.1875 / 0.2343 |
| B=128 | 0.8719 / 0.9773 / 1.2659 → **0.920×, loses** | — |
| **B=W (identity)** | 0.8112 / 0.8469 / 1.1440 → **overlaps OFF** | 0.2368 / 0.2664 / 0.4413 → **overlaps OFF** |
| saxpy (null) | 0.0992 / 0.0998 | 0.0758 / 0.0996 |

- **The 1-thread ranges OVERLAP** ([0.8046, 0.9672]); each cycle is a separate `exec` and the maxima
  carry launch noise. Reported as a median result, **not** as disjoint. Threaded **is** disjoint.
- **The identity arm overlaps OFF at both thread counts**, so the permutation arithmetic costs
  nothing measurable — the win is the traversal.
- **The probe over-priced the integrated win by 1.7×** (2.71× standalone → 1.578× emitted). Rule 3
  again, and the same direction as S42's `kc.c` (predicted 1.448×, delivered +6.1%).

## 5. THE STRUCTURAL FINDING — a per-core conflict is not an Amdahl term

**The threaded prediction written down before the run was refuted.** I predicted the win would
*shrink* threaded (≥1.3×), on the S43 pattern. It **grew**: 1.578× → **1.993×**. OFF scales 3.11× on
the full pool; ON scales **3.93×**. **The set conflict was itself limiting parallel scaling** — L1D is
per-core, and the walk `a[j·S + i]` sweeps all `S` rows of `a` no matter which output rows a worker
owns, so pressure does not dilute with thread count.

That places it in a third category, and the three of them **predict thread-count behaviour before
measurement**:

| what is removed | 1 thread | threaded | why | example |
| --- | ---: | ---: | --- | --- |
| a **shared bottleneck** in a term threading already shrank | large | **shrinks** | smaller share of the threaded wall | `kc` +6.1% / **−25.5%**; residency +71% / **+5%**; `nc` +18.7% / **parity** |
| a **serial fraction** | nothing | **grows** | Amdahl, in reverse | parallel B pack 0.998× / **1.381×** |
| a **per-core resource conflict** | real | **grows** | every core suffers it independently | **move panel 1.578× / 1.993×** |

> **24. Classify what an optimization removes — serial fraction, shared bottleneck, or per-core
> resource — and its thread-count behaviour follows.** S43's "three for three vanished threaded" was
> not bad luck; all three were the same category. This would have saved S42 and S43 real work.

## 6. What is NOT claimed, and what is open

- **`w` is not derived.** The rung is an instrument until `mapal-ir` grows a fold-less move-site
  record. **Open P1, scoped:** extend `tile_site`'s family with a fold-less arm — map body = a single
  proven `Index` whose address is affine in `(t÷C, t%C)`. `tile_split`/`TileAffine` already do exactly
  this one level up. Backend-local pattern matching was rejected: the emitter must not re-derive graph
  analysis.
- **Nothing here ships on by default**, and no constant is left behind: `B` lives on the flag, not in
  `TargetProfile`, deliberately.
- **The 8-way associativity is assumed, not measured.** `hw.cachelinesize` and L1D size are read from
  `sysctl`; ways are not exposed. The *ordering* results are robust to the assumption (they depend on
  `gcd`, not on `ways`); the absolute pressure numbers scale with it.
- **`pad=33` at side 1024 (0.906×, a loss) is unexplained.** `lda=1057` gives a 4228 B stride that is
  not 128 B-aligned; it also has the widest spread in that table. Flagged, not interpreted.
- **The published ladder row does not reproduce** (rule 19): C++ mt measures 0.4963 median today
  against a carried 0.26, and Mapal OFF 0.3718 against a carried 0.290. The C++ mt leg spans
  0.3351–0.7407, a 2.2× spread, so **OFF-vs-C++ threaded is not resolvable** — only ON-vs-C++ is.
- **Only transpose was measured with the rung on.** Every other shape's verdict rests on the
  arithmetic in §2 plus the byte-identity proof that the flag cannot reach a recognised tile site —
  which is *stronger* than timing them, since S39 measured −5.9%…+1.2% between byte-identical
  binaries on this machine.
