# VISION.md — Flow as a portable accelerator layer

**Status:** Positioning / north-star — **non-binding**. · **Date:** 2026-06-13 · **Amended 2026-07-25** (§3/§4 O8 co-execution; §5.1 geometry-vs-constants; §7 north star → O8; §9): review record in `docs/notes/2026-07-25-thesis-review.md`, decisions in ADR-0033…0036. Amendments are recorded options, not scope changes — §8 stands.

> This document records *strategic options* for what Flow could become in the
> market. It is the "why," not the "what." It **does not** alter the frozen
> Level-A spec (`docs/spec/category-ir.md` + `ERRATA.md`), the Flow-Core scope
> (`HANDOFF.md §4`), or the roadmap (`HANDOFF.md §8`). Per the project's own
> stated risk — *"the single biggest project risk is another year of
> specification without an implementation"* — nothing here adds scope to the next
> sessions. The technical thesis is still tested by **M5** (one `sepia.flow`,
> three targets); the market thesis only becomes real *after* that demo exists.
> Changes of direction are made by ADR, not by editing this file.

---

## 1. The thesis

NVIDIA controls accelerated computing through **CUDA**: nearly every framework
targets CUDA first, which raises the barrier for any new GPU/ASIC vendor — their
hardware is useless until the entire software ecosystem is re-ported to it. This
is a lock-in moat, not only a technology lead.

Flow's bet: **decouple the programming model from the hardware vendor.** A new
hardware company writes *one* backend for Flow and instantly inherits Flow's
entire ecosystem — every program, library, and tool. The same source runs on
CPU, GPU, FPGA, or novel silicon, with the compiler's *correctness argument being
functoriality*. CUDA uses LLVM to reach PTX; Flow does the same trick one level
up, with **plug-and-play backends** for both hardware developers (build a target)
and hardware interpreters (run the ecosystem on it).

## 2. Why Flow is structurally suited (this is not a pivot)

The vision is already latent in the architecture — it does not require redesign:

- **Backend = functor.** `category-ir.md §8` defines a backend as a functor
  `F : Flow-Cat → T` out of the IR. A new vendor backend is, in the model,
  *adjoining one functor* — the plug-and-play story is literally the spec.
- **Dataflow → spatial hardware.** A dataflow graph maps to FPGAs/ASICs far more
  naturally than CUDA's SIMT model. The existing Verilog backend is evidence the
  model reaches past GPUs in a way CUDA cannot.
- **Provable cross-target equivalence.** Functor laws give *"same source =
  provably same function"* across CPU/GPU/FPGA — a claim heuristic pipelines
  (MLIR/XLA/TVM) cannot make.
- **Parallel-first by default.** `seq` opts into ordering; everything else is
  structurally parallel — the right default for accelerators.
- **Backend/runtime is the one real `Loc`/`Trm` seam** (ADR-0014): placement and
  host↔device transmission carry real cost *only* at the target boundary — which
  is exactly where this market thesis lives.

## 3. The strategic options

Eight options. They are **not all mutually exclusive**: O1↔O2 is the core fork
(boil-the-ocean vs. niche-first); O3 and O4 are cross-cutting levers usable under
any positioning; O5/O7 are expansion plays the FPGA/ASIC + functor strengths
unlock; O6 is the tempting-but-hardest market. **O8 was added 2026-07-25** (Sapir's
direction; ADR-0035) and displaces O1 as the north star — see §7.

- **O1 — Universal compute layer ("the anti-CUDA").** Be the portable model for
  *every* accelerator — the default way anyone writes parallel code; CUDA's
  replacement.
- **O2 — Domain beachhead: real-time image/signal processing.** Win one vertical
  where dataflow is the native shape (the `HANDOFF.md` stated strategy + the
  `sepia.flow` demo), then generalize.
- **O3 — Embeddable kernel language (linked lib / FFI).** Compile accelerated
  kernels, export as a C-ABI library callable from Python/C++/Rust — a *guest* in
  existing stacks, no rewrite required.
- **O4 — Provable cross-target correctness layer.** Lead with the claim
  incumbents can't make: the same source is provably the same function on
  CPU/GPU/FPGA (functorial backends).
- **O5 — Novel-silicon bring-up & vendor enablement.** The tool a new GPU/ASIC
  vendor uses to validate hardware against the interpreter-oracle and inherit the
  ecosystem by writing one backend functor (your original "open the market"
  framing).
- **O6 — AI/ML compute.** A parallel-first language aimed at AI workloads;
  compete in the hottest, most-funded market.
