# conv2d's per-core gap — RESOLVED: a measurement boundary, not a kernel defect

**Status: RESOLVED (S33).** Flow's conv2d kernel is **not** slower than the naive C++ baseline.
Measured with exact hardware counters, it is **18% fewer cycles and 22% less real time** than
C++'s, at **higher IPC**. The recorded 1.55× "per-core gap" is an artifact of what each side's
timed window contains: **Flow's `time`-bracketed window includes the output array's first-touch
page-zeroing; the C++ baseline pre-pays that outside its timed region** (`std::vector<float>
out(side*side)` value-initialises, touching every page, before `run_iters`).

This closes the diagnosis that S31/S32 left open after eliminating eight in-kernel hypotheses.
All eight were looking inside the kernel. The cause was never in the kernel.

Basis: i9-14900F (AVX2, `perf`), clang-cross `-march=raptorlake`, conv2d 3×3 over 1024×1024 f32,
`FLOW_PAR=1`, pinned `taskset -c 2`, min-of-N. Cross-confirmed on M4 Pro (NEON), clang 22.1.8,
`-O2 -march=native -ffp-contract=fast`.

---

## 1. The kernel, by exact hardware counters

Counters are exact (`perf stat`, not sampled) and **differenced** so each row is the kernel alone:

- **flow** = `conv2d_1024.flow` minus a gen-only build (`genonly.flow`: byte-identical program with
  the convolution map removed), 200 back-to-back runs each.
- **cpp** = `cppb conv2d 1t 200 1024` minus `cppb conv2d 1t 1 1024`, divided by 199.

Both legs warm, pinned, `:u`-filtered, median of 5.

| leg | cycles | **ref-cycles** | instructions | **IPC** | clock (×TSC) |
| --- | ---: | ---: | ---: | ---: | ---: |
| flow `task7` | **905,100** | **299,221** | 2,034,188 | **2.25** | 3.025 |
| cpp `conv_range` | **1,072,928** | **382,489** | 1,914,675 | **1.78** | 2.805 |

`ref-cycles` ticks at the fixed TSC rate, so it is **frequency-invariant real time** — the column
that settles the question. TSC on this part is 2.0 GHz, which the method validates against C++:
382,489 ref-cycles = **0.191 ms** versus its self-timed 0.194 ms, i.e. **the C++ window is 98%
kernel**. Applying the same conversion to Flow gives 299,221 ref-cycles = **0.150 ms** against a
self-timed window of **0.258 ms**.

**Flow's kernel occupies 0.78× the real time C++'s does. Flow's own `time` builtin reports it
1.33× slower. The ~0.108 ms difference is inside `t0..t1` and is not user-mode execution.**

### The recorded "IPC 3.11 vs 1.57" was process-level

That pair was measured over whole processes and is not a kernel figure. Flow's process IPC is
dragged down by its generation legs (`task0` IPC 1.04, `task2` 0.86) — and the two programs'
generation phases are not comparable at all: C++'s scalar `%`/`/` gen is ~18.7M instructions,
Flow's vectorised gen is ~2.5M. **Per kernel, Flow's IPC is 2.25 and C++'s is 1.78** — the
opposite ordering. Do not quote process-level IPC for a kernel claim again.

---

## 2. Where the missing 0.108 ms goes

`:u`-filtered counters exclude kernel-mode time. Differencing **page faults** between the full and
gen-only builds attributes exactly **3 huge-page faults (6 MB) to the conv map** — the 4 MB output
array. THP is `[always]` on this box, so each fault costs the kernel a 2 MB zeroing.

Measured directly (`mmap` 4 MB anonymous, first-touch every page, `taskset -c 2`):

| | ms |
| --- | ---: |
| first-touch 4 MB, cold | 0.303 |
| first-touch 4 MB, warm | 0.084 – 0.110 |

**0.108 ms unaccounted for; 0.084–0.110 ms measured. The accounting closes.**

## 3. The falsifiable prediction, run on both architectures

