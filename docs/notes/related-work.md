# Related work — an honest survey

Written: 2026-07-18 · post-review correction (plan.md worker 3).
Status: **informational note, non-binding.** Trigger: a review found `VISION.md`'s
competitive claims misinformed — the O4 "no incumbent makes this claim" cell was
factually wrong, and the core thesis (one source → many targets via categorical
semantics) had uncited prior art. Sources below were re-verified by web search on
2026-07-18. This note corrects the record; it does not change any spec, roadmap,
or positioning decision (those move by ADR).

The short version: **"provable correctness" is taken** (mechanized, single-target:
Vericert, CompCert), **"one source → many targets" is taken** (Compiling to
Categories, DaCe, Futhark), and **"dataflow + certified codegen sells" is proven**
(SCADE). What is *not* taken is the specific combination Mapal bets on — a portable
cross-target (CPU/GPU/FPGA) correctness claim stated over one categorical IR — and
even that claim is currently informal (the E1 trace theorem is not mechanized).

---

## 1. Compiling to Categories — Elliott, ICFP 2017

**What it is.** Conal Elliott's demonstration that a single functional source can
be compiled to categorical combinators and then *interpreted* into radically
different targets — GPU graphs, digital circuits, VHDL, automatic differentiation —
by giving one category instance per target. The backend-as-functor architecture
Mapal's `category-ir.md §8` specifies is, architecturally, this idea.

**What it achieves relative to Mapal's claims.** It is the direct prior art for
Mapal's core thesis: one source, many targets, correctness carried by the
categorical interpretation. Mapal cannot claim the *idea* is novel. What C2C did
not do: carry the performance engineering, real language surface, or mechanized
correctness theses — it remained a Haskell research demonstration.

**What Mapal can take.** Confidence that the functorial-backend architecture is
sound and has been peer-reviewed; and the honest framing that Mapal's contribution
must be the *engineering* (a real language, real backends, real performance work),
not the category theory itself.
<https://doi.org/10.1145/3110271>

## 2. DaCe — ETH SPCL, 2017–present

**What it is.** A data-centric parallel programming framework: programs (Python/
NumPy and other frontends) are lowered to one **Stateful Dataflow Multigraph
(SDFG)** IR and mapped to high-performance CPU, GPU, and FPGA targets, with
performance portability via graph transformations and autotuning. No category
theory anywhere.

**What it achieves relative to Mapal's claims.** Pragmatically, DaCe already
delivers the "one dataflow IR → many accelerator targets" story Mapal's vision
rests on — and delivers it *performantly*, which Mapal has not yet demonstrated.
Its correctness story is heuristic/engineering, not proof.

**What Mapal can take.** The caution that performance portability, not portability
of correctness, is what the market has rewarded; and a concrete model of the
engineering mass (transformation libraries, autotuning, vendor backends) a
multi-target IR actually requires. DaCe is also the strongest existing competitor
for O5-shaped pitches.
<http://spcl.inf.ethz.ch/Research/DAPP/> · <https://github.com/spcl/dace>

## 3. Futhark — DIKU, 2014–present

**What it is.** A statically typed, purely functional, data-parallel array
language with a heavily optimizing ahead-of-time compiler generating GPU code
(CUDA, OpenCL) or multicore CPU code. Its **uniqueness types** support race-free
in-place array updates inside an otherwise pure semantics.

**What it achieves relative to Mapal's claims.** The near-twin: pure semantics +
multi-target accelerator codegen + the exact "pure `update`, in-place when the
source is dead" play that `docs/notes/array-update-design.md` (Option A) derives
from linearity. Futhark proves that design works in production compilers — and it
is the cautionary tale: a decade of excellent research engineering, strong
benchmarks, and still a small user base.

**What Mapal can take.** (a) Validation of the uniqueness/last-use in-place
lowering plan for arrays — this is proven practice, not a research risk. (b) The
adoption warning: technical excellence in a pure parallel language does not by
itself create users; the beachhead/wedge strategy in VISION §7 exists precisely
because of stories like this.
<https://futhark-lang.org/> ·
<https://hjemmesider.diku.dk/~zgh600/Publications/pldi17.pdf>

## 4. Vericert — Herklotz, Pollard, Ramanathan, Wickerson, OOPSLA 2021

**What it is.** The first mechanically verified high-level synthesis tool: extends
CompCert, written and proven in Coq, compiling a C subset to Verilog with a
machine-checked theorem that the hardware preserves the software's behavior.