- **O7 — High-level hardware synthesis (FPGA/ASIC).** Dataflow → spatial
  hardware: a higher-level, correctness-checked alternative to HLS and RTL DSLs;
  leverages the existing Verilog backend.
- **O8 — Co-execution across heterogeneous targets.** Not "the same source runs
  on CPU *or* GPU *or* FPGA" (that is portability — O1/O5/O7, and crowded), but
  **one program whose parts are placed across several targets at once**, with the
  transmissions between them typed and cost-visible in the same IR. The
  heterogeneous box (CPU + GPU + FPGA, moving data correctly and not too often)
  is a real, unsolved, underserved problem, and the claim it produces is the
  sharpest one available to this project: **byte-equal output under any
  placement** — the natural extension of S24's byte-equal under any thread count.
  Design: ADR-0035; post-M5, gated on modules + dynamic arrays + a CLI.

## 4. Options × competition × problems × opportunities

| ID | Option | Primary competition | Key problems / risks | Opportunities |
| -- | ------ | ------------------- | -------------------- | ------------- |
| **O1** | Universal compute layer ("anti-CUDA") | CUDA; Mojo/MAX; MLIR; OpenCL; SYCL/oneAPI | Head-on with NVIDIA + the best-funded players; platform cold-start (no users → no vendors → no users); portability ≠ performance at the broadest scope; needs a decade of two-sided subsidy NVIDIA already paid | Largest TAM; the "default parallel language" prize; rides industry-wide anti-lock-in demand |
| **O2** | Domain beachhead: image/signal processing | Halide; OpenCV+CUDA; MATLAB/Simulink; vendor DSP toolchains; OpenVINO | Niche is itself contested (Halide is strong); must beat tuned libraries on a real workload; risk of staying boxed in the niche | Dataflow is the native shape here; matches the frozen `sepia.flow` demo + stated strategy; concrete buyers; provable correctness is a real edge over Halide's schedule-bug surface |
| **O3** | Embeddable kernel lib (FFI / linked lib) | Triton; Numba; Taichi; JAX/Pallas; raw CUDA/C | ABI/FFI + host↔device memory management is where the hard engineering hides; must match kernel performance; "just a kernel DSL" caps the ambition/valuation narrative | **Easiest adoption** — no rewrite, meet users where they are; fastest path to real usage and feedback; composes with every other option |
| **O4** | Provable cross-target correctness layer | *None portable*; CompCert (CPU-only certified); Vericert (mechanized C→Verilog HLS); MLIR/XLA/TVM are heuristic (bug-for-bug target drift) | "Provable" only as strong as the functor proofs (the E1 trace theorem is still informal); correctness alone doesn't sell if performance is poor; market may not *pay* for correctness | No *portable cross-target* functorial claim among the heuristic pipelines — but Vericert/CompCert are **stronger, single-target, mechanized** proofs: they are the bar to beat, not absent prior art (see `docs/notes/related-work.md`); decisive in safety-critical/regulated domains (CompCert is DO-178C-qualified, SCADE sells exactly this workflow); defensible moat; already the spec's core thesis (§8) |
| **O5** | Novel-silicon bring-up / vendor enablement | Vendor MLIR backends; TVM BYOC; OpenXLA PJRT plugins | Chicken-and-egg (vendors want an *existing* ecosystem); long enterprise sales cycles; needs ≥1 backend + real users to be credible | Your original "open the market" thesis; new GPU/ASIC startups are a real, growing, underserved buyer; one backend = inherit the whole ecosystem; the correctness oracle is genuinely useful for HW validation |
| **O6** | AI/ML compute | OpenXLA/XLA; Triton; torch.compile/Inductor; TVM; cuDNN/CUTLASS; Mojo/MAX | Hardest, most-defended ground; dense GEMM/attention favors tuned libs + tensor cores, where general dataflow has **no edge**; fighting NVIDIA's full stack | Largest spend; real upside **if** won via streaming/irregular/heterogeneous dataflow (not dense GEMM); rides anti-CUDA sentiment |
| **O7** | High-level hardware synthesis (FPGA/ASIC) | Vivado/Intel HLS; Chisel; Bluespec; Spatial/Dahlia; Halide-HLS | FPGA/ASIC market is smaller + specialized; verification & timing-closure depth; Verilog backend today is feedforward + single-loop FSM only | Dataflow → spatial is more natural than SIMT; provable lowering is rare in HLS; differentiated against both the CUDA world and the RTL world; leverages the existing Verilog backend |
| **O8** | Co-execution across heterogeneous targets | *Thin.* oneAPI/SYCL (single-source, but placement is manual and correctness is not a claim); CUDA unified memory + streams (single-vendor); DaCe/XLA (portability, not simultaneous multi-target placement); MPI/Ray (distribution without a shared semantics) | Largest scope of any option — needs modules, dynamic arrays, a CLI, ≥2 backends consuming the deduced queries, and a cross-`Loc` runtime; placement cost models are hard; nothing ships until the CPU and GPU legs both exist | The one claim with a **structural** reason Flow gets there first: `Loc`/`Trm` are first-class in the method (ADR-0014), placement is already a deduced query (`path_plan`), effects are a linear token chain so cross-`Loc` ordering has a carrier, and no aliasing means "what must be transmitted" is exact reachability. Produces the project's sharpest correctness statement — **byte-equal under any placement**. Subsumes O5/O7 as *participants* rather than destinations |

