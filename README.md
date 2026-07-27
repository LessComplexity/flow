<!-- markdownlint-disable MD033 MD041 -->
<div align = center>

<img src="assets/logo-wordmark.svg" width="320" alt="Mapal">

<br>

[![Badge Workflow]][Workflow]
[![Badge License]][License]
![Badge Language]
[![Badge Pull Requests]][Pull Requests]
[![Badge Issues]][Issues]
![Badge Determinism]<br>

<br>

</div>
<!-- markdownlint-enable MD033 -->

<!--toc:start-->

- [The idea](#the-idea)
  - [What the compiler works out for you](#what-the-compiler-works-out-for-you)
  - [Why category theory](#why-category-theory)
- [Results](#results)
  - [Matrix multiply, f32 — M4 Pro](#matrix-multiply-f32--m4-pro)
  - [Other shapes — M4 Pro](#other-shapes--m4-pro)
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

**Parallel-first programming language.** The syntax is a serialization of an execution graph:
the parser's tree is scratch space, the graph is the program. Optimization happens on that
graph — dataflow-first, not the control-flow-first IRs (GIMPLE, MIR, LLVM IR) everyone else
optimizes on -> facts a traditional compiler cannot see. Same code -> runs everywhere: CPU,
GPU, FPGA, ASIC.

The graph is read once and the facts are deduced from it: what runs concurrently, which reads
repeat, which axes split, which indices are provably in bounds -> handed to every backend.
No threads, no locks, no intrinsics, no tuning pragmas in the source.

```flow
fn main() {
    1048576 -> iota -> ts;
    ts -> map { t -> t * 2 } -> doubled;
    doubled[1048575] -> println;
}
```

That runs across every core of the machine. Nothing in the source says so.

---

## The idea

Fast code is usually fast because a human wrote machine details into the program: tile sizes,
thread counts, vector widths, memory layouts. Those details do not survive a move to different
hardware.

Mapal splits the two apart:

|                                                                                                 | Comes from                          | Portable?                                  |
| ----------------------------------------------------------------------------------------------- | ----------------------------------- | ------------------------------------------ |
| **Geometry** — what depends on what, what can run at once, which reads repeat, which axes split | **deduced from the graph**, exactly | **yes** — a fact about the program         |
| **Constants** — tile widths, thread counts, cache blocking                                      | facts about the _machine_           | no — and they do not belong in the source  |

Source carries the first kind only. The second lives in a target profile.

### What the compiler works out for you

| Deduced from the graph        | What it produces                                     | Query          |
| ----------------------------- | ---------------------------------------------------- | -------------- |
| what can run at once          | a task DAG on a work-stealing pool                   | `path_plan`    |
| which loops are matrix-shaped | a register-blocked, cache-blocked, vectorized kernel | `tile_plan`    |
| which reads repeat            | data packed once instead of re-fetched               | `TileRead`     |
| which indices are in bounds   | those checks removed, and only those                 | `bounds_proof` |

Computed once, available to every backend:
[`docs/architecture/deduced-queries.md`](docs/architecture/deduced-queries.md).

### Why category theory

The split above is checkable, not arguable. Two places where that pays:

**Two optimizations get proven to be one optimization.** Reads are modeled as morphisms, and
the compiler asks whether two of them agree. Row blocking for matrix multiply needs a
row-invariant read (`ci == 0`); convolution needs a sliding one (`ci == cq`). As predicates
over the recorded read structure those are **one predicate at `q = 0` and `q = 1`**. So conv2d
row blocking was never written for convolution -> it fell out of the model, cost ~60 lines,
and gave conv2d **−25% at one thread**. A pattern-matching optimizer needs two passes for that.

**"Good architecture" becomes a check.** The compiler's own design is modeled as data,
transformations, locations and transmissions, with coherence laws a design either satisfies or
fails -> a datum read where nothing put it is a _failed law_, not a matter of taste. Those laws
located a live measurement bug: the timed region carried an undeclared transmission, the OS
handing over physical pages.

Checkable rules, not machine-checked proof. The model is in
[`docs/architecture/categorical-model.md`](docs/architecture/categorical-model.md) (ADR-0014),
and component specs cite the rule they apply.

---

## Results

Apple M4 Pro (10 P + 4 E), kernel time only. Baselines: naive triple loops at
`-O3 -march=native -ffp-contract=fast`, split across threads. NumPy is the expert-tuned column.

Statistic: single-threaded cells are the minimum of N runs, threaded cells the **median**.

Both build faces are shown. The default is bit-identical to the interpreter and emits no fused
multiply-add; `--contract` allows single-rounding FMA, which every baseline already gets from
`-ffp-contract=fast` or from BLAS.

Method, machine specs and raw logs: [`docs/performance/`](docs/performance/) ·
[`benches/results-s36/`](benches/results-s36/).

### Matrix multiply, f32 — M4 Pro

|    N | Mapal conformance |  Mapal FMA | C++ naive-mt | Rust naive-mt | NumPy 1t | NumPy mt |
| ---: | ----------------: | ---------: | -----------: | ------------: | -------: | -------: |
| 1024 |           3.65 ms | **2.25 ms** |          125 |           117 |     1.29 |     0.69 |
| 4096 |            245 ms |  **155 ms** |       33,439 |        33,574 |     90.5 |     44.3 |

**55× the naive baseline at 1024², 216× at 4096²**, and **3.3× behind NumPy** — which on this
machine reaches the AMX coprocessor, so that column is hardware rather than a compiler comparison
(see below).

### Other shapes — M4 Pro

Threaded, median of 30:

| workload               | class           | Mapal conformance |    Mapal FMA | C++ naive-mt | NumPy 1t |
| ---------------------- | --------------- | ----------------: | -----------: | -----------: | -------: |
| FIR filter, 1M samples | compute         |          0.334 ms | **0.299 ms** |         1.45 |     6.13 |
| conv2d 3×3, 1024×1024  | compute         |          0.104 ms | **0.096 ms** |         0.16 |     1.59 |
| saxpy, 1M              | streaming       |          0.179 ms |     0.240 ms |         0.20 | **0.16** |
| sum reduction, 1M      | reduction       |          0.546 ms |     0.549 ms |         0.92 | **0.10** |
| transpose, 1024²       | data movement   |          0.288 ms |     0.269 ms |     **0.26** |     0.76 |
| gather `x[idx[i]]`, 1M | irregular reads |      **0.157 ms** |     0.160 ms |         0.18 |     1.95 |

Per core, conv2d is 1.21× ahead of naive C++ on NEON and AVX2:
[conv2d-per-core-gap.md](docs/performance/conv2d-per-core-gap.md).

The lower four rows are not compute-bound and they set the honest boundary of the claim. **The
cause is memory layout, not arithmetic intensity, and it is self-inflicted.** Two facts the backend
proves and then hides from LLVM: every object is a field of one `%Frame` struct, so alias analysis
cannot separate them and the loop does not vectorize; and `iota` is materialized as an array rather
than being an index law, so every map over one is an indirection. Emitting the first is worth
**2.3×** on saxpy single-threaded and the second **3.1×** — together **0.097 ms against clang -O3's
0.095**, parity, with no change to the IR. A plain `map` is not the problem: a plain map over a
contiguous array emits 4-wide NEON. The scalar cases are maps over a **zipped** array and over an
**iota**.

The `sum reduction` row is a semantics difference as much as a speed one: NumPy's `sum` is pairwise
and the C++ baseline splits into per-thread chunks, while a Mapal fold is left-to-right by
definition and stays on one lane. At one thread, where all three compute the same function, Mapal
is the fastest of the three (0.367 vs 0.382 vs 0.382 on the i9). Splitting a fold needs an
associativity permission the type system does not carry yet —
[plan-s37-scan-recurrence.md](docs/components/ir/plans/plan-s37-scan-recurrence.md).

NumPy has no threaded kernel for FIR or conv2d (`np.correlate` is single-threaded C; conv2d is a
Python loop over nine array slices), so that column is not like-for-like.

### Against a hand-tuned BLAS, on equal hardware

On the M4, NumPy's matmul runs on the AMX coprocessor. On an i9-14900F, where NumPy goes
through OpenBLAS on the same AVX2 units Mapal targets, the gap is hardware:

| 1024² f32 | Mapal vs NumPy 1t | Mapal vs NumPy threaded | NumPy backend    |
| --------- | ----------------- | ----------------------- | ---------------- |
| M4 Pro    | NumPy 13.5× ahead | NumPy 3.3× ahead        | Accelerate → AMX |
| i9-14900F | NumPy 1.21× ahead | **tie** (1.53 / 1.51)   | OpenBLAS → AVX2  |

Pinned to the i9's 8 P-cores — both legs on identical cores, which is where OpenBLAS is strongest
(see the row below): single-threaded **1.21× behind** (14.82 vs 12.22 ms), threaded **1.23× behind**
(2.15 vs 1.74 ms), on the untuned `generic` profile.

The threaded parity is **not** a better scheduler. Same box, 8 threads per row, only CPU
uniformity varying:

| 8 threads on…       |   Mapal | NumPy | winner           |
| ------------------- | ------: | ----: | ---------------- |
| 8 E-cores (uniform) | 5.89 ms |  5.59 | NumPy by 5%      |
| 8 P-cores (uniform) | 2.44 ms |  1.72 | **NumPy by 41%** |
| 4 P + 4 E (mixed)   | 3.38 ms |  5.57 | **Mapal by 65%** |

Swapping four E-cores for four 35%-faster ones bought OpenBLAS nothing (5.59 → 5.57): it
partitions statically, so every panel waits on the slowest thread. Mapal went 5.89 → 3.38.
That is **heterogeneity tolerance** -> it matters on consumer CPUs and claims nothing about a
homogeneous server. Detail: [s33.md §4](docs/performance/matmul/s33.md).

The flat 20% single-threaded kernel gap is the remaining target.

### Two builds

| face                        | guarantee                                       | matmul 1024², threaded |
| --------------------------- | ----------------------------------------------- | ---------------------- |
| **conformance** (default)   | bit-identical to the interpreter, always        | 3.65 ms                |
| **contract** (`--contract`) | relative tolerance; single-rounding FMA allowed | **2.25 ms** (1.62×)    |

The default emits **zero** FMA instructions — verified on the object, not assumed
(`objdump -d matmul1024_f32.o | grep -c vfmadd` = 0 by default, 28 with `--contract`). Both faces
appear in every table above for that reason: the C++ and Rust baselines are built with
`-ffp-contract=fast` and NumPy goes through BLAS, so publishing only the default would compare a
bit-exact face against fused ones.

---

## Status

|                        |                                                                                                                               |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Language core          | working — functions, pipelines, parallel fanout, guards, loops, `map`/`fold`/`zip`/`enumerate`, tuples, fixed arrays, `print` |
| Interpreter            | working — _defines_ the language; every backend is tested against it                                                          |
| CPU backend (LLVM)     | working — automatic threading, vectorization, cache blocking                                                                  |
| GPU backend (CUDA)     | working — 640 compile-and-runs on an RTX 4090, July 2026; **not re-validated on hardware since.** No `time` builtin           |
| FPGA backend (Verilog) | not started                                                                                                                   |
| Command-line tool      | **not built** — `mapal` prints "not yet implemented" and exits 1                                                              |
| Tests                  | ~950; 161 are CUDA's and skip without `nvcc`. Green                                                                           |
| CI                     | `cargo fmt` + full suite on Linux and macOS, per push. Cannot pass vacuously — a skipped LLVM differential fails the run      |

**What byte-identical covers today:** 10 examples plus 320 generated programs, raw and
rewritten, at `-O0` and `-O2` against the interpreter -> 1,280 comparisons per run, CPU
backend. Thread-count variation is pinned on parallel-shaped programs, not the whole corpus.
CUDA gets the same treatment only when an NVIDIA machine is rented.

Per-component state and test counts: [`docs/STATUS.md`](docs/STATUS.md).

Research compiler. No packaged release.

---

## Scope

Kernels are the proving ground: byte-identical output is easiest to check against code everyone
already has a fast version of.

**Not in the language yet:** recursion, sum types, pattern matching, strings beyond printing,
closures, dynamic arrays, modules. Exactly two effects. General-purpose in shape, not yet in
surface.

**Target: co-execution** -> one program whose parts run on several processors at once, with the
data movement between them typed and checked instead of hand-managed. Same output, byte for
byte, however the work was split.

---

## Trying it

Needs a recent Rust toolchain and `clang`.

```sh
# the test suite = the correctness argument
cargo test --workspace --release

# run a program on the interpreter
cargo run --release -p mapal-interp --example run -- examples/pipeline.mapal

# compile a program to a native binary
cargo build --release -p mapal-rt          # the runtime it links against
cargo run --release -p mapal-backend-llvm --example emit -- \
    examples/fir.mapal - --rewrite > fir.ll
clang -O2 fir.ll target/release/libmapal_rt.a -o fir && ./fir
```

Start in [`examples/`](examples/) — `pipeline.mapal` for syntax, `sepia.mapal` for most of the
language, `fir.mapal` for a loop.

### Editor support

Both editors get highlighting and a file icon. Neither is published to a registry -> load from
disk.

|                 | Neovim                                     | VS Code                                      |
| --------------- | ------------------------------------------ | -------------------------------------------- |
| Highlighting    | Vimscript syntax file                      | TextMate grammar                             |
| File icon       | font glyph                                 | the real SVG logo                            |
| Binding vs call | resolved by scanning for `fn` declarations | lexical only — `-> name;` reads as a binding |
| Details         | [`editors/nvim/`](editors/nvim/)           | [`editors/vscode/`](editors/vscode/)         |

**Neovim** — with lazy.nvim:

```lua
{ dir = "/path/to/mapal/editors/nvim", ft = "mapal" }
```

or without a plugin manager:

```lua
vim.opt.runtimepath:append("/path/to/mapal/editors/nvim")
require("mapal.icon").setup()          -- optional: nvim-web-devicons / mini.icons
```

**VS Code / Cursor** — build the extension and install it:

```sh
python3 editors/vscode/package-vsix.py
code --install-extension editors/vscode/mapal-lang-0.1.0.vsix   # or `cursor`
```

Then restart. A `.mapal` file should show **Mapal** in the status bar. Copying the folder into
`~/.vscode/extensions` does _not_ work: VS Code reads `extensions.json` as its registry and
ignores unregistered directories silently.

**The logo as a terminal glyph.** The Rust and C++ marks in a file tree are font glyphs, not
images — Nerd Fonts ships those brand logos as characters. So Mapal's mark ships as a
single-glyph font, [`assets/font/MapalIcons.ttf`](assets/font/): install it and add it as a
terminal _fallback_ font, rather than us patching and redistributing someone else's Nerd Font.
Then:

```lua
local icon = require("mapal.icon")
icon.setup({ glyph = icon.logo })     -- the real mark at U+F8F0
```

Without it, the closest glyph the Nerd Font already has is used. Neither editor has an LSP yet,
so neither resolves names the way the compiler does (ADR-0008).

---

## What is next

1. ~~**Machine profiles**~~ — done. Tuning constants moved out of the compiler into a named
   table of machine facts.
2. ~~**conv2d row blocking**~~ — done, deduced from the recorded read structure.
3. ~~**Per-region scheduling**~~ — mostly done and on by default. Remaining: derive the size
   from the _program_, deduce the width, compose plans across a wide DAG
   ([plan-s32](docs/components/backend-llvm/plans/plan-s32-deduced-scheduling.md)).
4. ~~**The rewriter could delete a trap that must fire**~~ — fixed
   ([plan-s34](docs/components/rewrite/plans/plan-s34-identity-map-trap.md)).
5. ~~**A pool race made every threaded measurement suspect**~~ — fixed. A clock read is now a node
   in the task DAG, so work written after it cannot be dispatched before it fires
   ([plan-s33b](docs/components/backend-llvm/plans/plan-s33b-clock-read-barrier.md)).
6. **Matrix units** (Arm SME, Intel AMX). The cross-machine result is the argument: without a
   matrix unit we match OpenBLAS, so the M4's remaining gap is a different execution unit, not
   better codegen. They change arithmetic ordering, so this gets an explicit opt-in, never a
   silent default.
7. **Then GPUs in earnest**, then co-execution.

---

## How the project works

Everything is written down, including what turned out wrong:

- [`docs/STATUS.md`](docs/STATUS.md) — what is built, with test counts
- [`docs/decisions/`](docs/decisions/README.md) — every significant decision and the
  alternatives it rejected, indexed (ADR = Architecture Decision Record)
- [`docs/performance/`](docs/performance/) — every benchmark: machine, method, failures
- [`docs/sessions/`](docs/sessions/) — dated log of every work session, mistakes included

**Contributing:** [`CONTRIBUTING.md`](CONTRIBUTING.md) — the model-first workflow, the
measurement rules, and the [open ADRs](docs/decisions/README.md) anyone can pick up (dynamic
arrays, generics, sum types, an external-backend SDK, co-execution, `scan`). Recursion, modules
and closures are not in the language yet and have no ADR -> writing one is the contribution that
unblocks the code. Forks welcome. One house rule: a change arrives with the evidence of what it
did, and names the published numbers it moves.

---

## License

[Apache License 2.0 with the LLVM exception](LICENSE) — the license LLVM and Swift use, for the
same reason. Apache-2.0 carries an explicit patent grant; the exception keeps the runtime Mapal
links into your binaries from imposing attribution requirements on _your_ program's output.

<!----------------------------------{ Badges }---------------------------------->

<!-- The license badge is static because GitHub classifies Apache-2.0 WITH
     LLVM-exception as NOASSERTION, so shields' dynamic one renders the words
     "not identifiable by github". The exception is real; the badge says so. -->

[Badge Workflow]: https://github.com/LessComplexity/mapal/actions/workflows/ci.yml/badge.svg
[Badge License]: https://img.shields.io/badge/license-Apache--2.0_WITH_LLVM--exception-blue
[Badge Language]: https://img.shields.io/github/languages/top/LessComplexity/mapal
[Badge Pull Requests]: https://img.shields.io/github/issues-pr/LessComplexity/mapal
[Badge Issues]: https://img.shields.io/github/issues/LessComplexity/mapal
[Badge Determinism]: https://img.shields.io/badge/output-byte--identical-14b8a6

<!-----------------------------------{ Links }----------------------------------->

[Workflow]: https://github.com/LessComplexity/mapal/actions/workflows/ci.yml
[License]: LICENSE
[Pull Requests]: https://github.com/LessComplexity/mapal/pulls
[Issues]: https://github.com/LessComplexity/mapal/issues