**What it achieves relative to Mapal's claims.** This falsifies VISION's old O4
cell as written — an incumbent *does* make (and mechanize) a provable-correctness
claim, in exactly the software→hardware direction O7 cares about. The honest
boundary: Vericert is **single-target** (C → Verilog) and makes no portable
cross-target claim; Mapal's intended claim is over a *family* of targets from one
source.

**What Mapal can take.** The bar. "Provable" in this neighborhood means
mechanized proof in a proof assistant, not informal functor laws. Mapal's E1 trace
theorem is currently informal; VISION §9 already asks how strong the claim must
be to be sellable — Vericert is the reference point for the strong answer.
<https://doi.org/10.1145/3485494> · <https://github.com/ymherklotz/vericert>

## 5. CompCert — Leroy et al.; commercially licensed by AbsInt

**What it is.** The formally verified optimizing C compiler: correctness of
compilation proven in Coq, commercially supported, targeting ARM, PowerPC, x86,
and RISC-V machine code for safety-critical embedded software. In early 2026 it
was **officially qualified for the ATR 42/72 MFC_NG avionics computer**, with
certification credits claimable under DO-178C / DO-333 / DO-330 — the first time
compiler usage itself earned such credits.

**What it achieves relative to Mapal's claims.** The commercial proof that
provable compiler correctness is something regulated markets *pay for* — the
strongest evidence in this note for the O4 wedge. Its limits are Mapal's opening:
single-language, single target family (conventional CPUs), no accelerator story,
no cross-target equivalence claim.

**What Mapal can take.** The certification workflow as a business-model precedent
(cf. VISION §6 "certification-as-a-service"), and the discipline lesson: CompCert
spent ~15 years from first proofs to avionics qualification. Mechanization is a
long road; plan the E1 work accordingly.
<https://compcert.org/> · <https://www.absint.com/compcert/index.htm> ·
<https://www.absint.com/releases/260320.htm>

## 6. Halide formal semantics — Reinking, Bernstein, Ragan-Kelley, 2020/2022

**What it is.** The first formalization and soundness metatheory for a
user-schedulable language — Halide, the dominant image/array-pipeline DSL — whose
algorithm/schedule split is the canonical solution to the "schedule is not
portable" caveat recorded in VISION §5 truth #1.

**What it achieves relative to Mapal's claims.** Halide is no longer purely
heuristic at the language level: its core semantics and scheduling soundness have
formal treatment. (The production compiler's *implementation* is not verified —
VISION O2's "schedule-bug surface" remark survives, but only in that narrower
form.)

