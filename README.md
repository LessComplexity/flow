# Flow

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

| | Where it comes from | Portable? |
| --- | --- | --- |
| **Geometry** — which values depend on which, what can run at once, which reads repeat, which axes can be split | **deduced from the graph**, exactly, by the compiler | **yes** — it is a fact about your program |
| **Constants** — tile widths, thread counts, cache blocking | facts about the _machine_ | no — and they do not belong in your program |

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

All numbers below are from **one machine** — an Apple M4 Pro (10 performance + 4 efficiency
cores) — kernel time only, best of three. They have not been reproduced elsewhere.

### About the baselines, before the table

The C++ and Rust numbers come from **deliberately naive triple loops** — no blocking, no
intrinsics, no loop reordering; the inner loop strides down a column, which is the textbook
worst case. They are compiled `-O3 -march=native -ffp-contract=fast` and split across
threads, but they are what someone writes *before* optimising. Beating them by a large
factor demonstrates that the automatic blocking works; it is not a claim about beating
expert-tuned code.

**The expert-tuned comparison is the NumPy column, and matrix multiply is where we lose
it.**

### Matrix multiply, f32

| N | Flow | C++ (naive, threaded) | Rust (naive, threaded) | NumPy (1 thread) | NumPy (threaded) |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1024 | **4.1 ms** | 142 | 132 | 1.29 | 0.69 |
| 4096 | **173 ms** | 33,176 | 33,599 | 86.2 | 43.7 |

Against the naive baselines the margin grows with size — 11× at 512², 192× at 4096² —
because they have no cache blocking and fall apart once the data stops fitting, while
Flow's blocking is generated. **Against NumPy we are 4× behind** at 4096.

### Other shapes

| workload | Flow | C++ (naive, threaded) | NumPy (1 thread) |
| --- | ---: | ---: | ---: |
| FIR filter, 1M samples | **0.42 ms** | 1.45 | 6.21 |
| conv2d 3×3, 1024×1024 | 0.42 ms | **0.13** | 1.56 |

Two things to read carefully here:

- **conv2d is our loss, and half of what is left is not the kernel.** Our convolution used
  to compute one output row at a time, re-loading all three image rows for each — 24 vector
  loads per 36 multiply-adds, where our matrix kernel manages 4 per 32. Blocking over output
  rows is now built, and it is deduced rather than special-cased: the compiler notices from
  the recorded address coefficients that the read *slides* across rows, which is the same
  fact that makes matrix multiply's read row-invariant. That closed the single-thread gap
  from 2.09× to **1.56×** (0.53 ms → 0.40) and raised the kernel's arithmetic intensity by
  50%. The parallel number barely moved, and measurement says why: at its best thread count
  conv2d is 1.83× behind, at the default it is 2.67× behind, so **most of the remaining
  parallel deficit is scheduling, not arithmetic.** That is the next item below.
- **The NumPy comparisons are not like-for-like.** Those are Flow on 14 cores against
  NumPy on one, because the harness never runs a threaded NumPy leg for these shapes. And
  NumPy has no real conv2d kernel here — the baseline is a Python loop over nine array
  slices, so that column says more about the baseline than about NumPy.

### Why NumPy wins matrix multiply

Not a better compiler — **different hardware**. On this chip NumPy's matmul runs on a
matrix coprocessor, separate from the ordinary vector units. A single-threaded NumPy call
reaches about 11× the throughput one core's vector units can produce, and the threaded run
exceeds the whole chip's vector peak by 1.8×. No amount of tuning ordinary vector code
closes that; it needs code for that coprocessor, which is planned.

Our own kernel runs at roughly **75% of one core's vector peak**. That denominator assumes
4×128-bit fused-multiply-add pipes at about 4.4 GHz; Apple publishes neither number, so
treat it as a model rather than a datasheet.

### Two builds, and which one the table used

Flow emits two faces of the same program:

- **conformance** (default) — bit-identical to the interpreter, always.
- **contract** (`--contract`, opt-in) — permits single-rounding fused multiply-add, the
  same licence the C++ and Rust baselines get from `-ffp-contract=fast`. Checked against
  the conformance build to a relative tolerance, not bit-for-bit.

