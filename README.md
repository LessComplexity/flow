<p align="center">
  <img src="assets/logo-wordmark.svg" alt="Flow" width="320">
</p>

# Flow

<!--toc:start-->

- [Flow](#flow)
  - [The idea](#the-idea)
    - [What the compiler works out for you](#what-the-compiler-works-out-for-you)
    - [Why category theory](#why-category-theory)
  - [Results](#results)
    - [Matrix multiply, f32 — M4 Pro](#matrix-multiply-f32-m4-pro)
    - [Other shapes — M4 Pro](#other-shapes-m4-pro)
    - [Against a hand-tuned BLAS, on equal hardware](#against-a-hand-tuned-blas-on-equal-hardware)
    - [Two builds](#two-builds)
  - [Status](#status)
  - [Scope](#scope)
  - [Trying it](#trying-it)
    - [Editor support](#editor-support)
  - [What is next](#what-is-next)
  - [How the project works](#how-the-project-works)
  - [License](#license)
  <!--toc:end-->

**A parallel-first language: you describe _what_ to compute, the compiler works out _how_.**

A Flow program is a dataflow graph. The compiler reads that graph and deduces the
optimisation facts once — what runs concurrently, which reads repeat, which axes can be
split, which indices are provably in bounds — then hands them to any backend. You never
write a thread, a lock, a SIMD intrinsic, or a tuning pragma.

> **The same program gives byte-identical output at `-O0` or `-O2`, on one thread or all of
> them.** Not "close enough" — the same bits, checked against an interpreter that defines
> what the language means.

Read that as engineering discipline, not a theorem: nothing here is machine-proven.

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

Most fast code is fast because a human wrote the machine's details into the program — tile
sizes, thread counts, vector widths, memory layouts. That knowledge does not survive a move
to different hardware.

Flow splits the two halves apart:

|                                                                                                 | Comes from                          | Portable?                                  |
| ----------------------------------------------------------------------------------------------- | ----------------------------------- | ------------------------------------------ |
| **Geometry** — what depends on what, what can run at once, which reads repeat, which axes split | **deduced from the graph**, exactly | **yes** — a fact about your program        |
| **Constants** — tile widths, thread counts, cache blocking                                      | facts about the _machine_           | no — and they do not belong in your source |

Your source only ever contains the first kind. The second lives in a target profile, not in
your program.

### What the compiler works out for you

| Deduced from the graph        | What it produces                                     | Query          |
| ----------------------------- | ---------------------------------------------------- | -------------- |
| what can run at once          | a task DAG on a work-stealing pool                   | `path_plan`    |
| which loops are matrix-shaped | a register-blocked, cache-blocked, vectorised kernel | `tile_plan`    |
| which reads repeat            | data packed once instead of re-fetched               | `TileRead`     |
| which indices are in bounds   | those checks removed, and only those                 | `bounds_proof` |

Each is computed once and available to every backend —
[`docs/architecture/deduced-queries.md`](docs/architecture/deduced-queries.md).

### Why category theory

Not decoration. It is what makes the split above checkable rather than arguable, and it is
load-bearing in two specific ways.

**Optimisations get proven to be the same optimisation.** The compiler models a program's
reads as morphisms and asks whether two of them agree. Row blocking for matrix multiply needs
a read that is row-invariant (`ci == 0`); for convolution, one that slides (`ci == cq`).
Written as predicates over the recorded read structure, those are **one predicate at `q = 0`
and `q = 1`**. conv2d row blocking was therefore never written for convolution — it fell out
of the model, cost ~60 lines, and gave conv2d **−25% at one thread**. A pattern-matching
optimiser needs two passes for that and tends to ship only the first.

**"Good architecture" becomes a check.** The compiler's own design is modelled as data,
transformations, locations and transmissions, with coherence laws a design either satisfies
or fails — so a datum read where nothing put it is a _failed law_, not a matter of taste.
That is how the conv2d measurement bug was finally found: the timed region contained a
transmission (the OS handing over physical pages) that the model said had to be declared
somewhere, and it was not.

The honest limit: this is design discipline with checkable rules, not machine-checked proof.
The model is in
[`docs/architecture/categorical-model.md`](docs/architecture/categorical-model.md)
(ADR-0014), and component specs cite the rule they apply.

---

## Results

Apple M4 Pro (10 P + 4 E), kernel time only. Baselines are **deliberately naive triple
loops** at `-O3 -march=native -ffp-contract=fast`, split across threads — what you write
before optimising, not expert-tuned code. NumPy is the expert-tuned column.

**Statistic:** single-threaded cells are the minimum of N runs; threaded cells are the
**median**, because a known pool race makes ~3–4% of threaded runs self-time far too low.
It is a measurement bug, not a codegen one —
[s33.md §5](docs/performance/matmul/s33.md#5--p0--a-pool-race-makes-every-par-minimum-invalid).

Full method, machine specs and raw logs: [`docs/performance/`](docs/performance/) ·
[`benches/results-s33/`](benches/results-s33/).

### Matrix multiply, f32 — M4 Pro

|    N |        Flow | C++ naive-mt | Rust naive-mt | NumPy 1t | NumPy mt |
| ---: | ----------: | -----------: | ------------: | -------: | -------: |
| 1024 | **2.23 ms** |          140 |           133 |     1.30 |     0.68 |
| 4096 |  **151 ms** |       33,065 |        33,548 |     84.6 |     44.1 |

63× the naive baseline at 1024², 219× at 4096². **3.4× behind NumPy** at 4096 — see below,
that number is about the hardware.

### Other shapes — M4 Pro

| workload               |         Flow | C++ naive-mt | NumPy 1t |
| ---------------------- | -----------: | -----------: | -------: |
| FIR filter, 1M samples |  **0.27 ms** |         1.42 |     6.10 |
| conv2d 3×3, 1024×1024  | **0.089 ms** |         0.14 |     1.55 |

conv2d read as a 3.2× **loss** for five sessions. Eight in-kernel hypotheses were refuted by
measurement before the cause turned out to be the benchmark: our timed region included the
output array's first-touch page-zeroing, which `std::vector`'s zero-fill pre-pays outside the
C++ timer. Per core it is now 1.21× **ahead** on both NEON and AVX2 —
[conv2d-per-core-gap.md](docs/performance/conv2d-per-core-gap.md).

NumPy has no threaded kernel for either shape (`np.correlate` is single-threaded C; conv2d is
a Python loop over nine array slices), so that column is not like-for-like.

### Against a hand-tuned BLAS, on equal hardware

On the M4, NumPy's matmul runs on the AMX coprocessor. Rerun on an i9-14900F, where NumPy
goes through OpenBLAS on the same AVX2 units Flow targets, and the gap is hardware:

| 1024² f32 | Flow vs NumPy 1t  | Flow vs NumPy threaded | NumPy backend    |
| --------- | ----------------- | ---------------------- | ---------------- |
| M4 Pro    | NumPy 13.5× ahead | NumPy 3.3× ahead       | Accelerate → AMX |
| i9-14900F | NumPy 1.21× ahead | **tie** (1.53 / 1.51)  | OpenBLAS → AVX2  |

Across all four sizes on the i9: single-threaded a **flat 1.20× behind** (146 vs 174
GFLOP/s, both size-invariant — a steady micro-kernel deficit, not a blocking failure);
threaded within **±10%**, ahead only at 2048². On the **untuned `generic`** profile.

The threaded parity is **not** a better scheduler. Same box, 8 threads per row, only CPU
uniformity varying:

| 8 threads on…       |    Flow | NumPy | winner           |
| ------------------- | ------: | ----: | ---------------- |
| 8 E-cores (uniform) | 5.89 ms |  5.59 | NumPy by 5%      |
| 8 P-cores (uniform) | 2.44 ms |  1.72 | **NumPy by 41%** |
| 4 P + 4 E (mixed)   | 3.38 ms |  5.57 | **Flow by 65%**  |

Swapping four E-cores for four 35%-faster ones bought OpenBLAS nothing (5.59 → 5.57): it
partitions statically, so every panel waits on the slowest thread. Flow went 5.89 → 3.38.
So it is **heterogeneity tolerance**, which matters on consumer CPUs and does not claim
anything about a homogeneous server. Detail: [s33.md §4](docs/performance/matmul/s33.md).

The flat 20% single-threaded kernel gap is the honest remaining target.

### Two builds

| face                        | guarantee                                       | speed                                       |
| --------------------------- | ----------------------------------------------- | ------------------------------------------- |
| **conformance** (default)   | bit-identical to the interpreter, always        | 50–75% slower at ≥1024², up to 2.2× at 512² |
| **contract** (`--contract`) | relative tolerance; single-rounding FMA allowed | **every Flow number above**                 |

At 4096² the default is 226 ms parallel / 2,203 ms single-threaded, against 151 / 1,256.
Clone and run the default emitter and you get the slower pair.

---

## Status

|                        |                                                                                                                               |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Language core          | working — functions, pipelines, parallel fanout, guards, loops, `map`/`fold`/`zip`/`enumerate`, tuples, fixed arrays, `print` |
| Interpreter            | working — _defines_ the language; every backend is tested against it                                                          |
| CPU backend (LLVM)     | working — automatic threading, vectorisation, cache blocking                                                                  |
| GPU backend (CUDA)     | working — 640 compile-and-runs on an RTX 4090, July 2026; **not re-validated on hardware since.** No `time` builtin           |
| FPGA backend (Verilog) | not started                                                                                                                   |
| Command-line tool      | **not built** — `flow` prints "not yet implemented" and exits 1                                                               |
| Tests                  | ~950 green; 161 are CUDA's and skip without `nvcc`                                                                            |
| CI                     | `cargo fmt` + full suite on Linux and macOS, per push                                                                         |

**What byte-identical covers today:** 10 examples plus 320 generated programs, raw and
rewritten, at `-O0` and `-O2` against the interpreter — 1,280 comparisons per run, CPU
backend. Thread-count variation is pinned on parallel-shaped programs, not the whole corpus.
CUDA gets the same treatment only when an NVIDIA machine is rented.

Per-component state and test counts: [`docs/STATUS.md`](docs/STATUS.md).

This is a research compiler that works, not something to `brew install`.

---

## Scope

Kernels are the proving ground, not the destination — they are where "byte-identical" is
easiest to _check_ against something everyone already has a fast version of.

**Not in the language yet:** recursion, sum types, pattern matching, strings beyond
printing, closures, dynamic arrays, modules. Exactly two effects. General-purpose in shape,
not yet in surface.

**The target is co-execution** — not "runs on CPU _or_ GPU", but one program whose parts run
on several processors at once, with the data movement between them typed and checked rather
than hand-managed. The claim extends the one that already holds across threads: same output,
byte for byte, however the work was split.

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

Start in [`examples/`](examples/) — `pipeline.flow` for syntax, `sepia.flow` for most of the
language, `fir.flow` for a loop.

### Editor support

Both editors get highlighting and a file icon. Neither is published to a registry — load from
disk.

|                 | Neovim                                     | VS Code                                      |
| --------------- | ------------------------------------------ | -------------------------------------------- |
| Highlighting    | Vimscript syntax file                      | TextMate grammar                             |
| File icon       | font glyph                                 | the real SVG logo                            |
| Binding vs call | resolved by scanning for `fn` declarations | lexical only — `-> name;` reads as a binding |
| Details         | [`editors/nvim/`](editors/nvim/)           | [`editors/vscode/`](editors/vscode/)         |

**Neovim** — with lazy.nvim:

```lua
{ dir = "/path/to/flow/editors/nvim", ft = "flow" }
```

or without a plugin manager:

```lua
vim.opt.runtimepath:append("/path/to/flow/editors/nvim")
require("flow.icon").setup()          -- optional: nvim-web-devicons / mini.icons
```

**VS Code / Cursor** — symlink the extension and restart:

```sh
ln -s /path/to/flow/editors/vscode ~/.vscode/extensions/flow-lang
```

**The logo as a terminal glyph.** The Rust and C++ marks in a file tree are font glyphs, not
images — Nerd Fonts ships those brand logos as characters. So Flow's mark ships as a
single-glyph font, [`assets/font/FlowIcons.ttf`](assets/font/), which you install and add as a
terminal _fallback_ font (rather than us patching and redistributing someone else's Nerd Font).
Then:

```lua
local icon = require("flow.icon")
icon.setup({ glyph = icon.logo })     -- the real mark at U+F8F0
```

Skip it and you get the closest glyph your Nerd Font already has. Neither editor has an LSP
yet, so neither resolves names the way the compiler does (ADR-0008).

---

## What is next

1. ~~**Machine profiles**~~ — done. Tuning constants moved out of the compiler into a named
   table of machine facts.
2. ~~**conv2d row blocking**~~ — done, deduced from the recorded read structure.
3. ~~**Per-region scheduling**~~ — mostly done and on by default. Remaining: derive the size
   from the _program_, deduce the width, compose plans across a wide DAG
   ([plan-s32](docs/components/backend-llvm/plans/plan-s32-deduced-scheduling.md)).
4. **A pool race** — the one item here that is a bug, not a gap. A waiting thread helps with
   work past the checkpoint it waits on, so a kernel can finish before the clock bracketing
   it starts. Affects measurements only, in the direction that flatters us, which is why
   threaded figures are medians
   ([plan-s33b](docs/components/backend-llvm/plans/plan-s33b-clock-read-barrier.md)).
5. **Matrix units** (Arm SME, Intel AMX). The cross-machine result is the argument: without
   a matrix unit we match OpenBLAS, so the M4's remaining gap is a different execution unit,
   not better codegen. They fix their own arithmetic ordering, so this gets an explicit
   opt-in, never a silent default.
6. **Then GPUs in earnest**, then co-execution.

---

## How the project works

Everything is written down, including what turned out wrong:

- [`docs/STATUS.md`](docs/STATUS.md) — what is built, with test counts
- [`docs/decisions/`](docs/decisions/) — every significant decision and the alternatives
- [`docs/performance/`](docs/performance/) — every benchmark: machine, method, failures
- [`docs/sessions/`](docs/sessions/) — dated log of every work session, mistakes included

Worked example of why that last one is kept: a cache-blocking pass measured 3× _slower_ than
what it replaced; the first published explanation was wrong and a control refuted it; the
real cause was an accumulator bouncing through stack memory 92 times per inner loop. Fixing
that made it 2.6–3.5× faster, and it still loses on this machine for an unrelated reason, so
it ships off. All four steps are in the repo, wrong explanation included.

---

## License

[Apache License 2.0 with the LLVM exception](LICENSE) — the same licence LLVM and Swift use,
for the same reason. Apache-2.0 carries an explicit patent grant; the exception keeps the
runtime Flow links into your binaries from imposing attribution requirements on _your_
program's output.