## 5. Cross-cutting truths (apply to every option)

1. **The language imposes no inherent performance ceiling — performance lives in
   the backend, and that cuts both ways.** Flow is not inherently slower. Its
   pure, single-source/single-target dataflow IR is arguably a *better*
   optimization substrate than C/CUDA: no aliasing ambiguity (no pointers in
   Core), explicit data dependencies, explicit parallelism, and intent (`map f`)
   expressed instead of mechanism (a hand-written loop) — so the backend is free
   to fuse, tile, and re-schedule. High-level data-parallel compilers (Halide,
   XLA, Triton, Futhark) show such a language can match or beat hand-written
   kernels. **But the performance work does not vanish — it relocates into each
   backend, per target.** A *correctness*-preserving functor is cheap and
   provable; a *performance*-competitive backend is the same tiling / fusion /
   memory-hierarchy / autotuning effort CUDA represents, redone for each target.
   Two surviving caveats: (a) the performance-bearing **schedule is not
   portable** — the *algorithm* ports, the tuning must be re-autotuned per target
   (why Halide splits algorithm from schedule); (b) the top of the envelope
   (tensor-core GEMM) may need intrinsics or a library escape hatch. Retire the
   marketing line *"all a vendor does is write a backend"*: they inherit
   *correctness* for free, *performance* only with real investment. *Portable
   performance is the graveyard this class of project usually dies in — not
   because the language caps it, but because per-target tuning is expensive.*

   **Amendment, 2026-07-25 (S25–S28 evidence; ADR-0034, `docs/notes/2026-07-25-thesis-review.md`).**
   Caveat (a) is sharper than "the schedule is not portable," and the sharper form
   is *more* favourable to the thesis. The schedule decomposes into two parts with
   different portability:
   - **Geometry** — which reads broadcast vs. stride, which axes split, the nest
     order, what is legally interleavable. **Deduced** from the graph, exact,
     cheap, backend-independent (`tile_plan`, `TileRead.ksplit`, `path_plan`).
     **This part ports, and that is now measured, not hoped:** the recognizer
     holds no matrix concept — one affine-address rule — and S28 carried the
     ladder built on matmul to a FIR window rung (**zero flow-ir change**) and a
     conv2d micro-kernel in one session, both winning their tables.
   - **Constants** — tile factors, panel sizes, unroll depth, prefetch distance,
     task grain. Facts about a cache hierarchy and a register file, **not** about
     the program. These genuinely do not port, and today they are hand-set
     literals swept on one machine.

   So the honest claim is **not** "portable performance is free" and **not** "the
   schedule must be re-engineered per target." It is: *the geometry is deduced
   and portable; the constants are measured per target* — and measuring constants
   is a **search**, which is itself backend-generic (one tuner over the recorded
   facts, per-`Loc` table; ADR-0034). That is the designed exit from the
   graveyard, and it is the reason the tile ladder is **not** the GEMM-chasing it
   can be mistaken for: matmul is the hardest known shape and therefore the right
   stress test for a geometry engine; BLAS is the oracle for whether the geometry
   is right, not the product. Describe the work accordingly — *geometry
   recognition validated on matmul*, never "matmul performance."

   One thing this does **not** yet establish: every rung to date has exactly one
   consumer (llvm). Cross-backend transfer is designed (ADR-0032) and unmeasured
   until CUDA consumes `tile_plan` — the named exit condition of the CPU phase
   (ADR-0033).