**Every Flow number above is the contract build.** The default is 25–45% slower — at 4096²
it is 249 ms parallel and 2,249 ms single-threaded, against 173 and 1,302. Clone the repo,
run the default emitter, and you will get the slower pair. That is the honest trade: the
bit-exact guarantee at the top of this page belongs to the default build, and the
benchmark table does not.

### Status

| | |
| --- | --- |
| Language core | working — functions, pipelines, parallel fanout, guards, loops, `map`/`fold`/`zip`/`enumerate`, tuples, fixed arrays with element update, named types, `print`/`println` |
| Interpreter | working — it _defines_ the language's meaning; every backend is tested against it |
| CPU backend (via LLVM) | working, with automatic threading and vectorisation |
| GPU backend (CUDA) | working — 640 compile-and-runs validated on an RTX 4090 in July 2026; **not re-validated on hardware since**, and three sessions of changes have landed. It does not implement the `time` builtin |
| FPGA backend (Verilog) | not started |
| Command-line tool | **not built** — `flow` prints "not yet implemented" and exits 1 |
| Tests | ~950, all green — of which 161 are CUDA's and skip themselves on a machine without `nvcc` |
| CI | **none.** The suite is run by hand at the end of every work session |

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
convolutions are where the claims are easiest to *check* — everyone has a fast version to
compare against, and "byte-identical" is easy to verify. Passing those tests is how you
earn the right to be believed about anything harder.

**Where the language actually is right now:** no recursion, no sum types or pattern
matching, no strings beyond printing them, no closures, fixed-size arrays only, no modules,
and exactly two effects. It is general-purpose in shape, not yet in surface.

**The harder thing is co-execution.** Not "this program can run on a CPU *or* a GPU" — that
is portability, and plenty of tools offer it. The goal is **one program whose parts run on
several different processors at once**, with the data movement between them typed and
checked by the compiler instead of hand-managed by you.

The heterogeneous machine — CPU plus GPU plus accelerator, keeping data in the right place
and not copying it too often — is a real problem that is badly served. Flow's claim there
is the natural extension of what already works:

> **The same output, byte for byte, no matter how the work was split across machines.**

That holds across threads today. Extending it across *devices* is the point.

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
   hand now switches *itself* off on a machine whose cache makes it pointless.
2. ~~**conv2d row blocking.**~~ **Done** — see above. Deduced from the recorded read
   structure, not written for convolution.
3. **Scheduling deduced per region, at compile time.** Every parallel dispatch currently
   splits into exactly one piece per core, which leaves the work-stealing queues empty — a
   fast core that finishes cannot help a slow one, so every dispatch waits for the slowest
   piece. Separating *how many pieces* from *how many cores* is done; choosing them per
   region is not. Measured with the sizes forced by hand, the right choice is worth
   **1.46–1.78×** on three different kernels, and which direction to move is predicted by
   the same recorded fact that drives row blocking — a sliding read re-pays its overlap at
   every boundary, an invariant one does not. **These numbers are not in the table above:
   nothing here is on by default until the compiler picks the sizes itself.**
4. **Matrix units.** CPUs are growing dedicated matrix hardware (Arm SME, Intel AMX) and
   GPUs have had it for years. Same idea, same catch: they fix their own arithmetic
   ordering, so results stop being bit-identical. That gets an explicit opt-in, never a
   silent default.
5. **Then GPUs in earnest**, then co-execution.

---

## How the project works

Everything is written down. If you want to know why something is the way it is, the
reasoning is in the repository rather than lost in a chat log:

- `docs/STATUS.md` — what is built and what is not, with test counts
- `docs/decisions/` — every significant decision, with the alternatives considered
- `docs/performance/` — every benchmark, with the machine, the method, and the failures
- `docs/sessions/` — a dated log of every work session, including the mistakes

That last point is deliberate, and here is the example that earns it. A cache-blocking
optimisation was measured 3× *slower* than the code it replaced. The first published
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
it produces, and without the exception that would place attribution requirements on *your*
program's output. It is the same licence LLVM and Swift use, for the same reason.
