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
- [Results](#results)
  - [Matrix multiply, f32 — M4 Pro](#matrix-multiply-f32--m4-pro)
  - [Other shapes — M4 Pro](#other-shapes--m4-pro)
  - [The same shapes on an i9-14900F](#the-same-shapes-on-an-i9-14900f)
  - [Against a hand-tuned BLAS, on equal hardware](#against-a-hand-tuned-blas-on-equal-hardware)
  - [Three builds](#three-builds)
- [Status](#status)
- [Scope](#scope)
- [Trying it](#trying-it)
  - [Editor support](#editor-support)
- [What is next](#what-is-next)
- [How the project works](#how-the-project-works)
- [License](#license)

<!--toc:end-->

**A programming language that parallelizes your code for you.**

You write what you want computed. Mapal works out what can run at the same time, how to split it
across cores, how to use the vector and matrix hardware, and which bounds checks it can safely
drop. No threads, no locks, no intrinsics, no tuning pragmas — none of it appears in the source.

The same source runs on CPU and GPU, with FPGA planned.

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

Fast code is usually fast because a human wrote machine details into it: tile sizes, thread
counts, vector widths, memory layouts. Those details are what stop it moving to other hardware.

Mapal keeps them out of your source. The program says what depends on what; the machine details are
**read off the machine you are compiling on** — cache sizes, line sizes, vector widths, whether
there is a matrix unit. Change the machine, keep the source.

### What the compiler works out for you

- What can run at the same time, and how to spread it across cores
- Which loops are matrix-shaped, and how to tile them for the cache and the vector registers
- Which data is read repeatedly, so it gets laid out once instead of re-fetched
- When an access pattern will fight the cache, and how to reorder it — from the machine's own
  cache geometry, which it detects
- Which bounds checks are provably unnecessary — and only those

How it does that: [`docs/architecture/`](docs/architecture/).

---

## Results

Apple M4 Pro (10 P + 4 E), kernel time only. Baselines: naive triple loops at
`-O3 -march=native -ffp-contract=fast`, split across threads. NumPy is the expert-tuned column.

Single-threaded numbers are the fastest of N runs; threaded numbers are the median.

Where two Mapal columns appear: **FMA off** is the default build and gives bit-identical results to
the interpreter every time. **FMA on** (`--contract`) lets the hardware fuse multiply and add, which
is faster and changes the last bit. C++ and NumPy fuse by default; Rust does not.

Method, machine specs and raw logs: [`docs/performance/`](docs/performance/) ·
[`benches/results-s36/`](benches/results-s36/).

### Matrix multiply, f32 — M4 Pro

Mapal and NumPy run **alternating in the same session**, and both must print the same answer before
any time is recorded. NumPy on this machine uses Apple's Accelerate, which reaches the same matrix
hardware Mapal targets — so this is like-for-like, not a naive baseline.

**Threaded** (Mapal's `--target=apple-m4-sme` build):

|    N |    Mapal |    NumPy |                        |
| ---: | -------: | -------: | ---------------------- |
| 1024 |  0.81 ms |  0.67 ms | NumPy ahead 1.21×      |
| 2048 |  5.17 ms |  5.30 ms | tie                    |
| 4096 | **38.6 ms** | 43.9 ms | **Mapal ahead 1.14×** |

At 4096 that is **3,562 GFLOP/s against NumPy's 3,128** — the first size where Mapal is ahead, and
the distributions do not overlap: Mapal's slowest run beats NumPy's fastest.

**Single-threaded, NumPy is still about 2× ahead at every size.** That is the open gap, and it is a
different problem from the threaded one ([why](docs/performance/s43-residency-and-the-thermal-artifact.md)).

For scale: naive threaded C++ at `-O3 -march=native -ffp-contract=fast` takes **33,439 ms** on the
4096² case.

### Other shapes — M4 Pro

Threaded, every leg measured in the same pass so the columns are comparable. The bottom four rows
are a fresh 15-run pass on a machine-detecting build; FIR and conv2d are from an earlier campaign
and have a second column because `--contract` changes them (it does not touch the others).

| workload               | class           | Mapal FMA off | Mapal FMA on | C++ naive-mt | NumPy 1t |
| ---------------------- | --------------- | ------------: | -----------: | -----------: | -------: |
| FIR filter, 1M samples | compute         |      0.372 ms | **0.292 ms** |         1.50 |     6.35 |
| conv2d 3×3, 1024×1024  | compute         |  **0.114 ms** |     0.115 ms |         0.16 |     1.72 |
| saxpy, 1M              | streaming       |  **0.085 ms** |            — |         0.12 |     0.18 |
| sum reduction, 1M      | reduction       |      0.564 ms |            — |         0.92 | **0.11** |
| transpose, 1024²       | data movement   |  **0.162 ms** |            — |         0.26 |     0.86 |
| gather `x[idx[i]]`, 1M | irregular reads |      0.167 ms |            — |     **0.16** |     2.19 |

Per core, conv2d is 1.21× ahead of naive C++ on NEON and AVX2:
[conv2d-per-core-gap.md](docs/performance/conv2d-per-core-gap.md).

**Transpose used to be the slowest shape here, and its row above predates the fix.** Reading down a
column makes every access land on the same few cache slots, leaving ~97% of the cache unusable —
nothing to do with bandwidth. The compiler spots that from the graph, checks it against the cache
geometry it reads off the machine, and reorders the visits: **0.350 → 0.149 ms threaded, 2.35×.**
The same deduction on an Intel i9 measures a different cache layout, derives a different block size,
and gets 2.2× at one thread. **Nothing is configured** — no flag, and the compiler decides per shape,
so a matmul and a transpose in one program each get their own treatment
([detail](docs/performance/s44-conflict-not-capacity.md)). The reduction is a semantics difference, not a speed one: NumPy's `sum` is pairwise, a Mapal fold is left-to-right, and splitting
one needs an associativity permission the type system does not carry yet
([plan](docs/components/ir/plans/plan-s37-scan-recurrence.md)). NumPy has no threaded FIR or conv2d
kernel, so that column is not like-for-like.

### The same shapes on an i9-14900F

Median of 100, pinned to the 8 P-cores, `performance` governor, every leg in one pass:

| workload               | Mapal FMA off | Mapal FMA on | C++ naive-mt | NumPy 1t |
| ---------------------- | ------------: | -----------: | -----------: | -------: |
| FIR filter, 1M samples |      0.224 ms | **0.192 ms** |         1.93 |     5.16 |
| conv2d 3×3, 1024×1024  |  **0.104 ms** |     0.106 ms |         0.28 |     2.67 |
| saxpy, 1M              |  **0.118 ms** |     0.122 ms |         0.28 |     0.47 |
| sum reduction, 1M      |      0.394 ms |     0.393 ms |         5.09 | **0.12** |
| transpose, 1024²       |  **0.346 ms** |     0.346 ms |         0.40 |     2.33 |
| gather `x[idx[i]]`, 1M |  **0.221 ms** |     0.223 ms |         0.29 |     1.17 |

Every row except the reduction. This box takes the streaming and permutation shapes the M4 Pro
does not, so neither machine is the whole story
([why this box is easy to measure wrong](docs/sessions/2026-07-27-s37b-the-i9-cannot-be-measured-in-ms.md)).

**The transpose fix works here too, on a `--target=raptorlake` build.** This box's cache is laid
out differently from the M4's — 64-byte lines, 64 slot-groups — and a 1024-wide row collapses onto a
*single* group, worse than the M4's four. Measured **2.2× at one thread, disjoint**, and transpose
then beats naive C++ single-threaded (1.08 ms vs 2.25) where the table above has them tied. The
compiler reads this box's cache layout and derives a different block size than it derives on the M4,
from the same source. The rows above predate the fix.

### Against a hand-tuned BLAS, on equal hardware

On the M4, NumPy's matmul runs on the chip's matrix hardware. Before Mapal could use that hardware
NumPy was 3.3× ahead threaded — a different execution unit, not better code generation. Now both
sides use it, and the threaded result is above. On an i9-14900F, where NumPy goes through OpenBLAS
on the same vector units Mapal targets, 1024² f32:

| i9-14900F, 1024² f32     |    Mapal |    NumPy | gap          |
| ------------------------ | -------: | -------: | ------------ |
| single-threaded          | 14.88 ms | 12.21 ms | 1.22× behind |
| threaded, 8 P-cores      |  2.09 ms |  1.74 ms | 1.20× behind |
| threaded, whole box      |  1.52 ms |  1.50 ms | **tie**      |

The whole-machine tie is not a better kernel; it is the row below.

The threaded parity is **not** a better scheduler. Same box, 8 threads per row, only CPU
uniformity varying:

| 8 threads on…       |   Mapal | NumPy | winner           |
| ------------------- | ------: | ----: | ---------------- |
| 8 E-cores (uniform) | 5.89 ms |  5.59 | NumPy by 5%      |
| 8 P-cores (uniform) | 2.44 ms |  1.72 | **NumPy by 41%** |
| 4 P + 4 E (mixed)   | 3.38 ms |  5.57 | **Mapal by 65%** |

Swapping four E-cores for four faster ones bought OpenBLAS nothing (5.59 → 5.57) — it partitions
statically, so every panel waits on the slowest thread. Mapal went 5.89 → 3.38. That is
**heterogeneity tolerance**: it matters on consumer CPUs and claims nothing about a homogeneous
server ([detail](docs/performance/matmul/s33.md)). The 20% single-threaded kernel gap is the
remaining target.

### Three builds

| build                                        | what you get                                                    |
| -------------------------------------------- | --------------------------------------------------------------- |
| **FMA off** (default)                        | bit-identical to the interpreter, always                        |
| **FMA on** (`--contract`)                    | lets the hardware fuse multiply and add — faster, last bit moves |
| **SME** (`--contract --target=apple-m4-sme`) | same as FMA on, plus the matrix unit; needs an Apple M4 or newer |

The default emits **zero** fused instructions — verified on the compiled object, not assumed. All
three are listed because the comparison is otherwise uneven in both directions: C and C++ fuse by
default, Rust never does, and NumPy calls hand-written kernels that do.

Matrix multiply uses the SME build; the other two are not the matmul path and are not benchmarked
against it.

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
| Tests                  | 1,032; 161 are CUDA's and skip without `nvcc`. Green                                                                           |
| CI                     | `cargo fmt` + full suite on Linux and macOS, per push. Cannot pass vacuously — a skipped LLVM differential fails the run      |

**"Byte-identical" means:** every compiled program is checked to print exactly what the
interpreter prints — 10 examples plus 320 generated programs, at two optimization levels, 1,280
comparisons on every run. Changing the thread count does not change the answer.

Per-component state and test counts: [`docs/STATUS.md`](docs/STATUS.md).

Research compiler. No packaged release.

---

## Scope

Kernels are the proving ground: byte-identical output is easiest to check against code everyone
already has a fast version of.

**Not in the language yet:** recursion, sum types, pattern matching, strings beyond printing,
closures, dynamic arrays, modules. Exactly two effects. General-purpose in shape, not yet in
surface.

**Where it is going:** one program whose parts run on several processors at once — CPU and GPU
together — with the data movement between them handled by the compiler rather than by hand. Same
answer, byte for byte, however the work gets split.

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

Both editors get highlighting and a file icon. Neither is published to a registry, so both load
from disk.

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

1. **Close the single-threaded gap**, where NumPy is still ~2× ahead. Threaded is solved; this is a
   different bottleneck ([what we measured](docs/performance/s43-residency-and-the-thermal-artifact.md)).
2. **Pick the thread count from the program** instead of defaulting to every core
   ([plan](docs/components/backend-llvm/plans/plan-s32-deduced-scheduling.md)).
3. **Intel AMX**, so matrix units are not an Apple-only story.
4. **GPUs in earnest**, then running one program across several processors at once.

Done so far, and how each turned out, is in
[`docs/sessions/`](docs/sessions/) — including the things that were tried and did not work.

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
and closures are not in the language yet and have no decision record — writing one is the
contribution that unblocks the code. Forks welcome. One house rule: a change arrives with the evidence of what it
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