2. **The field is crowded with heavy hitters pursuing this exact thesis** — MLIR,
   TVM, IREE, XLA/StableHLO, Triton, SYCL/oneAPI, OpenCL, and especially
   **Mojo/MAX** (the anti-CUDA-lock-in stack from Chris Lattner — creator of
   LLVM, Clang, and Swift). This
   *validates* the thesis but means "be the universal layer" is a fight on
   everyone's turf. Flow needs a wedge they lack. (For the verified-correctness
   and one-source→many-targets prior art specifically — Compiling to Categories,
   DaCe, Futhark, Vericert, CompCert, Halide's formal semantics, SCADE — see
   `docs/notes/related-work.md`.)
3. **Platform cold-start.** "Vendors write a backend and get the ecosystem free"
   only works once an ecosystem worth getting *exists*. Today: zero users. The
   answer is not to out-subsidize NVIDIA — it is to out-*niche* them.

## 6. Open source — the delivery model the thesis requires

Open-sourcing is not a side question; it is structurally entailed by the thesis,
and it is the **mechanism that makes §5's per-target performance problem
tractable**. The two arguments are one: performance lives in backends → a
performant backend is large, per-target work → open source distributes that work
across vendors and community instead of leaving it to one team. An anti-lock-in
layer that is itself a proprietary single-vendor dependency is a contradiction;
the open compiler tooling the portability story rests on — **LLVM** and
**MLIR** — and the CUDA alternatives that gained real traction — **TVM**,
**Triton**, **IREE** — are all open source. **Open source therefore underlies every option in §3–§4, not a
new option beside them.**

**Why it accelerates the thesis**

- **Vendor trust.** No hardware company bets its silicon's software stack on a
  closed compiler it cannot inspect or fork. Permissive open source de-risks the
  exact adoption O5 needs.
- **Distributed performance work.** Backends, optimizers, and autotuning
  databases get community + vendor contributions — the only realistic way to fund
  *portable performance* across many targets.
- **A neutral "Switzerland."** Open governance lets competing vendors trust one
  layer none of them controls — what an anti-monopoly standard requires.
- **More eyes, faster oracle.** The differential-testing method (interpreter as
  oracle) strengthens with every contributed program and target.

**Honest nuances**

- **Necessary, not sufficient.** Open source *enables* an ecosystem; it does not
  *cause* one. Most OSS projects get zero contributors — users and a compelling
  reason to contribute still have to exist.
- **Timing.** Build in the open from day one (public repo, permissive license for
  credibility), but spend the one-time launch moment when something *runs* — the
  **M5 tri-target demo is the ideal launch artifact**, not a pre-interpreter P2
  repo.
- **License is strategic.** Permissive (**Apache-2.0**, with its patent grant) is
  the LLVM/MLIR standard and what vendors require; copyleft (GPL) would repel the
  hardware vendors O5 targets. Almost certainly Apache-2.0 — worth an explicit ADR.
- **Name the business model, don't foreclose it.** If the core is open and value
  is "vendors write backends," monetization lives elsewhere: hosted autotuning,
  correctness / certification-as-a-service, premium or proprietary backends,
  support and integration. Decide deliberately so open-sourcing keeps options
  open rather than closing them.

## 7. The recommended wedge (how the options combine)

The strongest version of the thesis is **not** "replace CUDA for AI training"
(O1/O6 head-on — the densest, best-defended ground, where the incumbent stack is
tensor cores plus a decade of cuBLAS/CUTLASS hand-encoding). It is:

> **One source, placed across heterogeneous targets at once, with the schedule
> deduced from the graph and the output byte-equal under any placement.**

That sentence is three claims stacked, each earned by a different piece of work
already in the repo: *deduced schedule* (§5.1 amendment — the tile ladder, proven
shape-generic at S28), *heterogeneous placement* (O8 / ADR-0035 — `Loc`/`Trm`
first-class), *byte-equal* (the oracle + differential duty, extended from S24's
byte-equal-at-any-thread-count).

**Correction, 2026-07-25:** the older framing of this section — "general dataflow
has no advantage on GEMM" — is retired as too strong. The advantage on
GEMM-shaped work is not the *algorithm* (a library will always encode that); it
is that **the same deduction that reaches BLAS-class code on CPU reaches
smem/mma on GPU and a systolic array on FPGA**, from naive source, with no
library per target. GEMM remains the wrong thing to *market*; it is the right
thing to *measure against*, because it is where the hand-encoded bar is highest.

A complementary stack rather than one bet:

