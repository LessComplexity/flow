# Plan: ADR-0027 capture semantics — implementation

Status: plan (model-first, HANDOFF §7.1.5) · Written: 2026-07-21 · S18 · Authority: [`decisions/ADR-0027-capture-semantics.md`](../../decisions/ADR-0027-capture-semantics.md) (ratified by Sapir 2026-07-21: read-only captures as broadcast edges / hidden body-fn params; legality as a graph property; L1108 narrowed with a teaching diagnostic).
Goal: the one-kernel GEMM (and stencil/broadcast forms) become writable and compilable through the whole pipeline, oracle-equal everywhere. The acceptance example: the S16 matmul rewritten in the natural map+fold form (`docs/notes/bench-matmul.md` finding #3), differential-green on both backends.

## 1. The IR delta (the one structural decision)

`Map`/`Fold` gain a capture count: `Map { body, captures: u32 }`, `Fold { body, captures: u32 }` (default 0 — every existing program unchanged).

- **Source shape:** `Map{k}`'s source is a product whose **last** component is the array `[A;n]` and whose **first k** components are the captured values (for k=0: the bare array, today's shape — no normalization churn). `Fold{k}`'s source is `(c₁…cₖ, acc, [A;n])` (k=0: today's `(acc, [A;n])`).
- **Body input shape:** `Map{k}`'s body fn input is `(c₁…cₖ, elem)`; `Fold{k}`'s is `(c₁…cₖ, acc, elem)` — captures first, then the original components in their current order (visible params keep their positions at the end).
- **Zip/Enumerate/Index/Update:** no body fn — no captures, no change.
- **Why an op field (not type-directed):** the split between captures and the mapped array must be *explicit in the graph* — a map over `[（A,B);n]` (element is a pair) must be unambiguous from a map with two captures over `[A;n]`. Implicit/type-inference readings were rejected: the graph says what it means (the ADR's whole spirit).
- **Capture edges are real edges:** the captured objects feed the Map/Fold source product via ordinary `Pair` morphisms — visible to DCE/CSE/region analysis and to the D2b legality check. `Str`/`IoToken`/`Unit` captures are rejected at lowering (bodies are token-free, L1605; an erased capture is meaningless).

## 2. Legality enforcement (ADR-0027 D2b, as built)

- **Surface (lower):** free-variable analysis of body blocks (identifiers resolving to enclosing bindings, excluding the body's own params/locals). A captured **name may not be a rebind target** inside the body — the narrowed **L1108**, with the teaching message: name the variable, state the rule (body instances run per-element, in parallel; a shared read must be a capture, never a write), show the legal form, and print the offending path (the read edge + the write/rebind site).
- **Graph (by construction):** fanout-self-dependence (a capture that depends on its own fanout's output) is unconstructible in the sealed dataflow graph — the capture is demanded *before* the fanout exists in dependency order; the only cycles the IR admits are loop back edges, and a loop-carried capture is the legal read-at-position case (the body reads the current iteration's value, exactly as if passed explicitly — ADR's semantics note).
- **check (flow-check):** no change — the E2 walk keys on token signature; capturing fns stay token-free.

## 3. Per-component work

1. **flow-ir:** `Operation::{Map, Fold}` gain `captures: u32`; typing rules updated (the source/body-input shapes above); builder constructors `map_captured`/`fold_captured` (+ keep `map`/`fold` as the k=0 delegations); independent validate arms (source product arity = k+1 / k+2, last-is-array, body input = captures+original, element/product consistency); Mermaid labels show `+k caps` (Q5 — capture set visible in dumps); snapshot updates.
2. **flow-lower (the core):** free-variable analysis (collect unresolved identifiers in body blocks → enclosing bindings, source-order, dedup); reject rebind-of-captured-name (narrowed L1108 + new message, P-code-quality); emit the capture `Pair` into the op source; the body's locals for captured names map to input projections at capture indices; the body's fn kind stays MapBody/FoldBody (purity machinery unchanged); `Str`/token captures rejected.
3. **flow-interp (the oracle — normative):** `Map{k}`/`Fold{k}` evaluation passes `(captures…, elem)` / `(captures…, acc, elem)` to the body — captured values read from the source product's first k components, read-at-position (a loop-carried capture reads the current iteration's value). Determinism unchanged.
4. **flow-rewrite:** capture-aware handling of the new field everywhere ops are inspected/constructed; **fusion scoped conservative**: Map∘Map fusion only when both maps' capture objects are *identical* (same ObjectIds, same order) — the common chained-map case; union re-threading of differing capture sets is recorded headroom. DCE/CSE see capture edges naturally (they're ordinary Pair edges). R1 property battery extended with capture programs.
5. **flow-backend-llvm:** map/fold sites pass the capture components as leading args to the body call (host side — plain values/allocas); the op-table cells for Map/Fold read the k field. No other change (host-only bodies thread args like any call).
6. **flow-backend-cuda:** the map/fold **kernels** take captured buffers/scalars as extra parameters (positionally, before the trap pointer); `body_call` prepends them; twin signatures already support array params. The F3 cell check and qualifier analysis treat capture edges as ordinary dataflow.
7. **testgen (flow-rewrite/tests):** generate capturing bodies (map/fold over enclosing scalars AND arrays, incl. a loop-carried capture read-at-position case); trap-free by construction discipline preserved.
8. **Docs (Spec impact, ADR):** user-guide §5 (bodies may read enclosing bindings; the mutation rule); `flow-as-implemented.md` (the capture rule + the one-kernel matmul as the canonical example); L-code catalogue (L1108 narrowed); ERRATA/living-corrections entry (LC-6?); backend-cuda STATUS (the F3 note mentions capture edges are ordinary dataflow).

## 4. Tests (per crate, plus the end-to-end proof)

- **ir:** typing-table goldens for the new shapes (k=0 unchanged; k≥1 source/body-input rules); validate rejections (capture component count mismatch, non-product source with k>0, token capture); builder round-trips.
- **lower:** free-var collection (order/dedup); a capturing map + capturing fold lower with correct source products + body inputs; rebind-of-capture → the new L1108 (message asserts variable name + legal form); token/Str capture rejected; loop-carried capture (the matmul `cell` shape re-expressed) lowers.
- **interp (oracle-normative):** capture value contracts — a map with a scalar capture (scale), a map with an array capture (the matmul body), a fold with captures, the loop-carried capture case; determinism; the one-kernel matmul oracle-pinned vs the S16 reference values (N=4: `-275\n3748`).
- **rewrite:** fusion identical-captures fuses; differing-captures does NOT (pinned); R1 battery green over capture programs.
- **backends (both):** golden emission for the capturing map/fold (llvm `.ll`, cuda `.cu` — the map kernel's extra params + readback-once); the one-kernel matmul `.cu` is ONE elementwise kernel with an inner per-thread k-loop (the S16 acceptance shape).
- **differential (both):** the capture testgen sweep raw+rewritten; the one-kernel matmul on both backends vs interp (N=4/16 local; the GPU re-run is the region-v2 session's job, recorded as the follow-up).

## 5. Sequencing

ir → lower (the bulk) → interp (oracle green first — the arbiter) → llvm ∥ cuda → rewrite → testgen + differential → docs reconcile. Per the build flow: TDD per component, fmt+clippy gates, workspace green at every step. No subagents this session (quota) — sequenced, small, verified commits of work.

## 6. Risks / notes

- **The op-field ripple** (every consumer matching on `Map{body}` must handle the new field) — mechanical but wide; compiler finds them all.
- **Fusion conservatism** (identical capture sets only) is a recorded limitation, not a correctness question — the union re-threading is provably sound but deferred.
- **Loop-carried capture + Update:** capturing an array that's Updated later in the same iteration — legal (read-at-position: the body reads the pre-Update value at the map's topo position; the graph orders it). Pinned by an interp contract.
- **Q5 dumps:** Mermaid + debug prints show captures — keeps the hidden-parameter mechanism honest (Sapir's Q5 resolution).