If the cause is the boundary and not the kernel, then a **C++** binary running the **naive** kernel
must reproduce the whole gap purely by moving where the output array is first touched. Same source,
same binary, one `if`; mode 0 is what `shapes_baseline.cpp` does, mode 1 is what Flow does.

**i9-14900F (AVX2), min-of-40 cold / min-of-150 warm:**

| mode | cold | warm |
| --- | ---: | ---: |
| 0 — out pre-faulted **outside** the window | 0.2118 | **0.1924** |
| 1 — out first-touched **inside** the window | 0.2862 | **0.2720** |

Delta **+0.080 ms warm** — matching §2. Mode 0 reproduces the real `cppb` baseline (0.2102 /
0.1944) to within 1%.

**M4 Pro (NEON), min-of-40:**

| mode | ms |
| --- | ---: |
| 0 — out pre-faulted outside the window | **0.2616** |
| 1 — out first-touched inside the window | **0.4513** |

Compare against the recorded M4 figures this file previously carried: **cpp 0.256** (≈ mode 0) and
**flow 0.395–0.426** (≈ mode 1, and *faster* than it). A C++ binary reproduces the M4 gap at
**1.72×**, larger than the 1.54–1.66× ever attributed to Flow.

**Why the gap looked architecture-independent.** S31 recorded 1.54× on NEON and 1.55× on AVX2 and
concluded the cause was "structural in what we emit". The instinct was right and the structure was
the measurement boundary: a page-zeroing cost inside one side's timed window is a property of the
harness, so of course it does not vary with the ISA. **Two unrelated architectures agreeing to two
significant figures is evidence against a microarchitectural cause, not for one.**

---

## 4. The corrected comparison

Like-for-like, both ways of drawing the boundary put Flow ahead:

| basis | flow | cpp | verdict |
| --- | ---: | ---: | --- |
| i9, same boundary (both pay first touch inside) | 0.2576 | 0.2720 | **flow 1.06×** |
| i9, kernel only (ref-cycles, boundary removed both sides) | 0.150 | 0.191 | **flow 1.28×** |
| M4, same boundary (both pay first touch inside) | 0.395–0.426 | 0.4513 | **flow 1.06–1.14×** |

## 5. Is the cost real, or is it only an artifact?

Both, and the distinction is the point:

- **As a kernel claim it is an artifact.** The cost is allocation, not convolution. Attributing it
  to a "per-core kernel gap" sent five sessions inside the kernel.
- **As an end-to-end cost it is real.** A Flow program genuinely pays first-touch on every array it
  allocates. It is paid **once per array**, so it amortises to nothing in any repeated or
  longer-running workload, and it is paid by the C++ program too — just before the clock starts.

What remains actionable is therefore a **harness** question, not a kernel one: the Flow shapes
benches bracket allocation-plus-compute while the baselines bracket compute only. See §7.

---

## 6. Method notes worth keeping

- **The repeat-loop bench is not a prerequisite** (it was carried as P0 for this diagnosis, and it
  is now moot). Both kernels are distinct symbols — Flow's `task7` and C++'s `conv_range` (the
  gcc-linked baseline keeps it out-of-line; on the 1t path clang inlines it into `main`, so drive it
  through `THREADS=1 mt` to split the symbol). `perf record` + symbol attribution reads per-kernel
  counters straight out of the diluted process, and **exact differencing against a gen-only build is
  better still** — no sampling error at all.
- **`ref-cycles` is the tool for "is this frequency or is this time".** `cycles` alone cannot tell a
  slower clock from more work; `ref-cycles` is fixed-rate. It is what proved the missing time was
  outside user-mode execution rather than a downclock.
- **Cross-validate sampled against exact.** Sampled per-symbol instructions came out 2,034,967 vs
  exact 2,034,188 — 0.04%. That agreement is what licensed trusting the rest.
- **Single runs on this box are worthless.** Mid-session, single cold runs read flow 0.464 / cpp
  0.196 and produced a confident "2.37× cold, 1.12× warm" reading that min-of-40 then refuted
  (1.56× cold, 1.33× warm). Every number in this file is min-of-N. *This was an error made and
  caught inside this session; it is recorded so the shape of it is recognisable.*