```mermaid
flowchart LR
    O2["O2 beachhead<br/>image / signal"] --> O5["O5 expansion<br/>silicon bring-up"]
    O3["O3 adoption lever<br/>kernel lib / FFI"] --> O2
    O4["O4 differentiator<br/>deduced + verified<br/>cross-target"] --> O2
    O4 --> O5
    O7["O7 expansion<br/>FPGA / ASIC HLS"] --> O8
    O5 --> O8["O8 north star<br/>co-execution"]
    O8 --> O1["O1 eventual<br/>universal layer"]
    style O2 fill:#4f8cf7,color:#fff
    style O3 fill:#7fc47f,color:#000
    style O4 fill:#f7c04f,color:#000
    style O5 fill:#cf7fcf,color:#fff
    style O7 fill:#cf7fcf,color:#fff
    style O8 fill:#e2683c,color:#fff
    style O1 fill:#9a9a9a,color:#fff
```

- **Now (≤ M5):** O2 (beachhead) as the framing, O3 (kernel-lib) as the adoption
  mechanism, O4 (deduced-and-verified cross-target behaviour) as the
  differentiator — all over an **open-source (Apache-2.0) base** (§6). All fit
  the current roadmap without changing scope.
- **The CPU phase, in flight:** take full advantage of the geometry ladder on one
  backend first (Sapir's sequencing, 2026-07-25), then port. Its **named exit
  condition** is CUDA consuming `tile_plan` (ADR-0033) — the phase ends on a
  thesis test, not by trailing off. Constants become searched rather than set
  (ADR-0034) as part of "full advantage."
- **After M5:** O5 (silicon bring-up) and O7 (FPGA/ASIC HLS) — the expansion
  plays the FPGA/ASIC + functor strengths unlock, and both become *participants*
  in O8 rather than separate destinations.
- **O8 (co-execution): the north star.** Fewer occupants than O1 and a
  structural reason Flow arrives first (§3). Gated on modules, dynamic arrays, a
  CLI, and ≥2 backends consuming the deduced queries.
- **O1 (universal layer):** downstream of O8, not a target in itself; earned,
  never assaulted.
- **O6 (AI/ML):** approach via streaming/irregular/heterogeneous dataflow and via
  *co-execution* of mixed pipelines; never by fielding a GEMM kernel against
  cuBLAS as the product claim.

## 8. What this does not change

- **Frozen Level-A spec untouched** (ADR-0014 firewall): no `category-ir.md`
  edit, no ERRATA entry, no scope change.
- **M5 remains the test of the technical thesis** — does one source actually run
  correctly on three targets? Market positioning is downstream of that proof.
- **Roadmap and Flow-Core scope are unchanged** (`HANDOFF.md §4`, §8). This doc
  is the "why" that motivates the existing "what."

## 9. Open questions (→ future ADRs / decisions)

- Which beachhead workload first proves *portable performance*, not just
  portable correctness? (The honest crux of the whole vision.) **Partly answered
  2026-07-25:** the ladder is proven portable *across shapes* (matmul → FIR →
  conv2d, S28) and unproven *across backends* (one consumer to date). The
  remaining crux is therefore narrower and testable — see ADR-0033.
- **What is the written bar for "full advantage taken on CPU"?** It is the
  trigger for the GPU phase (Sapir's sequencing) and therefore for the only
  experiment that can falsify the transfer claim. Without a bar the trigger
  recedes indefinitely. (ADR-0033 Q1 — the single most consequential open
  question in this file.)
- Does a **cost model** over the recorded geometry replace most of the constant
  search, with measurement only as tie-break (ADR-0034 Q3)? That is the strongest
  form of "optimal out of the box" and the most work.
- **Language discipline:** external phrasing must stay "differentially verified
  against a reference oracle across targets," not "provable" — nothing is
  machine-proven (`flow-as-implemented.md` §4.4). Mechanizing E1 is worth it only
  if the safety-critical market (DO-178C/SCADE territory) is actually targeted,
  since that is the one buyer that pays for it. Decide, don't drift.
- Is the primary buyer the **developer** (O3 adoption) or the **hardware vendor**
  (O5 enablement)? They imply different go-to-market and different first backend.
- How strong must the correctness claim be to be sellable — informal
  functoriality, or the mechanized E1 trace-preservation theorem?
- What is the minimum "ecosystem worth inheriting" that makes O5's vendor pitch
  credible?
- **Open-source license & governance:** Apache-2.0 + neutral governance? When to
  make the repo *public* (early, for credibility) vs. when to *launch* it (at M5,
  for momentum)? **Status 2026-07-25: unexecuted** — no `LICENSE` file, no git
  remote. §6 calls both strategic; neither has been done, and "build in the open
  from day one" has quietly not happened.
- **Business model under an open core:** hosted autotuning, certification-as-a-
  service, premium backends, or support/integration — which, and decided when?
