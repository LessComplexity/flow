# Flow — Development Handoff

**Version:** 1.0 · **Date:** 2026-06-11 · **Status:** Canonical bootstrap document for compiler implementation.

This document is the single entry point for developing the Flow compiler. Every development session — human or AI — starts here (or at `docs/next-session.md` once it exists). It consolidates the v0.2 specification corpus, records the accepted errata and pre-made technical decisions, defines the implementation subset (Flow-Core), and specifies the documentation-driven workflow that every session must follow.

---

## 0. How to use this document

- **First session ever:** read this file top to bottom, then execute §10 (Bootstrap).
- **Every subsequent session:** read `docs/next-session.md` → `docs/STATUS.md` → the `STATUS.md` of the component(s) you will touch → the spec sections those files reference. Then follow the session protocol in §7.2.
- **Conflict resolution:** if any document contradicts another, the authority order in §2.2 decides. Never silently resolve a conflict — record it (errata or ADR).

---

## 1. Project summary

Flow is a general-purpose dataflow language whose surface syntax directly denotes the compiler's graph IR: `data -> f -> g -> ret;` _is_ three nodes and two edges. The IR has a category-theoretic semantics (**Flow-Cat**): graphs denote morphisms, optimizations are justified by categorical laws (functor laws, naturality, algebraic equations, graph rewrites), and each backend (LLVM/CPU, CUDA/GPU, Verilog/FPGA, WASM) is modeled as a functor out of Flow-Cat. Parallelism is structural and default; `seq` opts into ordering. Memory is reclaimed at the graph's last-use frontier — no GC, no ownership annotations.

**The thesis artifact (Milestone M5):** one source file (`examples/sepia.flow`), demonstrably the same program, running correctly on CPU, GPU, and FPGA simulation — with the compiler's correctness argument being functoriality, and the dataflow graph rendered alongside. Everything in this handoff serves that demo. Scope beyond it is deferred by default.

**Strategic frame (decided previously, restated here):** validate in one domain — real-time image/signal processing — before generalizing. A 2-year checkpoint evaluates viability. The single biggest project risk is another year of specification without an implementation; the spec is frozen at v0.2 + errata, and all further design happens as ADRs driven by implementation feedback.

---

## 2. Document map

### 2.1 The corpus

All spec files live in `docs/spec/` (copied there during bootstrap, §10).

| File                                  | Version   | Role                                                                                                                                                                                                                                                         |
| ------------------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `docs/spec/category-ir.md`            | v0.2      | **Primary formal spec.** Flow-Cat definition (§2), IR data structures (§3), lowering rules (§4), graph representation (§5), functors (§6), natural transformations (§7), backends-as-functors (§8), optimization framework (§9), implementation guide (§10). |
| `docs/spec/user-guide.md`             | v0.2      | **Primary language reference.** Full syntax, conditionals/guards, loops, parallelism model (§5), memory model (§6), error handling (§7), complete examples (§8).                                                                                             |
| `docs/spec/architecture.md`           | v0.2      | Compiler pipeline, component responsibilities, backend architectures, tooling, runtime.                                                                                                                                                                      |
| `docs/spec/getting-started.md`        | v0.2      | 10-minute intro. Useful as the "what a new user sees" test surface.                                                                                                                                                                                          |
| `docs/spec/CHANGES.md`                | v0.1→v0.2 | Decision log for the v0.2 revision. **Read §1 (structural fixes) before touching the IR** — it explains _why_ the invariants exist (single-source morphisms, first-class Phi, loops as trace, honest coproducts for effects).                                |
| `docs/spec/flow-language-design.docx` | v0.1-era  | Original design document (philosophy, goals, rationale). Historical authority only; superseded where it conflicts with v0.2 files.                                                                                                                           |
| `FRAMEWORK.md`                        | current   | Categorical modeling method for compiler-internal (Level B) design; coherence checklist (§8); session reconcile gate (ADR-0014). Methodology only — does not touch the frozen Level A spec.                                                                   |
| `HANDOFF.md`                          | 1.0       | This file.                                                                                                                                                                                                                                                   |

