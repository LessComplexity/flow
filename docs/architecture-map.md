# Whole-system categorical map (Dat/Trn/Loc/Trm)

> Top-level architecture doc (FRAMEWORK §4; ADR-0017). Names the four atoms, lists
> components (each linking to its model), reifies placement where it is a relation,
> and runs the §4.5 coherence checklist against the code. High-level — detail lives
> in the linked component docs. Method + firewall + cross-component bridges:
> [`architecture/categorical-model.md`](architecture/categorical-model.md).
> Plain-terms explainer of the deduced-query layer (`loop_plan`, `path_plan`,
> `tile_plan`, `bounds_proof`, `last_use_plan`):
> [`architecture/deduced-queries.md`](architecture/deduced-queries.md).
> **Firewall (ADR-0014):** everything here is Level B — the compiler's own types and
> passes. Mapal-Cat (Level A, `docs/spec/category-ir.md`, frozen) appears only as data.

## 1. Why (one paragraph)

The compiler is a pipe-and-filter pipeline (FRAMEWORK §7.1) whose correctness story is
"every stage is a typed weld": naming each weld's datum and each pass's `t_from → t_to`
makes a miscompile a *located* defect (a weld whose types don't meet, a pass reading
data nothing delivered) instead of an argument. The physical pair is degenerate today
— which the map states rather than fights — and the one place it becomes real (the
backend/runtime seam) is exactly where the project's hardest theorem (E1 done-protocol)
and biggest cost (H↔D transfer) live. The map keeps that seam visible before any
backend exists.

## 2. The four atoms (at a glance)

