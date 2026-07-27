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
  - [The same shapes on an i9-14900F](#the-same-shapes-on-an-i9-14900f)
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
multiply-add; `--contract` allows single-rounding FMA. The C++ baselines fuse (`-ffp-contract=fast`,
and it is the C/C++ default anyway); the Rust baselines do **not** — rustc never contracts without
an explicit `mul_add`, and their objects contain zero FMA instructions; NumPy's speed comes from
BLAS kernels hand-written with it.

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

Threaded, median of 100, every leg measured in the same pass so the columns are comparable:

| workload               | class           | Mapal conformance |    Mapal FMA | C++ naive-mt | NumPy 1t |
| ---------------------- | --------------- | ----------------: | -----------: | -----------: | -------: |
| FIR filter, 1M samples | compute         |          0.372 ms | **0.292 ms** |         1.50 |     6.35 |
| conv2d 3×3, 1024×1024  | compute         |      **0.114 ms** |     0.115 ms |         0.16 |     1.72 |
| saxpy, 1M              | streaming       |      **0.115 ms** |     0.116 ms |         0.23 |     0.18 |
| sum reduction, 1M      | reduction       |          0.582 ms |     0.585 ms |         0.94 | **0.11** |
| transpose, 1024²       | data movement   |          0.290 ms |     0.308 ms |     **0.26** |     0.83 |
| gather `x[idx[i]]`, 1M | irregular reads |          0.194 ms |     0.179 ms |     **0.17** |     2.20 |

Per core, conv2d is 1.21× ahead of naive C++ on NEON and AVX2:
[conv2d-per-core-gap.md](docs/performance/conv2d-per-core-gap.md).

Two rows still go to C++ — transpose and gather — and they set the honest boundary of the claim.
(The reduction is a semantics difference, not a speed one; see below.) **The cause is memory layout,
not arithmetic intensity, and it is self-inflicted** — but not in the way this section previously
claimed, and the correction is worth stating because the wrong diagnosis was here for a while.

The compiler now records what `out[i]` **is**, as a graph fact: `iota`'s element is the index,
`zip`'s is a pair of its two sources' elements, `enumerate`'s needs no rule of its own because it
is `zip` over an `iota`. Consumers build the element instead of reading it back out of memory.
The payoff turned out not to be the arithmetic. Once every consumer rebuilds the element, **the
array is write-only** — saxpy was materializing 8 MB of zipped pairs per run that nothing read, and
doing it inside the timed region. Deleting it is worth **0.4769 → 0.0981 ms single-threaded** and
**0.1860 → 0.0833 ms threaded** (min of alternating runs against the same binary built before the
change; the table above is medians, so its saxpy row moves by a similar factor).

What was previously claimed here — that one shared `%Frame` struct defeats alias analysis and is
worth 2.3× — **is false for the code this compiler emits.** LLVM computes non-overlap from constant
field offsets without help; a struct-field control vectorizes with no metadata at all, and across
61 tasks in seven shapes exactly one reports `unsafe dependent memory operations` — the dead `zip`
task above. saxpy's timed loop already vectorized. The 2.3× came from a synthetic probe reproducing
a pattern the compiler does not actually emit. The predicted destination was nearly right (0.097 ms
predicted, 0.098 measured) and the predicted mechanism was wrong, which is a good argument for
measuring the artifact rather than the model of it.

A plain `map` is not the problem: a plain map over a contiguous array emits 4-wide NEON. The scalar
cases are maps over a **zipped** array and over an **iota**, and both layouts were ours.

The `sum reduction` row is a semantics difference as much as a speed one: NumPy's `sum` is pairwise
and the C++ baseline splits into per-thread chunks, while a Mapal fold is left-to-right by
definition and stays on one lane. At one thread, where all three compute the same function, Mapal
is the fastest of the three (0.367 vs 0.382 vs 0.382 on the i9). Splitting a fold needs an
associativity permission the type system does not carry yet —
[plan-s37-scan-recurrence.md](docs/components/ir/plans/plan-s37-scan-recurrence.md).

NumPy has no threaded kernel for FIR or conv2d (`np.correlate` is single-threaded C; conv2d is a
Python loop over nine array slices), so that column is not like-for-like.

### The same shapes on an i9-14900F

A second machine, and every leg — Mapal both faces, C++, NumPy — measured in one pass on the same
cores. Median of 100, pinned to the 8 P-cores (`taskset -c 0-15`), `performance` governor:

| workload               |    Mapal conformance |    Mapal FMA | C++ naive-mt | NumPy 1t |
| ---------------------- | -------------------: | -----------: | -----------: | -------: |
| FIR filter, 1M samples |             0.224 ms | **0.192 ms** |         1.93 |     5.16 |
| conv2d 3×3, 1024×1024  |         **0.104 ms** |     0.106 ms |         0.28 |     2.67 |
| saxpy, 1M              |         **0.118 ms** |     0.122 ms |         0.28 |     0.47 |
| sum reduction, 1M      |             0.394 ms |     0.393 ms |         5.09 | **0.12** |
| transpose, 1024²       |         **0.346 ms** |     0.346 ms |         0.40 |     2.33 |
| gather `x[idx[i]]`, 1M |         **0.221 ms** |     0.223 ms |         0.29 |     1.17 |

Mapal takes every row except the reduction, which is the semantics difference described above —
NumPy's `sum` is pairwise, Mapal's fold is a left fold, and they are not computing the same thing.
Note this box wins the streaming and permutation shapes that the M4 Pro does not; the two machines
disagree about those rows, which is itself worth knowing before quoting either in isolation.

**Methodology, because this box is easy to measure wrong.** It defaults to the `powersave` governor,
which parks it near 1.1 GHz — the same binary read 0.70 ms and 1.12 ms in two sessions an hour
apart, a 60% swing that is entirely the frequency ramp. The table above was taken at `performance`
(5.5 GHz under load), and the ratio between the two settings, 5.0×, matches the clock ratio almost
exactly. Reaching for `perf stat` instead does not help unless it is differenced around the timed
region: the self-timed kernel is **3.1%** of what a whole-process count sees, the rest being data
generation and startup. Two independent 30-run passes agreed within ~5% on every cell above 0.2 ms;
conv2d and saxpy sit near 0.1 ms and swung up to 40% at that sample size, which is why the published
table is 100.

### Against a hand-tuned BLAS, on equal hardware

On the M4, NumPy's matmul runs on the AMX coprocessor. On an i9-14900F, where NumPy goes
through OpenBLAS on the same AVX2 units Mapal targets, the gap is hardware:

| 1024² f32 | Mapal vs NumPy 1t | Mapal vs NumPy threaded | NumPy backend    |
| --------- | ----------------- | ----------------------- | ---------------- |
| M4 Pro    | NumPy 13.5× ahead | NumPy 3.3× ahead        | Accelerate → AMX |
| i9-14900F | NumPy 1.22× ahead | see both rows below     | OpenBLAS → AVX2  |

The threaded answer depends on which cores you give it, and both configurations are worth stating
because they say different things. Median of 15, `performance` governor, product face:

| i9-14900F, 1024² f32     |     Mapal |     NumPy | gap             |
| ------------------------ | --------: | --------: | --------------- |
| single-threaded          | 14.88 ms  | 12.21 ms  | 1.22× behind    |
| threaded, 8 P-cores only |  2.09 ms  |  1.74 ms  | 1.20× behind    |
| threaded, whole machine  |  1.52 ms  |  1.50 ms  | **tie** (1.01×) |

Pinning to the P-cores is where OpenBLAS is strongest — identical cores, no heterogeneity to
exploit — and Mapal is 1.20× behind there on the untuned `generic` profile. The whole-machine tie
is not a better kernel; it is the row below.

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
appear in every table above because the comparison is otherwise uneven in both directions: C/C++
contracts by default, Rust never does, and NumPy calls kernels written with FMA by hand.

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
