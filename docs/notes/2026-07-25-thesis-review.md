# Thesis review — 2026-07-25

**Status: informational note, non-binding.** Origin: an external-lens review of the
language, premise and direction (Sapir's ask), followed by Sapir's challenge to the
review's central criticism — which was **correct, and the criticism is withdrawn
here on the evidence**. Recorded because the correction is load-bearing: it changes
how the tile work should be described, internally and externally.

Nothing here changes spec, scope or roadmap. Decisions move by ADR — the four this
review produced are ADR-0033…0036.

---

## 1. The retracted criticism, and why it was wrong

**The review claimed:** sessions S24–S29 are dense-GEMM chasing — reimplementing
BLIS inside an LLVM text emitter — which is the "portable performance graveyard"
VISION §5.1 warns about and the dense-GEMM ground VISION §7 says never to fight on.
Evidence offered: 10–14× behind OpenBLAS/Accelerate at 1024, six sessions spent,
CLI/M4/M5 untouched meanwhile.

**Sapir's counter:** the work is not GEMM tuning. It is geometric analysis on the
graph, which *lowers to* BLAS-class code on CPU (cache blocking, FMA) and will lower
to smem/mma on CUDA. The capability transfers to every backend; the CPU number is a
measuring stick, not the product.

**The counter is right, and the repo already proves it.**

- `docs/notes/tile-ladder-direction.md`: *"Matmul/FIR/attention matched ONE rule; no
  matrix concept exists in the recognizer."* The recognizer is one affine-address
  question — how does an address move when the neighbouring lane moves? 0 =
  broadcast, 1 = unit stride, else refuse. Nothing in it knows what a matrix is.
- **S28 measured the transfer.** The ladder built on matmul carried to a FIR 1-D
  window rung (`window1d_site` → `emit_tiled_map_blocked_1d`, **zero flow-ir
  change**) and a conv2d unrolled micro-kernel, in one session. Both won their
  tables — fir fma-1t 0.216 vs cpp-1t 0.924 (4.3×), and on the box 0.287@16T vs
  numpy-1t 1.39.
- `graph-advantage.md`'s ledger is shipped-and-measured, not theory: parallel
  orchestrator from no-path independence, guard elision from index intervals, copy
  elision from last-use, tiling from address regularity.

**The review measured the wrong quantity** — the GEMM gap, rather than the transfer
rate across shapes. GEMM is the hardest known shape and therefore the right stress
test for a geometry engine; BLAS is the oracle for whether the geometry is right.
That is a different activity from chasing parity with a library, and the distinction
should be made explicitly in VISION, because a reader will make the same mistake the
review did.

**Standing correction to internal language:** do not describe the tile work as
"matmul performance." It is *geometry recognition, validated on matmul*.

---

## 2. What the review got right and still stands

### 2.1 The geometry / constants split

Two things have been treated as one:

| | Source | Portable? |
| --- | --- | --- |
| **Geometry** — which reads broadcast vs stride, which axes split, nest order, what is legally interleavable | **deduced** from the graph (`tile_plan`, `TileRead.ksplit`, `path_plan`) | **yes — proven** |
| **Constants** — `TILE_J=16`, `TI=4`, `TILE_KC=128`, `NC=TJ×32`, unroll ×2, prefetch, `GRAIN` | **hand-set literals** in `backends/llvm/src/func.rs`, from manual sweeps (S26: *"TI sweep 2/4/8 → 4 (8 spills)"*) | **no** — facts about a cache hierarchy, not about the program |

Both the local M4 Pro (NEON, 14T) and the EPYC box (zen3, AVX2, 61T) run the same
literals. BLIS's margin over a competent generic tiler is largely per-uarch
parameters and per-ISA microkernels — so the residual gap is **not** evidence the
geometry is wrong; it is substantially evidence the constants are untuned for the
target.

This *completes* the genericity thesis rather than qualifying it: the search
procedure is as backend-generic as the geometry (one tuner over the recorded facts,
per-`Loc` constant table). ADR-0032 D4 already draws the line; **ADR-0034** proposes
how the tables get their values — searched, not set.

Phrasing that survives contact with a skeptic: *the geometry is deduced and
portable; the constants are measured per target.*

### 2.2 The second-consumer gap

Verified 2026-07-25:

```
tile_plan consumers:  backends/llvm/src/lib.rs, backends/llvm/src/func.rs
backends/cuda/src/:   no hits
```

`tile_plan` landed S25; the last CUDA session was S23. Every rung since has had one
consumer. "This transfers to all backends" is therefore currently *architectural
assertion*, not measurement — which matters twice: the thesis is unfalsified rather
than proven, and nothing tests ADR-0032's genericity contract, so a cache-hierarchy
assumption could migrate into a "generic" query with no gate firing.

**Sapir's sequencing (2026-07-25): CPU to full advantage first, then GPU.** Accepted
— **ADR-0033** is written to that order: the CUDA leg becomes the *named exit
condition of the CPU phase* rather than a gate on the next rung, with a cheap interim
guard (each rung's plan doc names its CUDA realization from the record, three lines,
minutes not sessions). The guard is what makes the deferral affordable; without it
the exposure grows per rung and is discovered at port time.

Open and important: **what is the written bar for "full advantage on CPU"?** Without
one, the trigger recedes indefinitely. ADR-0033 Q1.

### 2.3 Unchanged and uncontested

`cli` not-started (no `flow` binary exists — nobody outside this repo can run
anything), `backend-verilog` is a 1-line `lib.rs`, M4/M5 not reached, no `LICENSE`
file, no git remote. VISION §6 calls the Apache-2.0 choice strategic and names M5 as
the launch artifact; neither is executed. Orthogonal to the performance argument.

---

## 3. Language findings

### 3.1 The good face is real

`benches/matmul/matmul128_cap_f32.flow` — `iota → map { … fold … }` — is intent, not
mechanism, and the compiler owns the schedule. That is the pitch, and it lands.

### 3.2 Guards: coproducts do **not** break Core (review's flag was wrong)

The review flagged "coproducts will change what a guard is." **Incorrect** — ADR-0026
already solves this: sum guards desugar onto the *existing* strict Phi, with
`Payload { variant }` totalized (non-matching tag yields the canonical zero), so the
oracle and every backend keep one story and no Core semantics change. That ADR is
more finished than the review assumed.

**The residual gap is narrower and real:** sums give *safety*, not *short-circuit*.
Guard arms still all compute (`calc.flow`: `a / b` runs when you picked `+`), and
early exit inside an arm is still designed-out (L1405). Cheap dispatch and
data-dependent early exit remain undesigned — a separate item, not a coproducts item.

### 3.3 Flat indices vs recorded shape

The surface makes the user write `t / 128 -> i; t % 128 -> j`, and S28's
`TileRead.ksplit` then *recovers* `(k÷div, k%div)` from that div/mod pair. The
compiler is reconstructing structure the surface discarded. Not urgent — the
recovery works and is generic — but N-D shape in the type would let the record carry
it directly instead of re-deriving it. Worth a candidate ADR when the array work
(ADR-0023/0024) is next opened.

### 3.4 `loop` vs map/fold — Sapir's open question, and the missing third form

Sapir: *"not even loop is a map. Maybe right to remove it."*

Correct, and the review overstated. `fib` is neither map nor fold — carried state,
non-associative, intermediates are the point. The general shape between `fold` and
"arbitrary loop" is **`scan`**, and it is missing from the op set. Consequence today:
**IIR filters and recurrences — the direct sibling of the FIR window rung that won at
S28 — fall off the ladder entirely**, because written as `loop` they are opaque to
every deduced query. And scan has a known parallel geometry (log-depth tree) on every
backend, which a `loop` cannot expose.

**ADR-0036** proposes `scan` as a Core op and recommends *keeping* `loop` as the
honest escape hatch for genuinely sequential iteration — while noting that removing
it becomes safer once `scan` exists, since the residue needing it shrinks. That
decision is left open, as flagged.

### 3.5 Generic-language work — review's objection withdrawn

The review argued the missing pieces (coproducts, modules, dynamic arrays, recursion,
stdin) should wait behind the kernel-language story. Sapir's position — they are
implementations and decisions that do not break the core, and being generic is
important — is right, and the co-execution goal is why: **a language that places
parts of one program on CPU + GPU + FPGA simultaneously needs to be able to express
whole programs, not kernels.**

Two items are *not* "just implementation" and want ADRs before someone adds a
builtin:

- **`stdin` reopens the token model.** `IoToken` is output-only today
  (`Print : (IoToken × P) → IoToken`). An input effect is
  `Read : IoToken → (IoToken × A)` — a token-carrying *value production*, a shape the
  model has never had. Small, but E2/E3 want re-reading against it.
- **Traps across `Loc`s.** Currently per-backend (llvm exit-101, CUDA exit-102 +
  device flag, S24 speculate-and-order for the pool). Co-execution needs one protocol
  across backends.

Both are recorded in **ADR-0035** D4.

---

## 4. The positioning upgrade

Sapir's co-execution framing — *"compile a source into multiple backends at the same
time to facilitate communication on the same machine (cpu-cuda-fpga), and
backend-specific overrides for portability"* — is **stronger than what VISION.md
currently sells.**

VISION §3's options are all *portability*: same source runs on A, or on B. That is
crowded ground (DaCe, XLA/PJRT, SYCL, Mojo/MAX). **Co-execution** — one program whose
parts are placed across several targets at once, with the transmissions between them
typed and cost-visible in the same IR — is a different claim, and Flow is
structurally built for it where the others are not: `Loc`/`Trm` are first-class in
the method (FRAMEWORK §0/§4.2, ADR-0014), placement is already a deduced query
(`path_plan`), effects are a linear token chain so cross-`Loc` ordering has a carrier,
and the absence of aliasing makes "what must be transmitted" exact.

And the correctness statement it produces is the sharpest one in the project:
**byte-equal output under any placement** — the natural extension of S24's byte-equal
under any thread count. That is the product.

Recorded as **O8** in VISION §3/§4 and as the recommended north star in §7;
designed in **ADR-0035** (post-M5, gated on modules + dynamic arrays + the
second-consumer discharge + a CLI).

---

## 5. What changed in the docs, this pass

| Artifact | Change |
| --- | --- |
| `VISION.md` | §3/§4 gain **O8 co-execution**; §5.1 amended with the geometry/constants split (the "all a vendor does is write a backend" retirement is refined, not reversed); §7 north star becomes O8; §9 open questions updated |
| `ADR-0033` (new, candidate) | Second-consumer proof obligation — CUDA consumes `tile_plan`; trigger is CPU saturation per Sapir; interim per-rung paper guard |
| `ADR-0034` (new, candidate) | Placement constants searched not set — the generic autotuner over the record; extends ADR-0032 D4 |
| `ADR-0035` (new, candidate) | Co-execution; `Trm` as typed cross-backend transmission; backend-override seam for `stdin`/traps |
| `ADR-0036` (new, candidate) | `scan` as a Core op; `loop`'s role stated; R-LS lift rule |
| `docs/suggestions.md` | Rows #11–#13 |

Not touched: `docs/STATUS.md` and `docs/sessions/` (session-log domain, reconcile-only
per plan.md's standing constraint); all code (the S29 tree is mid-flight and
uncommitted).
