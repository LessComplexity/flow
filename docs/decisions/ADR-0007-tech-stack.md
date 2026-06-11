# ADR-0007: Compiler technical stack — Rust, handwritten front end, arena IR, interpreter oracle

Date: 2026-06-11 · Status: accepted

## Context (what forced the decision; spec refs)

Bootstrap must fix the compiler's technical foundations before any component is built, so
that ten stateless sessions across syntax, IR, lowering, checking, interpretation, rewriting,
and four backends share one set of choices instead of relitigating them. HANDOFF §5 records
eight pre-made decisions; the `category-ir.md` §3 IR invariants, §4 lowering rules, and §9
optimization taxonomy constrain several of them. This ADR encodes all eight as the binding
stack; deviating from any of them requires a superseding ADR.

## Decision (one paragraph, imperative)

(1) Implement the compiler in **Rust** — the spec pseudocode is already Rust-shaped.
(2) Write a **handwritten lexer plus recursive-descent parser**, carrying `SourceLoc` spans
from day one, because the guard/flow syntax is unusual enough that parser generators fight it.
(3) Build the IR as an **arena/slotmap-backed graph** exactly per `category-ir.md` §3 (single
source and target per morphism; multi-arg ops lower Pair-then-primitive; `Phi` first-class;
loops via `Trace` + `LoopMerge`; back-edges as real adjacency edges visible to Tarjan SCC),
with **all invariants enforced in the builder API** so an ill-formed graph is unconstructible
through the public interface. (4) Make the **fueled reference interpreter on the IR the
oracle**, built before any backend; every rewrite and backend is judged against it, and loop
evaluation carries fuel (E1). (5) Have **backends emit source text** — textual LLVM `.ll`
piped to `clang`, CUDA `.cu` via `nvcc`, Verilog `.v` simulated with Verilator (fallback
Icarus) — with **skip-with-reason** tests when a toolchain is absent (never a faked pass).
(6) Use the **testing stack** `cargo test` + `insta` snapshots + `proptest` properties +
differential backend-vs-interpreter tests + `criterion` benches, with Mermaid graph dumps
lint-checked (quote special-char labels; no mixed arrow styles). (7) Organize the **rewrite
engine by the four-layer taxonomy** (`category-ir.md` §9), implementing layers 3/4
(constant folding, DCE, CSE) first, then layer 1 (map fusion), then layer 2 (naturality),
one source directory per layer. (8) Adopt a **verification posture** of property-based
differential testing now, reserving mechanization (Lean/Coq) solely for the E1
trace-preservation theorem, only at write-up time.

## Consequences (tradeoffs, implementation impact)

- Handwritten parsing costs more up-front lines than a generator but yields the precise,
  spanned diagnostics Flow's guard/flow syntax needs; this is a deliberate trade.
- Builder-enforced invariants mean the IR has no "validate later" pass — illegality is a
  compile error in the builder's own API, which is the cheapest layer of the testing strategy.
- Emitting source text (not FFI/LLVM bindings) keeps backends inspectable and golden-testable
  and lets absent toolchains degrade to documented skips rather than build failures.
- Building the oracle first means no backend or rewrite may claim correctness without a
  differential/property test against it; the interpreter is expected to surface the next spec
  bugs faster than further reading.
- The layered rewrite ordering front-loads the laws that need no categorical justification
  (layers 3/4) and defers naturality (layer 2), matching difficulty to schedule.

## Spec impact (exact files/sections to patch; patched? n/a)

These are implementation-tooling decisions, not corrections to the v0.2 corpus; they cite
`category-ir.md` §3/§4/§9 but change none of it. The binding record is HANDOFF §5. patched? n/a.