**Dat** — the pipe-weld data (each modeled richly in its component's DESIGN):

| Type | Shape | Home |
| --- | --- | --- |
| `𝕊` (source) | one `.mapal` file | input |
| `Token*` | free monoid of spanned tokens | `mapal-syntax` |
| `Program` | thin spanned parse tree | `mapal-syntax` |
| `CategoryIr` | sealed dataflow graph (objects/morphisms, edge-only dataflow, ADR-0013) | `mapal-ir` |
| `RValue` env | interp value domain over `ObjectId` | `mapal-interp` |
| `Diagnostic` / `IrError` / `IrViolation` | renderer-free structured errors (three by design — §7.2 of the [audit](architecture/categorical-model.md)) | per crate |
| `TargetText` | emitted `.ll` / `.cu` / `.v` source | `mapal-backend-llvm` (built, S13); `mapal-backend-cuda` (built, S15); verilog planned |

**Trn** — the passes (`⊸` = effectful):

| Trn | `t_from → t_to` | Component | State |
| --- | --- | --- | --- |
| `lex` | `𝕊 → LexOutput` | syntax | built |
| `parse` | `Token* → ParseOutput` | syntax | built |
| `lower` | `(𝕊 × Program) ⇀ CategoryIr ⊕ Diag*` (partial — domain = Mapal-Core) | lower | built |
| `validate` | `CategoryIr → Violation*` (independent oracle) | ir | built |
| `eval`/`run` | `(CategoryIr × Input × Fuel) ⇀ Output ⊸` (fueled, E1) | interp | built |
| `check` | `Src × Program × CategoryIr → Diag*` (ε = accept) | check | built |
| rewrite passes | `CategoryIr → CategoryIr` (plan+replay; layers 3–4 + map fusion) | rewrite | built (S12) |
| backend emit | `CategoryIr → TargetText` (ADR-0020 `emit(&CategoryIr) -> Result<String, EmitError>`) | backends | built (llvm S13, cuda S15); verilog planned |
| `render` | `Diag* → 𝕊 ⊸` (the lone renderer) | cli | planned |

**Loc** — **collapsed** for the compiler pipeline: one OS process end-to-end (§7.1
degenerate case — the model reduces to `Dat` + `Alg`). The physical pair de-collapses at
the backend/runtime seam: backend-llvm made it real first (S13 — the external `clang`
toolchain + the native process), and **backend-cuda gives the project its first genuine
two-site execution (S15)**: the host process and the GPU device, with kernels, device
buffers, and the trap flag resident on the device. FPGA fabric remains a planned `Loc`
(verilog).

**Trm** — **none inside the pipeline** (every pass handoff is a same-`Loc` `Trn`). Real at
the backend harness boundaries: the llvm differential's `stdout`/exit-code capture (S13),
and **the first `Trm`s whose cost is real, priced, and latency-visible (S15)**: backend-cuda's
`cudaMemcpy` H↔D crossings — literal uploads, launch args, the per-launch trap-flag readback,
`Index`/`Fold` scalar readbacks — each enumerated and counted in backend-cuda DESIGN §2
(the transfer inventory; zero whole-array D→H by construction). Still ahead: FPGA streaming,
the E1 `valid/busy/done/result` handshake.

## 3. Components

| Component | Owned `Trn` | Built/active when | Model | Map |
| --- | --- | --- | --- | --- |
| syntax | `lex`, `parse` | built (P1) | [DESIGN](components/syntax/DESIGN.md) | [IMPL](components/syntax/IMPLEMENTATION.md) |
| ir | builder ops, `seal`, `validate`, `topo`/`sccs`, Mermaid dump | built (P2) | [DESIGN](components/ir/DESIGN.md) | [IMPL](components/ir/IMPLEMENTATION.md) |
| lower | `lower` (passes A–E) | built (P2) | [DESIGN](components/lower/DESIGN.md) | [IMPL](components/lower/IMPLEMENTATION.md) |
| check | E2 effect legality + Return exclusivity (typing at boundary; E3 vacuous-by-proof) | tested (25) | [DESIGN](components/check/DESIGN.md) | [IMPL](components/check/IMPLEMENTATION.md) |
| interp | `eval`/`run` — **the oracle** | built (P3/M1) | [DESIGN](components/interp/DESIGN.md) | [IMPL](components/interp/IMPLEMENTATION.md) |
| rewrite | plan+replay rewriter: const fold/CSE/DCE + map fusion, R1 property harness + testgen | tested (S12) | [DESIGN](components/rewrite/DESIGN.md) | [IMPL](components/rewrite/IMPLEMENTATION.md) |
| backend-llvm | `F_LLVM` emit (+ `mapal-rt` runtime seam, ADR-0020) | built (P5/M2, S13) | [DESIGN](components/backend-llvm/DESIGN.md) | [IMPL](components/backend-llvm/IMPLEMENTATION.md) |
| backend-cuda | `F_CUDA` emit (host/device split; kernels + H↔D `Trm`s) | built (P6/M3, S15) | [DESIGN](components/backend-cuda/DESIGN.md) | [IMPL](components/backend-cuda/IMPLEMENTATION.md) |
| backend-verilog | `F_Verilog` emit + done-protocol | planned (P7/M4) | [DESIGN](components/backend-verilog/DESIGN.md) | [IMPL](components/backend-verilog/IMPLEMENTATION.md) |
| cli | `mapal build\|run\|dump-ir\|test`, `render` | planned (M5) | [DESIGN](components/cli/DESIGN.md) | [IMPL](components/cli/IMPLEMENTATION.md) |

Status detail: [`docs/STATUS.md`](STATUS.md) (global roll-up, HANDOFF §7.1.1).

## 4. Placement (only where runsAt is a relation, §4.2)

| `Trn` / `Dat` | Placements | Why it matters |
| --- | --- | --- |
| `validate` | debug-assert after seal AND the property-test harness — never a mandatory production pass | the oracle's independence is the point; "seal Ok ⇒ validate empty" is a *tested* law, not a call chain |
| running a Mapal program | `interp` (built) AND each backend target (planned) | one meaning, many realisations — the differential tests are the commuting squares; the interp fibre is authoritative (HANDOFF §7.3) |
| `Print{newline}` effect | interp's writer-style token now; each backend's runtime later | E2: effect order is semantics — every placement must observe the same token order |

## 5. Coherence checklist (§4.5 / §8) against the implementation

All six PASS as of S15 (backend-cuda added — the first component with a genuine two-site
execution pair: host process + GPU device, with the H↔D `cudaMemcpy` crossings enumerated
and counted in its DESIGN §2; every one materialised at both ends per Law 2).

- [x] 1. Placement honesty — single process for the pipeline; every pass consumes exactly
      the value the previous returns; interp reads only its `SecondaryMap` env keyed by
      `ObjectId`. No data teleport. backend-llvm's `emit` is still one in-process pass; the
      only cross-`Loc` step is the differential harness spawning `clang`/the native process.
- [x] 2. Transmission well-typing — first real `Trm` landed (S13): the backend-llvm harness
      captures native `stdout`/exit-code, typed by L1 oracle parity (`Done` ⟺ exit 0 + stdout
      byte-equal; `Trapped` ⟺ exit 101). Becomes heavier at the GPU/FPGA seam; the E1
      done-protocol is its hardest instance.
- [x] 3. Placement totality — every built `Trn` has a crate home (§3 table); no
      floating pass.
- [x] 4. Dependency mediation — all cross-crate reach is same-`Loc` Cargo edges;
      `mapal-ir` stays the zero-dep hub (the D8 `SourceLoc` copy at one declared seam,
      `mapal_lower::tys::ir_loc`, exists precisely to keep it so).
- [x] 5. Composition soundness — roll-ups are deduced, not redescribed: `topo`/`sccs`
      recomputed (ir D3/D5), global STATUS deduced from component STATUS, this map
      links down rather than restating.
- [x] 6. runsAt is a relation — no `runsAt` column anywhere; the multi-placement rows
      are reified in §4.

## 6. Modeling smells swept (§3)

Swept in the Session-06 reduction audit (twelve clusters, adversarially verified —
[categorical-model.md §7](architecture/categorical-model.md)): consolidations ratified
(`Object.value?`, no stored `Trace`, deduced topo/SCC), justified twos recorded (two
`SourceLoc`s, `IrError`/`IrViolation`, surface-vs-IR `Ty`) so nobody "fixes" them.
Open candidates live in [`docs/suggestions.md`](suggestions.md).