### 2.2 Authority order (highest wins)

1. Accepted ADRs in `docs/decisions/` (including the bootstrap ADRs encoding errata E1–E5, §3)
   - `FRAMEWORK.md` — for all compiler-internal (Level B) modeling and doc-reconcile gate questions; defers to accepted ADRs on any spec-touching question.
2. `category-ir.md` v0.2 (formal semantics)
3. `user-guide.md` v0.2 and `architecture.md` v0.2 (tie: user-guide for language behavior, architecture for compiler structure)
4. `getting-started.md` v0.2
5. `CHANGES.md` (rationale, not normative)
6. `flow-language-design.docx` (historical)

---

## 3. Known defects and accepted errata (E1–E5)

These were found in a formal review of the v0.2 corpus. They are **pre-made decisions**: bootstrap (§10) turns each into an ADR (status `accepted`, revisable by Sapir) and patches the spec files. No implementation work may contradict them without a superseding ADR.

### E1 — Flow-Cat cannot be both "total" and traced-cartesian

**Defect.** `category-ir.md` §2.1 defines morphisms as pure **total** functions; §2.8 claims Flow-Cat is traced with monoidal product = categorical product. Jointly inconsistent: a traced cartesian category is equivalent to one with a Conway fixed-point operator (Hasegawa 1997), and total functions lack fixpoints in general (`not : Bool → Bool` has none). Unbounded loops are exactly where partiality enters (`loop { -> loop; }` is legal and diverges).

**Fix.**

- Loops/iteration live in the Kleisli category of the partiality (divergence) monad — the same §2.6 machinery already used for I/O and errors. The total core of Flow-Cat has no trace; the traced structure exists on the partial extension (least-fixpoint / Elgot-iteration semantics).
- `category-ir.md` §8.3 must be rewritten: Clocked-Cat's trace is **guarded** (register = unit delay ⇒ always productive ⇒ total; Mealy-machine semantics). `F_Verilog` therefore maps an _iteration_ trace to a _guarded_ trace — different traced structures. "F commutes with Tr" is not free; it is a theorem with content, mediated by a **done-signal protocol**: _the iteration terminates in n steps with value v ⟺ the circuit asserts `done` at cycle n with output v_. This theorem is the project's most publishable single result; state it precisely, discharge it informally now, mechanize later.

**Implementation impact.** Interpreter loop semantics are partial: all loop evaluation carries a fuel/step-limit in tests; divergence is a defined outcome, not a hang. The Verilog FSM for any lowered loop implements the done protocol (`valid_in / busy / done / result` handshake).

### E2 — Parallel effects rule (replaces "executor decides")

**Defect.** `user-guide.md` §5.4 row 4: "Independent + effectful → Executor decides (may parallelize with non-deterministic order)." This makes program meaning scheduler-dependent, contradicting both "no data races by construction" and the functorial-correctness story (if the denotation is "whatever the executor did," there is nothing for a functor to preserve).

**Fix.** Effectful morphisms are **not permitted in parallel fanout**. Effects either (a) sequence via `seq`, or (b) communicate via channels with **Kahn process network semantics** — blocking reads, unbounded FIFOs — under which determinism independent of scheduling is a theorem (Kahn 1974). The streaming/FPGA subset later adopts synchronous-dataflow restrictions (Lee & Messerschmitt 1987) for static schedules and bounded buffers. Channels are out of Flow-Core scope (§4), but the rule is fixed _now_ so the effect checker is built right the first time.

### E3 — Memory-model guarantee is scoped

**Defect.** "No use-after-free / double-free / leaks / races, with zero annotations" is claimed for the whole language. Whole-program region inference at general-purpose scope (closures, channels, cyclic structures) is historically treacherous (cf. Tofte–Talpin region pathologies); cycles already punt to refcounting.

