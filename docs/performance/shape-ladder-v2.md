# Shape ladder v2 — algorithm classes beyond matmul/fir/conv2d

Status: **PLAN — S35, 2026-07-26.** Written before any benchmark exists, per FRAMEWORK §6.1.
Driver (Sapir): *"add more different algorithm types that can be relevant and measure that,
fir/conv2d are only 2 — I wanna see generalizations in different types of algo (maybe not only
compute?)"*

## Why

Every published number today comes from three shapes — matmul, fir, conv2d — and all three are
the **same class**: dense, compute-bound, affine reads, perfectly parallel. They exercise one
corner of the compiler (`tile_plan`, register blocking, FMA) and say nothing about the rest of
it. A reader is entitled to ask whether the thesis generalizes or whether the compiler is three
kernels in a trench coat.

The claim under test is not "Mapal is fast at GEMM". It is that **the facts are deduced from
the graph and handed to every backend**. That claim is only interesting if it survives shapes
where the bottleneck is *not* arithmetic: bandwidth, latency, irregular access, sequential
dependence, and scatter.

## What the language can express

Core ops (`crates/mapal-ir/src/graph.rs`): `Add Sub Mul Div Mod Neg`, comparisons, `And Or Not`,
`Phi`, `Map Fold Index Zip Enumerate Iota Fill Update`, guard-first `loop`, `Widen`, `time`,
`print`. **No `sqrt`/`exp`, no bit operations.** That rules out n-body, Black-Scholes and CRC
without an ADR; it leaves everything below.

## The ladder

Each row names the bottleneck it is chosen for, not the domain it comes from.

| # | Shape | Class | Bottleneck under test | Expressible today |
| --- | --- | --- | --- | --- |
| 1 | **saxpy** `y = a·x + y` | streaming | **memory bandwidth**; zero reuse, one FLOP per two loads | yes — `zip` + `map` |
| 2 | **reduce** `Σx`, `max x` | reduction | **fold shape + tree reduction** (ADR-0028); no output array, so nothing for `tile_plan` to block | yes — `fold` |
| 3 | **transpose** `B[j][i] = A[i][j]` | data movement | **cache behavior with zero arithmetic**; the read is affine, the write is not | yes — `iota` + `map` with `div`/`mod` |
| 4 | **gather** `y[i] = x[idx[i]]` | irregular reads | **indirection**: defeats `bounds_proof`, defeats vectorization, prefetch-hostile | yes — `Index` on a computed index |
| 5 | **scan** (prefix sum) | sequential dependence | **the anti-parallel case** — every element depends on the last; `path_plan` should find no width | yes — `loop` + `Update` |
| 6 | **histogram** `h[bin(x)] += 1` | scatter | **data-dependent writes**; parallel form needs privatization the compiler does not have | yes sequentially; the parallel refusal is itself the result |
| 7 | **mandelbrot** | divergent compute | **data-dependent trip count** per element | needs a `loop` inside a `map` body — **verify before promising** |
| 8 | **binary search** (batched) | latency + branch | **no vectorization, guard-heavy**, dependent loads | fixed-depth form: yes |
| 9 | **bitonic sort** | compare-exchange | data movement + parallel structure over stages | yes — `map` + `Update` per stage |

Rows 1–4 are batch one: each is unambiguous to express, has an obvious C++ and NumPy baseline,
and covers a distinct bottleneck. Rows 5–9 follow once batch one is measured and honest.

## What counts as a result

The same rules as every other number in this repo, restated because new shapes are exactly where
they get broken:

- **Compute-only.** Data generation lives outside the `() -> time` bracket on every leg, Mapal
  and baseline alike. The S28 gen-boundary finding is what forced this.
- **Same machine, same run, both legs.** Ratios within one run; never against a recorded number.
- **min for single-threaded, median for threaded** until the `mapal_par_wait` race is fixed.
- **A losing cell is published.** Rows 3–6 are expected to lose or tie against a tuned baseline;
  a ladder that only lists wins is marketing, and the point of these shapes is to find where the
  deduction stops paying.
- Baselines stay **naive-but-fair**: the obvious C++ loop at `-O3 -march=native`, threaded the
  same way, plus NumPy where a real vectorized primitive exists (`np.add`, `np.sum`, `.T.copy()`,
  fancy indexing, `np.cumsum`, `np.bincount`).

## Predictions, recorded before measuring

Written down so the results can contradict them:

