<p align="center">
  <img src="assets/logo-wordmark.svg" alt="Flow" width="320">
</p>

# Flow

<!--toc:start-->

- [Flow](#flow)
  - [The idea](#the-idea)
    - [What the compiler works out for you](#what-the-compiler-works-out-for-you)
  - [Where it stands](#where-it-stands)
    - [About the baselines, before the table](#about-the-baselines-before-the-table)
    - [Matrix multiply, f32](#matrix-multiply-f32)
    - [Other shapes](#other-shapes)
    - [Why NumPy wins matrix multiply — and the machine where it doesn't](#why-numpy-wins-matrix-multiply-and-the-machine-where-it-doesnt)
    - [Two builds, and which one the table used](#two-builds-and-which-one-the-table-used)
    - [Status](#status)
  - [Not just kernels](#not-just-kernels)
  - [Trying it](#trying-it)
  - [What is next](#what-is-next)
  - [How the project works](#how-the-project-works)
  - [License](#license)
  <!--toc:end-->

**A language where you describe _what_ to compute, and the compiler works out _how_.**

Flow programs are dataflow graphs. You write a chain of steps; the compiler reads the graph
and figures out the rest — what can run in parallel, what can be vectorised, how to block a
loop for cache. You never write a thread, a lock, a SIMD intrinsic, or a tuning pragma.

The property it is built around:

> **The same program gives byte-identical output whether it is compiled at `-O0` or `-O2`,
> and whether it runs on one thread or all of them.**

Not "close enough" — the same bits, checked against an interpreter that defines what the
language means. (Read that as an engineering discipline, not a theorem. Nothing here is
machine-proven, and the spec says so in as many words.)

```flow
fn main() {
    1048576 -> iota -> ts;
    ts -> map { t -> t * 2 } -> doubled;
    doubled[1048575] -> println;
}
```

That runs across every core of your machine. Nothing in the source says so.

---

## The idea

Most fast code is fast because a human told the machine how to be fast — tile sizes, thread
counts, vector widths, memory layouts. That knowledge gets written into the program, so it
does not survive a move to different hardware.

Flow splits the two halves apart:

|                                                                                                                | Where it comes from                                  | Portable?                                   |
| -------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- | ------------------------------------------- |
| **Geometry** — which values depend on which, what can run at once, which reads repeat, which axes can be split | **deduced from the graph**, exactly, by the compiler | **yes** — it is a fact about your program   |
| **Constants** — tile widths, thread counts, cache blocking                                                     | facts about the _machine_                            | no — and they do not belong in your program |

Your source only ever contains the first kind. The second kind is the compiler's problem.

**Being honest about the current state of that split:** the geometry half works and is the
part that has been proven across shapes. The constants half is not built yet — today they
are literals in the CPU backend, hand-tuned on one machine and used unchanged everywhere.
Moving them into a table of machine facts is the next piece of work, and until it lands the
portability claim above describes the design, not the artifact.

### What the compiler works out for you

- **What can run at the same time** → tasks and their dependencies, dispatched to a
  work-stealing thread pool.
- **Which loops are secretly matrix-shaped** → a register-blocked, vectorised,
  cache-blocked kernel, generated.
- **Which reads are repeats** → data packed once and reused instead of re-fetched.
- **Which array accesses can never go out of bounds** → those checks removed, and only
  those.

Each answer is a fact about your program, computed once and available to every backend.

---

## Where it stands

The tables below are from an **Apple M4 Pro** (10 performance + 4 efficiency cores), kernel
time only. The headline results — matrix multiply at every size, and conv2d — have now been
**reproduced on a second, unrelated machine**: an Intel i9-14900F (AVX2). That machine _changes
one of the conclusions below_, so it gets its own section further down.

**The statistic matters here.** Single-threaded cells are the _minimum_ of N runs. Multi-threaded
cells are the _median_, because a race in our work-stealing pool can let a kernel finish before
the clock that is supposed to bracket it starts, which makes a threaded run's self-timing
spuriously _short_. Roughly 3–4% of threaded runs are affected, so a minimum is the wrong
statistic for them and gets worse the more runs you take. It is a known P0 bug, not a property of
the generated code — details in `docs/performance/matmul/s33.md` §5.

### About the baselines, before the table

The C++ and Rust numbers come from **deliberately naive triple loops** — no blocking, no
intrinsics, no loop reordering; the inner loop strides down a column, which is the textbook
worst case. They are compiled `-O3 -march=native -ffp-contract=fast` and split across
threads, but they are what someone writes _before_ optimising. Beating them by a large
factor demonstrates that the automatic blocking works; it is not a claim about beating
expert-tuned code.

**The expert-tuned comparison is the NumPy column, and on this machine matrix multiply is where
we lose it** — by 3.4×. Hold that number: the second-machine section shows it is a fact about
Apple silicon rather than about our code generation.

### Matrix multiply, f32

| N    |        Flow | C++ (naive, threaded) | Rust (naive, threaded) | NumPy (1 thread) | NumPy (threaded) |
| ---- | ----------: | --------------------: | ---------------------: | ---------------: | ---------------: |
| 1024 | **2.23 ms** |                   140 |                    133 |             1.30 |             0.68 |
| 4096 |  **151 ms** |                33,065 |                 33,548 |             84.6 |             44.1 |

Against the naive baselines the margin grows with size — 63× at 1024², 219× at 4096² —
because they have no cache blocking and fall apart once the data stops fitting, while
Flow's blocking is generated. **Against NumPy we are 3.4× behind** at 4096.

### Other shapes

| workload               |         Flow | C++ (naive, threaded) | NumPy (1 thread) |
| ---------------------- | -----------: | --------------------: | ---------------: |
| FIR filter, 1M samples |  **0.27 ms** |                  1.42 |             6.10 |
| conv2d 3×3, 1024×1024  | **0.089 ms** |                  0.14 |             1.55 |

Two things to read carefully here:

- **conv2d used to be our loss, and the reason it looked like one is worth telling.** This row
  read _Flow 0.42 vs C++ 0.13_ for five sessions — a 3.2× loss — and we spent most of that time
  hunting a defect in the convolution kernel, eliminating eight hypotheses by measurement
  (register pressure, weight broadcasting, arithmetic intensity, pointer aliasing, alias
  metadata, and more). All eight were correctly refuted, because **the kernel was never the
  problem.** Our timed region included something the C++ baseline's did not: the first write to
  the output array, which is where Linux and macOS actually hand out the physical memory behind
  a fresh allocation. The kernel was paying for the operating system zeroing 4 MB of pages.
  `std::vector<float> out(n)` zero-fills on construction, so the C++ baseline pre-paid that cost
  _above_ its own timer. Making our allocator hand back memory that is already resident moved
  the cost to where it belongs — and the row inverted. Per core, conv2d is now **1.21× ahead**
  of naive C++ on _both_ NEON and AVX2 (measured separately on each). Two hardware counters
  agreed to within 4% on the diagnosis before the fix was written. Full account:
  `docs/performance/conv2d-per-core-gap.md`.
- **The NumPy comparisons are not like-for-like, and for these two shapes they cannot be.**
  Those are Flow on 14 cores against NumPy on one — not because the harness declines to run a
  threaded NumPy leg, but because **no threaded NumPy kernel exists for either shape**: FIR is
  `np.correlate`, single-threaded C that ignores the BLAS thread settings entirely, and conv2d is
  a Python loop over nine whole-array `out += w[k]*slice` passes — nine reads and nine writes of
  4 MB, so it is memory-bound rather than compute-bound. That column says more about the baseline
  than about NumPy. On the one shape where NumPy _does_ have a real threaded kernel — matrix
  multiply — we report it threaded, and we lose.

### Why NumPy wins matrix multiply — and the machine where it doesn't

Not a better compiler — **different hardware**. On this chip NumPy's matmul runs on a
matrix coprocessor, separate from the ordinary vector units. A single-threaded NumPy call
reaches about 11× the throughput one core's vector units can produce, and the threaded run
exceeds the whole chip's vector peak by 1.8×. No amount of tuning ordinary vector code
closes that; it needs code for that coprocessor, which is planned.

Our own kernel runs at roughly **75% of one core's vector peak**. That denominator assumes
4×128-bit fused-multiply-add pipes at about 4.4 GHz; Apple publishes neither number, so
treat it as a model rather than a datasheet.

That was the explanation. Here is the test of it. Run the identical benchmark on a machine with
**no matrix coprocessor** — an Intel i9-14900F, where NumPy goes through OpenBLAS 0.3.30 on the
same AVX2 units Flow compiles to — and the gap should collapse. It does:

| 1024² f32 | Flow vs NumPy (1 thread) |        Flow vs NumPy (threaded) | NumPy's backend              |
| --------- | -----------------------: | ------------------------------: | ---------------------------- |
| M4 Pro    |    NumPy **13.5×** ahead |            NumPy **3.3×** ahead | Accelerate → AMX coprocessor |
| i9-14900F |    NumPy **1.21×** ahead | **dead even** (1.53 vs 1.51 ms) | OpenBLAS → AVX2              |

Same compiler, same generated code, same NumPy source — only the silicon differs. **On equal
vector hardware our generated matrix kernel is level with a hand-tuned assembly BLAS.** That is
the strongest performance claim in this project, and the one we were least expecting to make.

**"Level" means parity, not victory, and the full picture is worth printing rather than the one
cell that flatters us.** On the i9, across all four sizes:

|    N | 1 thread          | threaded              |
| ---: | ----------------- | --------------------- |
|  512 | NumPy ahead 1.11× | NumPy ahead **1.24×** |
| 1024 | NumPy ahead 1.21× | **tie**               |
| 2048 | NumPy ahead 1.20× | **Flow ahead 1.08×**  |
| 4096 | NumPy ahead 1.20× | NumPy ahead **1.06×** |

Single-threaded we are **a flat 1.20× behind** at every large size — 146 GFLOP/s against
OpenBLAS's 174, both perfectly size-invariant. Flat is the interesting part: it means this is a
steady micro-kernel deficit, not blocking or cache behaviour falling apart. Threaded, we close it
and land within ±10%, ahead at exactly one size.

Three honest qualifications. That run used our **untuned `generic`** profile — nothing about it was
specialised for Raptor Lake, which makes the 20% more impressive and also means some of it is
probably recoverable. OpenBLAS was built `DYNAMIC_ARCH` and picked an AVX2 kernel — the right
kernel for a chip with no AVX-512, but not hand-selected for it. And we verified OpenBLAS really
does use all 32 threads by default (1.5036 ms default vs 1.5055 forced), so the threaded comparison
is not us quietly using more cores.

**Why threaded closes what single-threaded does not — and it is _not_ that our scheduler is
better.** Scaling 1 thread to 32, Flow gets 9.2–9.8× where OpenBLAS gets 7.1–8.1×, which invites
exactly that conclusion. We tested it instead, and it is wrong.

Same machine, same binaries, **8 threads in every row** — only the uniformity of the 8 CPUs
changes. This chip has 8 fast P-cores and 16 slower E-cores:

| 8 threads on…       |    Flow | NumPy | winner           |
| ------------------- | ------: | ----: | ---------------- |
| 8 E-cores (uniform) | 5.89 ms |  5.59 | NumPy by 5%      |
| 8 P-cores (uniform) | 2.44 ms |  1.72 | **NumPy by 41%** |
| 4 P + 4 E (mixed)   | 3.38 ms |  5.57 | **Flow by 65%**  |

**On uniform cores OpenBLAS beats us. Flow only wins when the cores are mixed.** The mechanism is
plain in NumPy's own column: going from 8 E-cores to 4 P + 4 E — swapping four cores for four that
are 35% faster — bought OpenBLAS **nothing** (5.59 → 5.57), because it partitions work statically
and every panel waits on the slowest thread. Flow went 5.89 → 3.38 on the same change, because
work stealing lets the fast cores absorb the slack.

So what we have is **heterogeneity tolerance, not a better scheduler.** The full-machine parity
above exists because 16 of that machine's 32 threads are E-cores. On homogeneous server hardware
the honest expectation is that OpenBLAS leads both single- and multi-threaded.

That is still worth having — nearly every consumer CPU is hybrid now (Intel client since Alder
Lake, Apple silicon, ARM big.LITTLE), and on those our dispatch extracts throughput a statically
partitioned BLAS cannot. It is just not a claim about beating BLAS on a server.

The flat 20% single-threaded kernel gap, on the same units with no hardware excuse, remains the
honest target.

### Two builds, and which one the table used

Flow emits two faces of the same program:

- **conformance** (default) — bit-identical to the interpreter, always.
- **contract** (`--contract`, opt-in) — permits single-rounding fused multiply-add, the
  same licence the C++ and Rust baselines get from `-ffp-contract=fast`. Checked against
  the conformance build to a relative tolerance, not bit-for-bit.

**Every Flow number above is the contract build.** The default is 50–75% slower at 1024² and
above, and up to 2.2× slower at 512² — at 4096² it is 226 ms parallel and 2,203 ms
single-threaded, against 151 and 1,256. Clone the repo,
run the default emitter, and you will get the slower pair. That is the honest trade: the
bit-exact guarantee at the top of this page belongs to the default build, and the
benchmark table does not.

### Status

|                        |                                                                                                                                                                                                   |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Language core          | working — functions, pipelines, parallel fanout, guards, loops, `map`/`fold`/`zip`/`enumerate`, tuples, fixed arrays with element update, named types, `print`/`println`                          |
| Interpreter            | working — it _defines_ the language's meaning; every backend is tested against it                                                                                                                 |
| CPU backend (via LLVM) | working, with automatic threading and vectorisation                                                                                                                                               |
| GPU backend (CUDA)     | working — 640 compile-and-runs validated on an RTX 4090 in July 2026; **not re-validated on hardware since**, and three sessions of changes have landed. It does not implement the `time` builtin |
| FPGA backend (Verilog) | not started                                                                                                                                                                                       |
| Command-line tool      | **not built** — `flow` prints "not yet implemented" and exits 1                                                                                                                                   |
| Tests                  | ~950, all green — of which 161 are CUDA's and skip themselves on a machine without `nvcc`                                                                                                         |
| CI                     | **none.** The suite is run by hand at the end of every work session                                                                                                                               |

**What "byte-identical" actually covers today:** ten example programs plus 320 generated
ones, raw and rewritten, compiled at `-O0` and `-O2` and compared against the interpreter —
1,280 comparisons per run, CPU backend. Thread-count variation is pinned on the
parallel-shaped programs specifically, not swept across the whole corpus. The CUDA backend
gets the same treatment only when an NVIDIA machine is rented.

This is a research compiler that works, not something to `brew install`.

---

## Not just kernels

It is easy to look at the benchmarks and file this under "fast math kernels". That is the
proving ground, not the destination.

Flow is meant to be a general-purpose dataflow language. Matrix multiply, filters and
convolutions are where the claims are easiest to _check_ — everyone has a fast version to
compare against, and "byte-identical" is easy to verify. Passing those tests is how you
earn the right to be believed about anything harder.

**Where the language actually is right now:** no recursion, no sum types or pattern
matching, no strings beyond printing them, no closures, fixed-size arrays only, no modules,
and exactly two effects. It is general-purpose in shape, not yet in surface.

**The harder thing is co-execution.** Not "this program can run on a CPU _or_ a GPU" — that
is portability, and plenty of tools offer it. The goal is **one program whose parts run on
several different processors at once**, with the data movement between them typed and
checked by the compiler instead of hand-managed by you.

The heterogeneous machine — CPU plus GPU plus accelerator, keeping data in the right place
and not copying it too often — is a real problem that is badly served. Flow's claim there
is the natural extension of what already works:

> **The same output, byte for byte, no matter how the work was split across machines.**

That holds across threads today. Extending it across _devices_ is the point.

---

## Trying it

Needs a recent Rust toolchain and `clang`.

```sh
# the test suite — this is also the correctness argument
cargo test --workspace --release

# run a program on the interpreter
cargo run --release -p flow-interp --example run -- examples/pipeline.flow

# compile a program to a native binary
cargo build --release -p flow-rt          # the runtime it links against
cargo run --release -p flow-backend-llvm --example emit -- \
    examples/fir.flow - --rewrite > fir.ll
clang -O2 fir.ll target/release/libflow_rt.a -o fir && ./fir
```

Start in `examples/` — `pipeline.flow` for the syntax, `sepia.flow` for a program using
most of the language, `fir.flow` for a loop.

---

## What is next

1. ~~**Machine profiles.**~~ **Done.** The tuning constants are out of the compiler and into
   a named table of machine facts, so blocking is derived for the target chip instead of
   assumed. One consequence is worth naming: a cache-blocking rung that had to be disabled by
   hand now switches _itself_ off on a machine whose cache makes it pointless.
2. ~~**conv2d row blocking.**~~ **Done** — see above. Deduced from the recorded read
   structure, not written for convolution.
3. ~~**Scheduling deduced per region, at compile time.**~~ **Mostly done, and now on by
   default** — so unlike last time, its effect _is_ in the table above. A parallel dispatch used
   to split into exactly one piece per core, leaving the work-stealing queues empty: a fast core
   that finished could not help a slow one, so every dispatch waited for the slowest piece. The
   compiler now derives the piece count per region, and which direction to move is predicted by
   the same recorded fact that drives row blocking — a sliding read re-pays its overlap at every
   boundary, an invariant one does not. What remains is deriving the size from the _program_
   rather than from the machine alone, deducing the width, and composing plans across a wide
   dependency graph.
4. **A race in the work-stealing pool — the one thing here that is a bug, not a gap.** When a
   thread waits on a checkpoint it helps with whatever work is available, including work _past_
   the checkpoint it is waiting for. A kernel can therefore finish before the clock meant to
   bracket it starts, and about 3–4% of threaded runs self-time far too low. It does not affect
   results, only measurements — but it affects them in the direction that flatters us, which is
   the dangerous direction, and it is why every threaded figure on this page is a median rather
   than a minimum. Fix is scoped: help only with work at or below the watermark being waited on.
5. **Matrix units.** CPUs are growing dedicated matrix hardware (Arm SME, Intel AMX) and
   GPUs have had it for years. The cross-machine result above is the argument for it: on a chip
   without a matrix unit our generated kernel is level with hand-tuned OpenBLAS, so the remaining
   4× on the M4 is not something better code generation reaches — it is a different execution
   unit. Same catch, though: matrix units fix their own arithmetic ordering, so results stop
   being bit-identical. That gets an explicit opt-in, never a silent default.
6. **Then GPUs in earnest**, then co-execution.

---

## How the project works

Everything is written down. If you want to know why something is the way it is, the
reasoning is in the repository rather than lost in a chat log:

- `docs/STATUS.md` — what is built and what is not, with test counts
- `docs/decisions/` — every significant decision, with the alternatives considered
- `docs/performance/` — every benchmark, with the machine, the method, and the failures
- `docs/sessions/` — a dated log of every work session, including the mistakes

That last point is deliberate, and here is the example that earns it. A cache-blocking
optimisation was measured 3× _slower_ than the code it replaced. The first published
explanation — that it moved too much data — was wrong, and a control experiment refuted it.
The real cause was that the accumulator had stopped living in registers and was bouncing
through stack memory 92 times per inner loop. Fixing that made it 2.6–3.5× faster, and the
optimisation still lost on this machine for an unrelated reason, so it ships switched off.
All four of those steps are in the repository, including the wrong explanation, because
which claims were wrong is useful information.

---

## License

Apache License 2.0 with the LLVM exception — see [LICENSE](LICENSE).

Apache-2.0 carries an explicit patent grant, which plain MIT does not. The LLVM exception
exists for the situation a compiler creates: Flow links a small runtime into the binaries
it produces, and without the exception that would place attribution requirements on _your_
program's output. It is the same licence LLVM and Swift use, for the same reason.