**Fix.** State the guarantee as **proven for the first-order, non-cyclic dataflow core** (which contains Flow-Core entirely) and **open for the full language**. `user-guide.md` §6.5 is amended accordingly. Implementation benefit: the Flow-Core lifetime engine is simple (stack/static allocation for fixed-size data; last-use frontier for arrays) and can be exactly right.

### E4 — Operator-precedence example is self-contradictory

**Defect.** `user-guide.md` §3.6 table places `->` looser than `+`, but the example claims `a -> b + c -> d` parses as `(a -> b) + (c -> d)` — the impossible parse, and one that presupposes a flow has a value as an operand.

**Fix.** Per the table, `a -> b + c -> d` ≡ `a -> (b + c) -> d`. Additionally (parser-level decision, recorded in the same ADR): **a flow is a statement, not a value-producing expression**; `->`/`<-` chains are parsed at statement level. The example is corrected; an explanatory line is added.

### E5 — Rename surface keyword `category` → `type`

**Defect.** The keyword collision (surface `category` = type vs. ambient category-theoretic "category") actively confuses the spec's own exposition (flagged in `category-ir.md` Appendix A and `CHANGES.md` §8).

**Fix (accepted, pending Sapir's veto at bootstrap).** Rename now, while zero code exists — the last free moment. Affects `user-guide.md`, `getting-started.md`, all examples, the docx (deferred), and the `Ty` naming in the IR (already neutral). Keyword `category` may be reserved-and-rejected with a helpful error.

---

## 4. Implementation scope: Flow-Core (v0.3 subset)

Flow-Core is the frozen subset the compiler implements through M5. Anything outside it is **rejected with a clear diagnostic**, not silently accepted. Scope changes require an ADR.

### 4.1 In scope

- **Types:** `i32 i64 u8 f32 f64 bool`; tuples `(A, B, …)`; named product types (`type Point { x: f32, y: f32 }` after E5); fixed-size arrays `[T; N]`. String **literals** allowed only as arguments to `print`.
- **Expressions/ops:** arithmetic `+ - * / %`, comparisons, `&& || !`, member access, tuple/struct construction, array indexing (bounds-checked), literals.
- **Flows:** `->` / `<-` statements; pipelines with operator shorthand (`data * 2 -> + 5 -> ret;`); explicit intermediates.
- **Functions:** `fn name(args) -> Ret { … }`, tuple-input calls `(a, b) -> f -> r;`, `ret` target. Call graph must be **acyclic** in Core (no recursion).
- **Conditionals:** guard blocks with `-true->/-false->`, integer-literal guards, `-_->` default. **Pure branches only** in Core → Phi lowering (`category-ir.md` §4.4 / CHANGES §1.5).
- **Loops:** labeled `loop { … -> loop; … -> ret; }` with scalar/tuple carried state, lowered to the trace construction of `category-ir.md` §4.5 under E1 semantics. `mut` permitted for loop-carried variables (lowered to trace state) and simple accumulation.
- **Parallel fanout** `x -> { -> a; -> b; }` for **pure** branches; implicit join. `seq { … }` for ordering.
- **Effects:** `print` only, modeled in Kleisli(IO); `print` is legal only in sequential context (E2).
- **Collections:** `map` and `fold` over fixed arrays with an **inline block** body (the block is not a first-class value).

### 4.2 Out of scope (Core+1 and later — reject with diagnostics)

Dynamic arrays/slices; strings as data; coproduct/enum types, `Option/Result`, `Some(x)` patterns, and `?` (Core+1: first feature after M2, since coproducts are central to the categorical story); recursion (Core+1, CPU backends only); closures as values; channels (post-M5, KPN per E2); `executor` definitions; hardware annotations `@…`; modules/`use` (single file per program); `category`/`type` declarations beyond product types.

### 4.3 Backend capability matrix (maintained in global STATUS.md)

Each feature row × backend column carries one of `supported / rejected-with-error / planned`. Initial known restriction: Verilog backend supports only feedforward pipelines + single-loop FSMs (with E1 done protocol); it must cleanly reject everything else.

---

## 5. Pre-made technical decisions

Recorded as bootstrap ADRs; summarized here.

1. **Implementation language: Rust.** Spec pseudocode is already Rust-shaped.
2. **Handwritten lexer + recursive-descent parser.** The guard/flow syntax is unusual enough that generators will fight it. Error spans (`SourceLoc`) from day one.
3. **IR:** arena/slotmap-backed graph with the ADR-0013 delta from `category-ir.md` §3 (v0.2 invariants retained: every morphism has exactly one source and one target object; multi-arg ops lower as Pair-then-primitive; `Phi` first-class; back-edges are real adjacency edges visible to Tarjan SCC). All dataflow is adjacency edges; loops are inline cycles — a `LoopMerge` object receives `LoopEnter` + ≥1 `LoopBack`, and `LoopExit` edges leave the cycle; `Operation::Trace` is not materialized — the trace is the cycle. Full operation set: the Core subset of §3.3 plus `Neg`, `Index`, `Map{body}`, `Fold{body}`, `Print`, `LoopEnter`, `LoopBack`, `LoopExit`, `Output`; minus `Identity`, `Const`, `Trace`, and out-of-Core variants. **Invariants are enforced in the IR builder API** — it must be impossible to construct an ill-formed graph through the public interface. See ADR-0013.
4. **Reference interpreter on the IR is the oracle.** Built before any backend. Every rewrite and every backend is judged against it. Loop evaluation is fueled (E1).
5. **Backends emit source text:** textual LLVM IR (`.ll`) piped to `clang` (no FFI bindings initially); CUDA `.cu` via `nvcc` when present; Verilog `.v` simulated with **Verilator** (fallback Icarus). Toolchain absence ⇒ tests skip-with-reason, recorded in STATUS.
6. **Testing stack:** `cargo test`; golden/snapshot tests (`insta`) for parse trees, IR dumps, emitted code; property tests (`proptest`) for rewrite soundness; **differential tests** backend-vs-interpreter on random inputs; `criterion` benchmarks. Graph dumps render to Mermaid and are lint-checked (quote labels containing `'` or special chars; no mixed arrow styles — both were past failure modes).
7. **Rewrite engine** organized by the four-layer taxonomy (`category-ir.md` §9): layer-3/4 first (constant folding, DCE, CSE), then layer-1 (map fusion via functor laws), layer-2 (naturality) last. One source directory per layer, mirroring the spec.
8. **Verification posture:** property-based differential testing now; mechanization (Lean/Coq) only for the E1 trace-preservation theorem, only when writing it up.

---

## 6. Repository layout

```
flow/
├── HANDOFF.md                      # this file
├── Cargo.toml                      # workspace
├── docs/
│   ├── STATUS.md                   # GLOBAL status (template §7.1.1)
│   ├── next-session.md             # written at end of EVERY session (template §7.1.4)
│   ├── spec/                       # the corpus (§2.1) + errata patches
│   │   └── ERRATA.md               # E1–E5 text + any later spec corrections
│   ├── decisions/                  # ADRs (template §7.1.3)
│   │   ├── ADR-0001…ADR-0007 (bootstrap)
│   │   └── ADR-NNNN-slug.md (added each session as needed)
│   ├── architecture/               # FRAMEWORK model index (ADR-0014)
│   │   ├── INDEX.md
│   │   └── categorical-model.md
│   └── components/<name>/          # one folder per component
│       ├── STATUS.md               # development status (template §7.1.2)
│       └── DESIGN.md               # living design doc, written BEFORE code
├── crates/
│   ├── flow-syntax/                # lexer, parser, parse tree, diagnostics
│   ├── flow-ir/                    # objects/morphisms/compositions, builder, invariants, Mermaid dump
│   ├── flow-lower/                 # parse tree → IR (category-ir §4 rules)
│   ├── flow-check/                 # type check, effect check (E2), lifetime analysis (E3 scope)
│   ├── flow-interp/                # the oracle (fueled)
│   ├── flow-rewrite/               # layers 1–4 passes + property-test harness
│   ├── flow-backend-llvm/
│   ├── flow-backend-cuda/
│   ├── flow-backend-verilog/       # + Verilator harness, done-protocol (E1)
│   └── flow-cli/                   # `flow build|run|dump-ir|test`
├── editors/                        # editor/tooling support (ADR-0008)
├── examples/                       # sepia.flow, fir.flow, abs.flow, sum_to_n.flow, pipeline.flow, fanout.flow
└── tests/
    ├── golden/                     # .flow → expected interpreter output / IR snapshots
    └── differential/               # backend == interpreter harnesses
```

**Component list** (each gets `docs/components/<name>/`): `syntax`, `ir`, `lower`, `check`, `interp`, `rewrite`, `backend-llvm`, `backend-cuda`, `backend-verilog`, `cli`.

---

## 7. The AI-ready development workflow

The repository is operated through `docs/`. The code is the product; `docs/` is the shared memory that makes stateless sessions cumulative. **A session that wrote code but not docs did not happen.**

### 7.1 The docs/ system

#### 7.1.1 Global `docs/STATUS.md` — template

```markdown
# Flow — Global Status

Last updated: YYYY-MM-DD · Session NN
Current phase: P<k> — <name> Current milestone: M<k> — <one-line definition of done>

## Components

| Component | Status   | Tests        | One-line state                 | Docs                                  |
| --------- | -------- | ------------ | ------------------------------ | ------------------------------------- |
| syntax    | building | 34 ✅ / 2 ⏭ | Guards parse; loop labels TODO | [status](components/syntax/STATUS.md) |

| ...

Status vocabulary: not-started · design · building · tested · stable · blocked

## Backend capability matrix

| Feature   | interp | llvm | cuda    | verilog |
| --------- | ------ | ---- | ------- | ------- |
| pipelines | ✅     | ✅   | planned | planned |

| ... (✅ supported · ✋ rejected-with-error · planned)

## Blockers

## Errata/ADR ledger

| ID | Title | Status | Applied to spec? |

## Session log (newest first)

| NN | date | focus | outcome |
```

#### 7.1.2 Per-component `docs/components/<name>/STATUS.md` — template

```markdown
# Component: <name>

Status: not-started | design | building | tested | stable | blocked
Last updated: YYYY-MM-DD · Session NN
Spec references: <files + section numbers this component implements>
Depends on: <components> Depended on by: <components>

## What works

## What does not / known issues

## Invariants enforced (and where in code)

## Test coverage (golden / property / differential / skipped+why)

## Performance notes (numbers + bench name + date; regressions flagged)

## Open questions (→ ADR candidates)
```

#### 7.1.3 ADRs `docs/decisions/ADR-NNNN-slug.md` — template

```markdown
# ADR-NNNN: <title>

Date: · Status: proposed | accepted | superseded-by-NNNN

## Context (what forced the decision; spec refs)

## Decision (one paragraph, imperative)

## Consequences (tradeoffs, implementation impact)

## Spec impact (exact files/sections to patch; patched? yes/no)
```

#### 7.1.4 `docs/next-session.md` — template (overwritten every session)

```markdown
# Next Session

Written: YYYY-MM-DD · end of Session NN · by: <agent/human>

## Where things stand (≤5 lines)

## Test state: ALL GREEN | RED (exact failing tests + suspected cause)

## Do next (ordered, smallest-first)

1. ...

## Open questions for Sapir

## Gotchas / warnings (things that will waste the next session's time)

## Commands (build/test/bench invocations that currently work)
```

#### 7.1.5 `DESIGN.md`

Living design document per component, written/updated **before** code in every session that touches the component: data structures, public API, algorithms, error behavior, how invariants are enforced, what the tests will assert. Spec deviations discovered while designing go to an ADR first. Every component `DESIGN.md` **MUST** lead with a `## Categorical model (Dat + Trn)` section (FRAMEWORK §2; ADR-0014) — objects and morphisms before tables and API — and is listed in the model index `docs/architecture/INDEX.md`.

### 7.2 The session protocol — mandatory, in order

1. **Read.** `docs/next-session.md` → global `STATUS.md` → component `STATUS.md` + `DESIGN.md` for today's target(s) → the spec sections those reference (use §2.2 authority order). Do not write code before this step.
2. **Design.** Create/update the component `DESIGN.md` for today's increment. If design deviates from spec: write an ADR (proposed), get it accepted (or flag for Sapir in next-session.md), patch `docs/spec/ERRATA.md` if needed. Spec is law until an ADR changes it.
3. **Code.** Implement against `DESIGN.md`. Small commits, conventional messages (`syntax: parse loop labels`). IR invariants stay enforced in builder APIs, never by convention.
4. **Test.** Write tests _with_ the code: golden for syntax/IR/emission, property for rewrites, differential for backends (oracle = interpreter). Run the full suite, not just new tests.
5. **Fix.** Iterate to green. If green is not reachable this session, stop coding early enough to document the red state precisely (step 8) — a documented red beats an undocumented "almost".
6. **Perf / profile.** Once the component is functional: run `criterion` benches (and profiler when investigating); record numbers + date in the component STATUS. Optimize only with profile evidence; never trade away an invariant for speed without an ADR.
7. **Reconcile docs.** Diff actual behavior against spec sections. Update: component `STATUS.md` (always), global `STATUS.md` (component table, capability matrix, session log), `ERRATA.md`/ADRs (if the implementation found a spec bug — this is expected and good; v0.2 itself came from five such finds). The implementation never silently diverges from the spec.
   - Verify the change against the FRAMEWORK §8 coherence/reduction checklist, and update the component's `## Categorical model (Dat + Trn)` section + morphism table (and `docs/architecture/INDEX.md`) in the same change (FRAMEWORK §6; ADR-0014).
8. **Hand off.** Overwrite `docs/next-session.md` (template §7.1.4). Commit everything. **Every session ends with this file — especially failed sessions.**

### 7.3 Hard rules for AI sessions

- **The interpreter is the oracle.** No backend or rewrite correctness claim without differential/property tests against it.
- **One component focus per session** where possible; cross-cutting changes name every touched component in next-session.md.
- **No scope creep:** anything outside Flow-Core (§4) is rejected by the compiler and out of bounds for sessions, absent an ADR.
- **Determinism of meaning is sacred:** nothing observable may depend on scheduling (E2). If a test is flaky, that is a semantics bug until proven otherwise.
- **All loop evaluation in tests is fueled** (E1). A hanging test process is a protocol violation, not bad luck.
- **Graph dumps must render:** Mermaid output is lint-checked in tests (quoting, arrow styles).
- **Toolchain absence degrades gracefully:** skip-with-reason, recorded in STATUS — never fake a pass.
- **Don't trust memory over docs:** if recollection of the project conflicts with `docs/`, `docs/` wins; if `docs/` conflicts internally, §2.2 wins and the conflict gets logged.

---

## 8. Roadmap and milestones

| Phase                       | Component focus | Definition of done                                                                                                                                                       |
| --------------------------- | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **P0 Bootstrap (M0)**       | repo, docs      | §10 executed: scaffold + docs system live; spec copied; ERRATA.md + ADR-0001…0007 written (bootstrap ADRs; subsequent sessions added ADR-0008…0014, see `docs/decisions/`); E1–E5 patches applied to spec files; empty workspace `cargo test` green.      |
| **P1 Frontend**             | syntax          | Lexer+parser for all Flow-Core constructs; golden parse-tree tests incl. every `examples/*.flow`; spanned diagnostics; E4 statement-rule implemented.                    |
| **P2 IR + lowering**        | ir, lower       | §3/§4 structures with builder-enforced invariants; lowering golden tests; property test "no ill-formed graph constructible"; Mermaid dump of any program.                |
| **P3 Interpreter (M1)**     | interp, check   | Type/effect/lifetime checks for Core; fueled evaluator; **`sepia.flow` (and abs, sum_to_n, pipeline, fanout) run correctly on CPU via interpreter.** Oracle established. |
| **P4 Rewrites**             | rewrite         | Constant folding, DCE, CSE (layers 3–4) + map fusion (layer 1); every pass property-tested: random Core program × random inputs → interpreter-equal before/after.        |
| **P5 LLVM backend (M2)**    | backend-llvm    | `.ll` → clang → native; differential green on all examples + random programs; first perf baseline recorded (sepia at N×N).                                               |
| **P6 CUDA backend (M3)**    | backend-cuda    | `.cu` for map-kernels; compiles under nvcc; differential where GPU available, else documented skip; kernel-fusion = source-level map-fusion preserved (spec §8.2).       |
| **P7 Verilog backend (M4)** | backend-verilog | Feedforward pipelines + single-loop FSM with E1 done-protocol; **Verilator simulation of sepia matches the interpreter bit-for-bit**; capability matrix enforced.        |
| **M5 Tri-target demo**      | cli, all        | One `sepia.flow`; `flow build --target {cpu,cuda,verilog}` + `flow dump-ir --mermaid`; demo README showing identical outputs on three targets and the rendered graph.    |

**After M5 (parked — do not wander here):** coproducts/`Option`/`?` (Core+1), recursion (CPU), channels with KPN semantics + SDF streaming for FPGA, strings, modules, error-handling design, the E1 theorem write-up, domain validation push (real-time image processing), `flow-language-design.docx` regeneration. The 2-year checkpoint judges the project on M5 + domain traction.

---

## 9. Testing & verification strategy (summary)

Layered, cheapest-first: **(1) builder-enforced IR invariants** (ill-formed graphs unconstructible) → **(2) golden/snapshot tests** for syntax, IR, emitted code → **(3) property tests** for every rewrite (semantics-preservation vs oracle) → **(4) differential tests** for every backend (oracle equality on examples + randomized Core programs) → **(5) perf benchmarks** with recorded baselines and flagged regressions → **(6) mechanization** reserved for the E1 trace-preservation theorem at write-up time. Random-program generation for (3)/(4) lives in `flow-rewrite`'s test harness and grows with Flow-Core only.

---

## 10. Bootstrap — first session instructions (M0)

1. Create the repo skeleton of §6 (empty crates compiling; `cargo test` green).
2. Copy the six corpus files into `docs/spec/` (convert nothing; the docx stays a docx, referenced not edited).
3. Write `docs/spec/ERRATA.md` containing E1–E5 verbatim from §3, then **apply the textual patches** to `category-ir.md` (§2.1/2.7/2.8/8.3) and `user-guide.md` (§3.6, §5.4, §6.5) — marked with `> **Erratum E<k> applied — see docs/spec/ERRATA.md and ADR-000<k+1>.**`
4. Write ADR-0001…0007 (status: accepted; E5's ADR-0006 flagged "pending Sapir veto" in next-session.md). Perform the E5 rename across `user-guide.md`, `getting-started.md`, and `examples/` if not vetoed.
5. Instantiate `docs/STATUS.md`, all ten component folders with `STATUS.md` (status: not-started) + empty `DESIGN.md`, using §7.1 templates.
6. Write `examples/` programs (sepia, fir, abs, sum_to_n, pipeline, fanout) in Flow-Core syntax — these are the acceptance surface for every later phase; they may not yet compile, only exist.
7. Write the first `docs/next-session.md`: "P1 Frontend — start with the lexer; read user-guide §3 + ADR-0005."

---

_This handoff supersedes nothing and freezes everything: the v0.2 corpus + ERRATA is the spec; ADRs are the only mechanism of change; the demo is the goal; the docs/ loop is the method. Build the interpreter — it will find the next five bugs faster than another reading pass._