**What Mapal can take.** The algorithm/schedule split itself: if Mapal's
performance lives per-backend (VISION §5 #1), an explicit, semantics-preserving
scheduling layer is the proven shape for exposing it — and "soundness of the
scheduling language" is now a published research bar, not an open field.
<https://www2.eecs.berkeley.edu/Pubs/TechRpts/2020/EECS-2020-40.html>

## 7. SCADE — Ansys (from Lustre/Esterel), commercial

**What it is.** A synchronous *dataflow* language and model-based suite used for
safety-critical real-time software for 20+ years. Its code generator (KCG/KCC) is
qualifiable as a DO-330 TQL-1 tool under DO-178C and certified under ISO 26262
(ASIL D/C), IEC 61508 (SIL 3), and EN 50128 (SIL 3/4).

**What it achieves relative to Mapal's claims.** The market proof that *dataflow +
certified code generation* is a workflow customers pay for at scale — the closest
existing analogue to Mapal's O4+O7 combination, sold into exactly the regulated
domains VISION names. Its limits: software targets only (C/Ada), synchronous
reactive domain, proprietary, no GPU/FPGA acceleration story, no cross-target
equivalence claim.

**What Mapal can take.** Evidence that dataflow is the native shape of the
certified-embedded buyer; the qualification/certification vocabulary (TQL-1,
DO-330) any future "certified Mapal backend" pitch must speak; and a reminder that
this market buys *workflows and evidence*, not languages.
<https://www.ansys.com/products/embedded-software/ansys-scade-suite> ·
<https://dl.acm.org/doi/10.1145/3427763.3432350>

---

## 8. Bend / HVM — HigherOrderCO, 2023–present

Added: 2026-07-22 (S20 marathon, on the user's "look at other implementations like
Bend"). Sources: [Bend README](https://github.com/HigherOrderCO/Bend),
[hvm.cu](https://raw.githubusercontent.com/HigherOrderCO/HVM/main/src/hvm.cu),
[Futhark PLDI'17](https://hjemmesider.diku.dk/~zgh600/Publications/pldi17.pdf),
[Munksgaard PhD 2023](https://di.ku.dk/english/research/phd/phd-theses/2023/Philip_Munksgaard_Thesis.pdf),
[Accelerate ICFP'13](https://benl.ouroborus.net/papers/2013-accoptim/optimising-ICFP2013-sub.pdf),
[Dex ICFP'21](https://arxiv.org/abs/2104.05372),
[folding-floats (Futhark blog)](https://futhark-lang.org/blog/2024-09-05-folding-floats.html),
[PPoPP'19 incremental flattening](https://elsman.com/pdf/ppopp19.pdf).

**What it is.** Bend compiles high-level functional programs to interaction-net
graphs (HVM) whose redexes reduce in parallel each turn — parallelism is the
*dependence structure*, not an annotation (a linear-recursion sum is serial; a
midpoint-split sum is parallel, with no keyword). HVM2's CUDA evaluator is one
persistent megakernel over a single `cudaMalloc`'d heap (`struct GNet` — the
whole graph arena in one allocation, per-thread bump allocation inside).

**What it achieves relative to Mapal's claims.** It is the existence proof that
"write high-level, get GPU parallelism" can ship — but at a price Mapal is not
willing to pay: dynamic scheduling and reordered float reduction break any pinned
evaluation order, and the Bend README benchmarks Bend only against *itself*
(12.15 s interpreter → 0.21 s CUDA bitonic) — the methodological anti-pattern the
marathon's native-baseline matrix exists to avoid.

**What Mapal took (and rejected), S20.** Taken: tree reduction needs
associativity read off the graph, not an annotation (→ ADR-0028, exact-op folds
only — the folding-floats rule: the compiler may not reorder what the reference
interpreter doesn't); the whole-heap arena existence proof (→ smart arenas
v1.0, with Futhark's compile-time last-use → interference → coloring as the
*algorithm* — Mapal's static sizes make the runtime zero-scan unnecessary);
Futhark's `stream_red` kernel shape (sequential chunks in registers, parallel
combine) for the tree-fold wave; Accelerate's delayed-array + "don't fuse past a
shared consumer" legality rule (→ the region-emission cost model); Dex's
inline-everything-then-compose (external confirmation of Sapir's strip-to-primitive
directive, Move 1 landed S20 as the `inline` pass); version-per-size mapping from
incremental flattening (Mapal's static sizes collapse it to a compile-time switch).
Rejected: the megakernel evaluator (dynamic scheduling breaks oracle-order
pinning; register-pressure collapse is the known failure mode — region emission
is the right stopping point); any loose float-reduction contract (the oracle pins
order — f64 tree reduction deferred to the canonical-tree re-pin candidate).

---

## Also noted (verified, lower priority)

- **Exo** (Ikarashi et al., PLDI 2022) — exocompilation / user-schedulable
  languages; pushes the Halide scheduling idea toward hardware accelerators with
  a verified-rewrite sibling line (Liu, Bernstein, Chlipala, Ragan-Kelley, POPL
  2022). Relevant if Mapal grows a scheduling layer. <https://doi.org/10.1145/3519939.3523446>
- **Kami** (Choi et al., ICFP 2017) — Coq platform for parametric hardware
  specification with modular verification; the verified-hardware neighborhood
  Vericert/Mapal-Verilog live in. <https://doi.org/10.1145/3110268>
- **MLIR / XLA / TVM / Triton / SYCL / Mojo-MAX** — the heuristic portability
  incumbents, already covered in VISION §5 truth #2; not repeated here.

## Net correction to the positioning

| Old claim (VISION O4) | Corrected claim |
| --------------------- | --------------- |
| Provable cross-target correctness is "a claim **no incumbent makes**" | No *portable cross-target* correctness claim exists among the heuristic pipelines; Vericert and CompCert make **stronger but single-target, mechanized** claims. They are the bar to beat, not absent prior art. |
| (uncited) one functional source → many targets via categorical semantics | Prior art: Compiling to Categories (the idea itself), DaCe and Futhark (engineering realizations without category theory). Mapal's delta is the *combination* with a correctness argument — currently informal. |