- **`calloc` is not a pre-fault.** An early version of the §3 probe used `calloc`/`malloc` and
  showed no difference, apparently refuting the hypothesis. `calloc` on a 4 MB request returns a
  fresh mapping whose pages are still untouched, so *both* modes faulted inside the window — and
  glibc's region was not 2 MB-aligned, so it took ~1024 small faults instead of 3 huge ones and ran
  0.52 ms. Only `std::vector`'s value-initialisation actually pre-faults.
- **Boost ramp is a real but secondary effect here.** Cold/warm on the i9: flow 0.3278/0.2576
  (1.27×), cpp 0.2102/0.1944 (1.08×). Flow gains more from a warm core because the page-zeroing it
  pays inside the window is itself much cheaper warm (0.303 → ~0.10 ms).

## 7. The fix, shipped (S33)

Sapir's call: **standardise on a compute-only window by making Flow pre-fault**, matching every
baseline. Shipped as `reside` in `crates/flow-rt/src/lib.rs`, called from `flow_rt_alloc` —
plan `components/backend-llvm/plans/plan-s33-timed-window-boundary.md`.

`flow_main` emits exactly **one** arena call for the whole program, in the entry-block prologue
above every task and both clock reads, so making `flow_rt_alloc` hand back a *resident* block
fixes every heap-lowered program at once. **No emitter change, no flow-ir change, no `.ll`
moved.** One byte written per 4 KiB — not a memset: the fault's own zeroing is the
initialisation, so a memset would zero every page twice (~15 ms wasted on a 64 MB matmul frame).

### Result

| leg | i9 warm | M4 |
| --- | ---: | ---: |
| flow, before | 0.2586 | 0.395–0.426 |
| **flow, after** | **0.1440** | **0.2111** |
| cpp baseline (pre-faults, unchanged) | 0.2072 | 0.2616 |
| **verdict** | **flow 1.44× ahead** | **flow 1.24× ahead** |

The window landed on **0.1440 ms** against the **0.150 ms** that §1's `ref-cycles` counters
predicted for the kernel *before the fix existed* — a 4% agreement between two fully independent
methods, which is the strongest evidence in this file that the diagnosis was right.

Two further confirmations: the **cold/warm ratio went 1.15 → 1.01** (flat — the cold-fault cost
left the window entirely, and Flow is now steadier than cpp's 1.08), and **total process wall
time is unchanged**, 1.485 → 1.498 ms (+0.9%, the ~4k extra stores). *The fix makes nothing
faster. It moves a real cost out of a window that was never meant to contain it.*

Gate: 72 suites green, `fmt` clean, conv2d stdout `576/-96` byte-identical before and after.

## 8. What is left

| item | note |
| --- | --- |
| **Re-measure conv2d and fir through the harness** | The artifact scales as *output size ÷ kernel length*, so corrections are large and uneven: conv2d_1024 1.87–2.02×, conv2d_512 1.60×, fir_65536 2.07× (M4, 1t). All of them **understated Flow**. Do **not** publish these hand-linked figures — re-run `shapes_ab.sh`. |
| matmul | Expected near-immune (~20 ms kernel against the same 4 MB output ⟹ boundary under 1%), but **unverified**. Check before assuming its rows stand. |
| `README.md` conv2d row | Currently reports Flow losing to naive C++ on this shape. It wins. Blocked on the harness re-run above — nothing goes in the README that a default build has not been measured delivering. |
| **NUMA first-touch** | `reside` runs on the main thread, so on a multi-socket host every page lands on one node, where a worker's own first touch would have distributed them. Irrelevant on i9/M4 (single socket), live on a dual-socket EPYC — i.e. the vast.ai class. Before any multi-socket parallel run: A/B it, or make `reside` lane-aware (which is the NUMA-correct form of the same invariant). |
| The eight in-kernel hypotheses | All correctly refuted, and now explained: they were refuted because the kernel was never the problem. Keep the list; still a do-not-re-run. |
| Fold tap 0 into `fmul` | Unrelated and still worth doing: 16 of 274 instructions (~6%), and both kernels waste it. |