1. **saxpy** — everyone lands on the same memory bandwidth; expect a tie with C++ and NumPy, ±10%.
   If Mapal is materially slower, the cost is in the emitted loop, not the algorithm.
2. **reduce** — competitive at one thread. Threaded depends on whether the fold splits; ADR-0028
   says exact-op folds may tree-reduce, so this measures whether that path fires here.
3. **transpose** — expected **loss** against a blocked C++ transpose. There is no `tile_plan` rung
   for a pure permutation, so Mapal should emit the naive nest and take the cache misses.
4. **gather** — expected tie-to-loss; the interesting number is how much the *removed* bounds
   checks buy, since `bounds_proof` cannot prove a data-dependent index.

If 3 and 4 lose, that is the honest shape of the claim: the deduction pays on structured reuse
and does nothing for permutation and indirection — *yet*. Both are candidate rungs afterwards.

---

## Results — batch one, M4 Pro (10 P + 4 E), 2026-07-26

`RUNS=9`, `bash benches/shapes/ladder2_ab.sh`. Compute-only on every leg; min for 1t, median for
par. All three legs print identical values on all four shapes before any timing is taken, so the
comparison is like-for-like.

| shape | class | mapal-1t | mapal-par | cpp-1t | cpp-mt | numpy |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| saxpy 1M | streaming | 0.551 | **0.172** | 0.148 | 0.127 | 0.162 |
| reduce 1M | reduction | 0.620 | 0.584 | 0.508 | 0.892 | 0.101 |
| transpose 1024² | data movement | 1.008 | **0.229** | 0.761 | 0.251 | 0.779 |
| gather 1M | irregular reads | 0.559 | **0.140** | 0.476 | 0.154 | 2.043 |

Threaded: Mapal wins transpose (1.09×) and gather (1.11×), loses saxpy (1.36×). Single-threaded
it loses all four, by 1.17–3.72×.

## The finding: streaming kernels emit scalar loops

saxpy at one thread is **3.7× behind naive C++**, which is far outside the range of the other
rows. Two candidate explanations, both tested rather than argued:

1. **`zip` materializes an intermediate pair array** -> rewriting saxpy to index both arrays from
   the lane id instead of zipping gives 0.436 ms against 0.522 ms. Real, ~17%, **not** the gap.
2. **The kernel is not vectorized.** Disassembly of the emitted `task6` (the saxpy kernel): one
   `fmul`, **zero `q`-register loads or stores**. A scalar loop. The generation task in the same
   binary (`task0`) is full-width NEON, so this is not a toolchain flag.

So the vectorization that matmul, fir and conv2d get comes from the tile ladder recognizing those
sites. **A plain `map` over a zipped array is not a recognized site, so it emits scalar code**, and
3.7× is what 4-wide NEON is worth on a memory-bound kernel that still fits in cache. The threaded
column hides it: work-stealing across 14 cores recovers enough to land within 1.36× of C++.

That is a rung, not a defect — but it is the first measured evidence that the deduction is
*narrower* than the headline shapes suggest.

## Predictions vs outcomes

Recorded before measuring, in the section above:

| Prediction | Outcome |
| --- | --- |
| saxpy: tie ±10% | **Wrong.** 3.72× behind at 1t, 1.36× threaded — scalar kernel |
| reduce: competitive at 1t | **Roughly right** (1.22× behind). NumPy is 6× ahead because `np.sum` is pairwise; the fold is left-to-right by definition, so this is a semantics difference, not only a speed one |
| transpose: expected **loss** vs blocked C++ | **Wrong on threaded** — 1.09× ahead. Right at 1t (1.33× behind) |
| gather: tie-to-loss | **Wrong on threaded** — 1.11× ahead; NumPy's `np.take` is 3.7× *behind* Mapal-1t |

Two of four predictions were wrong in the direction of underestimating the threaded path, and the
one I was most confident about (saxpy tie) was the worst miss.

## Caveats on these numbers

- **Working sets are cache-resident.** 1M f32 = 4 MB per array; gather in particular measures an
  L2-resident gather, not a DRAM one. A 64 MB variant is the honest follow-up before claiming
  anything about irregular access at scale.
- `reduce`'s threaded C++ cell (0.892) is *slower* than its 1t cell (0.508): at ~0.5 ms of work,
  spawning threads per iteration costs more than it saves. Recorded as measured, not tuned away.
- Everything here is one machine (M4 Pro / NEON). The i9 cross-check that made the matmul story
  honest has not been run for these shapes.
