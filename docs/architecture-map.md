# Whole-system categorical map (Dat/Trn/Loc/Trm)

> Top-level architecture doc (FRAMEWORK §4; ADR-0017). Names the four atoms, lists
> components (each linking to its model), reifies placement where it is a relation,
> and runs the §4.5 coherence checklist against the code. High-level — detail lives
> in the linked component docs. Method + firewall + cross-component bridges:
> [`architecture/categorical-model.md`](architecture/categorical-model.md).
> **Firewall (ADR-0014):** everything here is Level B — the compiler's own types and
> passes. Flow-Cat (Level A, `docs/spec/category-ir.md`, frozen) appears only as data.

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
| `𝕊` (source) | one `.flow` file | input |
| `Token*` | free monoid of spanned tokens | `flow-syntax` |
| `Program` | thin spanned parse tree | `flow-syntax` |
| `CategoryIr` | sealed dataflow graph (objects/morphisms, edge-only dataflow, ADR-0013) | `flow-ir` |
| `RValue` env | interp value domain over `ObjectId` | `flow-interp` |
| `Diagnostic` / `IrError` / `IrViolation` | renderer-free structured errors (three by design — §7.2 of the [audit](architecture/categorical-model.md)) | per crate |
| `TargetText` | emitted `.ll` / `.cu` / `.v` source | backends (planned) |

**Trn** — the passes (`⊸` = effectful):

| Trn | `t_from → t_to` | Component | State |
| --- | --- | --- | --- |
| `lex` | `𝕊 → LexOutput` | syntax | built |
| `parse` | `Token* → ParseOutput` | syntax | built |
| `lower` | `(𝕊 × Program) ⇀ CategoryIr ⊕ Diag*` (partial — domain = Flow-Core) | lower | built |
| `validate` | `CategoryIr → Violation*` (independent oracle) | ir | built |
| `eval`/`run` | `(CategoryIr × Input × Fuel) ⇀ Output ⊸` (fueled, E1) | interp | built |
| `check` | `Src × Program × CategoryIr → Diag*` (ε = accept) | check | built |
| rewrite passes | `CategoryIr → CategoryIr` (plan+replay; layers 3–4 + map fusion) | rewrite | built (S12) |
| backend emit | `CategoryIr → TargetText` (one contract, three realisations) | backends | planned |
| `render` | `Diag* → 𝕊 ⊸` (the lone renderer) | cli | planned |

**Loc** — **collapsed**: one OS process end-to-end (§7.1 degenerate case — the model
reduces to `Dat` + `Alg`). The physical pair de-collapses only at the backend/runtime
seam: CPU host, GPU device, FPGA fabric are genuine `Loc`s (none exist in code yet).

**Trm** — **none at this scale** (every handoff is a same-`Loc` `Trn`). Real when
backends land: `cudaMemcpy` H↔D (carries buffers), FPGA streaming, the E1
`valid/busy/done/result` handshake. Laws 1–2 start doing real work there.

## 3. Components

| Component | Owned `Trn` | Built/active when | Model | Map |
| --- | --- | --- | --- | --- |
| syntax | `lex`, `parse` | built (P1) | [DESIGN](components/syntax/DESIGN.md) | [IMPL](components/syntax/IMPLEMENTATION.md) |
| ir | builder ops, `seal`, `validate`, `topo`/`sccs`, Mermaid dump | built (P2) | [DESIGN](components/ir/DESIGN.md) | [IMPL](components/ir/IMPLEMENTATION.md) |
| lower | `lower` (passes A–E) | built (P2) | [DESIGN](components/lower/DESIGN.md) | [IMPL](components/lower/IMPLEMENTATION.md) |
| check | E2 effect legality + Return exclusivity (typing at boundary; E3 vacuous-by-proof) | tested (25) | [DESIGN](components/check/DESIGN.md) | [IMPL](components/check/IMPLEMENTATION.md) |
| interp | `eval`/`run` — **the oracle** | built (P3/M1) | [DESIGN](components/interp/DESIGN.md) | [IMPL](components/interp/IMPLEMENTATION.md) |
| rewrite | plan+replay rewriter: const fold/CSE/DCE + map fusion, R1 property harness + testgen | tested (S12) | [DESIGN](components/rewrite/DESIGN.md) | [IMPL](components/rewrite/IMPLEMENTATION.md) |
| backend-llvm | `F_LLVM` emit | planned (P5/M2) | [DESIGN](components/backend-llvm/DESIGN.md) | [IMPL](components/backend-llvm/IMPLEMENTATION.md) |
| backend-cuda | `F_CUDA` emit | planned (P6/M3) | [DESIGN](components/backend-cuda/DESIGN.md) | [IMPL](components/backend-cuda/IMPLEMENTATION.md) |
| backend-verilog | `F_Verilog` emit + done-protocol | planned (P7/M4) | [DESIGN](components/backend-verilog/DESIGN.md) | [IMPL](components/backend-verilog/IMPLEMENTATION.md) |
| cli | `flow build\|run\|dump-ir\|test`, `render` | planned (M5) | [DESIGN](components/cli/DESIGN.md) | [IMPL](components/cli/IMPLEMENTATION.md) |

Status detail: [`docs/STATUS.md`](STATUS.md) (global roll-up, HANDOFF §7.1.1).

## 4. Placement (only where runsAt is a relation, §4.2)

| `Trn` / `Dat` | Placements | Why it matters |
| --- | --- | --- |
| `validate` | debug-assert after seal AND the property-test harness — never a mandatory production pass | the oracle's independence is the point; "seal Ok ⇒ validate empty" is a *tested* law, not a call chain |
| running a Flow program | `interp` (built) AND each backend target (planned) | one meaning, many realisations — the differential tests are the commuting squares; the interp fibre is authoritative (HANDOFF §7.3) |
| `Print{newline}` effect | interp's writer-style token now; each backend's runtime later | E2: effect order is semantics — every placement must observe the same token order |

## 5. Coherence checklist (§4.5 / §8) against the implementation

- [x] 1. Placement honesty — single process; every pass consumes exactly the value the
      previous returns; interp reads only its `SecondaryMap` env keyed by `ObjectId`.
      No data teleport.
- [x] 2. Transmission well-typing — vacuous (no `Trm` exists). Becomes load-bearing at
      the backend seam; the E1 done-protocol is its hardest instance.
- [x] 3. Placement totality — every built `Trn` has a crate home (§3 table); no
      floating pass.
- [x] 4. Dependency mediation — all cross-crate reach is same-`Loc` Cargo edges;
      `flow-ir` stays the zero-dep hub (the D8 `SourceLoc` copy at one declared seam,
      `flow_lower::tys::ir_loc`, exists precisely to keep it so).
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
