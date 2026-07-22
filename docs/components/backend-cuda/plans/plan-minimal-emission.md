# Plan: minimal emission — the S22 mandate's split rule (suggestions #15 + #16, done as structure)

Written: 2026-07-22 · S22 · by: Claude Fable (orchestrator). Status: **DRAFT → dispatching** (Sapir's S22 mandate is the standing directive; each WP differential-gated).
Judged against: `benches/matmul/matmul512_cap.cu` `d_fn3`/`d_fn4` (the before-exhibits).
Sequencing: AFTER rewrite-before-emit (S22 item 1 — CSE kills the duplicate-wrapper class first, e.g. d_fn4's `o6`/`o8`); BEFORE region emission v2 (independent: regions change the unit of emission, this changes the text within any unit).

## 0. The mandate, restated as two laws (Sapir, S21 close, verbatim intent)

1. **One name per value, ever.** A value gets a name only where its chain genuinely
   splits (>1 consumer) — and then ONE name that consumers REFERENCE; never a
   per-consumer re-wrap, never a name-copy.
2. **Chains emit as chains.** Straight-line graph paths compose into expressions;
   no hanger local per edge. The graph doesn't carry named points; the text reads
   as the operations directly.

Cross-backend: the decision is a **Cat-IR-level deduced query** (the
`loop_plan`/`last_use_plan`/`bounds_proof` family), not a cuda patch. cuda consumes
first (the exhibits); llvm assessed after (its SSA names are structural — the
expected llvm delta is small to none; measured, not assumed).

## 1. Categorical model (Dat + Trn)

Why category theory buys anything here: the emitter today realises the functor
`F_CUDA : Flow-Cat → C-text` morphism-by-morphism — every edge becomes a statement,
every object a local. But the functor's obligation is only that the **composite**
agrees with the oracle (R1); the factorisation into statements is an emission
choice. Minimal emission is the observation that the text should mirror the
**consumption structure** of the graph, not its edge count: an object is a *point*
in the text only where the graph itself has a point (fanout, a boundary, an
effect); everywhere else it is a subterm.

Objects (all deduced from the sealed graph; nothing stored):

| Object | Meaning |
| --- | --- |
| `EmissionClass` | `Dissolved \| Inline \| Named` — per owned object per fn |
| `consumers(o)` | the multiset of consuming edges (op Pair-slots, Proj, call arg, Output/Return, loop edges, effect operands, launch operands) |
| `ExprTree(m)` | the maximal pure expression tree rooted at a materialization point, leaves = Named/param/Constant references |

Morphisms (the classification, total over owned objects):

| Morphism | Signature | Rule |
| --- | --- | --- |
| `class` | `Obj → EmissionClass` | below |
| `boundary?` | `Obj → 𝔹` | consumed-whole: call arg · capture product · kernel-launch operand · escape (Output/Return) · effect operand · loop-carried/merge/exit · array construction |
| `guarded?` | `Morphism → 𝔹` | producer needs statement-form guards: unproven `Index`/`Update` (¬`bounds_proof`), `Div`/`Mod` without #13 const-divisor elision |

Classification (in order; first match wins):

1. **Dissolved** — a product object whose consumers are ONLY (a) the Pair-fed
   primitive it argues (the ADR-0013 Pair-then-primitive shape) and/or (b) `Proj`
   edges — and which is not `boundary?`. The struct never materialises: the
   primitive reads its argument expressions directly; each `Proj` forwards the
   field's source expression. (d_fn3's `o8/o10/o12/o14/o16/o18/o20/o22` — all 8.)
2. **Named** — `boundary?`, or `|consumers| > 1`, or producer `guarded?` (guards
   are statements; the guarded result is then a name), or type has no scalar
   C form (arrays stay handle-named).
3. **Inline** — everything else (exactly 1 consumer, pure, guard-free): emitted as
   a subexpression at its consumer.

Composition rules (the invariants the implementation must preserve):

- **R-ONENAME (the same-variable rule — Sapir, S22):** a split value is computed
  once into exactly ONE variable, and every consumer references THAT variable.
  No name-copies (`o0 = in;`), no aliases, no per-consumer re-wraps. What already
  IS a variable is referenced in place: parameters (`in`, `in.f3`), captures, the
  loop-carried name — copying any of them into a fresh local is forbidden, not
  merely discouraged. "Named" in this plan means *owns the single variable*,
  never *renamed*.
- **R-NODUP (no duplication):** `Inline ⇔ |consumers| = 1`, so every operation's
  text appears exactly once. Emission is a tree unfolding of a DAG whose shared
  nodes are all Named — no exponential blowup, module size monotone non-increasing.
- **R-ORDER (effect/trap order):** statement order of Named/guarded/effect points
  is today's topo order restricted to those points; inlining never migrates a
  trapping or effectful op across a statement boundary (guarded? ops are never
  Inline). Oracle trap order preserved by construction.
- **R-TEXT (value forms unchanged):** the wrapping-int cast discipline
  (`(int32_t)((uint32_t)a * (uint32_t)b)`), IEEE float forms, `-fmad=false`, i64
  index conversion — all compose unchanged as nested subexpressions. R1 untouched.
- **R-DET (determinism):** class + tree extraction are pure functions of the
  sealed graph; same IR ⇒ same text (ADR-0020).

## 2. The query (flow-ir)

`flow_ir::emission_plan(f) -> EmissionPlan` with per-object `class`, and
per-statement-point the expression tree in leaf-to-root order. Lives beside
`loop_plan`/`last_use_plan`/`bounds_proof`; same doc/test discipline (§5.1-style
golden rows + proptest: classification total, R-NODUP holds on random testgen
graphs). `guarded?` composes the EXISTING `bounds_proof` + const-divisor facts —
deduce once; the backend must not re-derive.

The query owns `guarded?` entirely — no backend parameter: `bounds_proof` is
already in flow-ir, the const-divisor fact is graph-visible (the divisor Pair-edge
source is a Constant — the same fact `kernel::const_int_operand` reads), and
float-Div/Mod-never-traps is IR semantics (ADR-0013 IN6). A backend that elides
MORE guards than the query assumes only leaves a value conservatively Named —
correct, marginally non-minimal; never the reverse. Home: `algo.rs`, the
`loop_plan`/`last_use_plan`/`bounds_proof` family.

Dissolution and counting compose (the R-NODUP-preserving order): first mark
dissolvable products structurally; then consumer counts for ordinary objects are
taken on the TRANSPARENT graph — a dissolved product's consumers count against
each field's source. `Inline ⇔ effective count = 1`, so dissolving a product
consumed by two primitives correctly leaves shared field chains Named.

**As-built (WP-A, S22) — two review-driven refinements:**
1. **Dissolution is Pair-built-only.** Redistribution resolves fields via
   `pair_slot_source`; a Proj-PRODUCED tuple has none, so dissolving one silently
   dropped its consumers from the counts — a shared computed field chain would
   classify Inline and duplicate textually (R-NODUP break). Rule: a product
   dissolves only if every slot has a Pair source. Regressions:
   `emission_nested_product_dissolution_is_pair_built_only`,
   `emission_proj_produced_tuple_fanout_is_named_not_dropped`.
2. **Product-typed Inline exists** (an inner product consumed once through a
   dissolved outer, or a single-consumer Proj-produced tuple). WP-B must emit it
   as a braced compound literal at its consumer or locally force a name —
   classification stays as deduced; the choice is textual only.

## 3. Consumer changes (cuda)

- `kernel.rs DevEmit` (device twins — the d_fn3/d_fn4 class, the exhibits): drop
  the per-object hoisted decls + per-morphism statements; emit one statement per
  Named point (`T name = <ExprTree>;`), guards as today before their Named result,
  `return <ExprTree>;` for the output chain. R-ONENAME concretely: the parameter
  is already a variable — consumers write `in.fK` in place; the `o0 = in;` /
  `o2 = o0.f0;` prelude class is deleted, not renamed. d_fn3 target shape: ~1 return
  statement; d_fn4: capture unpack + the carried-fold loop with its per-iteration
  Named points only.
- `func.rs FnEmit` (host walk): same split rule for scalar chains between launch
  sites; launches/copies/trap checks/arena/frees are all `boundary?` — the host
  spine stays statement-form. Loop cones: the quartet's decide/advance emission
  applies the same rule inside each cone (loops.rs), carried state stays Named.
- Products consumed whole (call args, capture structs, launch `pair`s) stay
  materialised — but their FIELDS are ExprTrees now (assembly still field-by-field,
  no hanger locals feeding the fields).
- `--perf`/#19a, #14 caps, #17 dedup key, #18 arena: orthogonal; dedup keys get
  RE-KEYED text (shapes that merged before still merge — text is a function of
  the same inputs).

## 3b. As-built (WP-B Phase 1, S22 — orchestrator inline; codex network-dead twice)

Shipped as an increment INSIDE the existing walk (op text-forms untouched —
R-TEXT free): `DevEmit` gains the plan + an expression memo; `store_obj` routes
Inline values to memoized `(expr)` strings; `component_expr` resolves THROUGH
dissolved products; the decl loop skips expr-only objects; the input param slots
as the literal `in` (the `o0 = in;` prelude class is deleted).

Deviations/decisions, all emitter-side (the query is untouched):
1. **Call targets force-Named** — the §3 post-call `if (*trap)` must keep its
   statement position; the backend-agnostic query cannot know the trap protocol.
2. **Product-typed Inline force-Named** (the plan's sanctioned fallback) — the
   braced-aggregate-literal form is deferred; exhibits unaffected (their
   wrappers are Dissolved).
3. **Captures read through the Named operand product's field** (`o10.f1`),
   never via the feeder object — the feeder may be Inline and reading it
   directly would re-emit its expression (the d_fn4 `(o5/512)` duplication the
   orchestrator review caught). The per-iteration `pair.fK = o10.fK` re-reads
   are WP-D's hoisting target.
4. **Array handles stay blanket-Named** (query rule) — pointer-alias copies
   (`o6 = o3;`) survive; a handle-inlining refinement is recorded headroom.
5. Exhibit state: d_fn3 = 4 names + 2 index temps + one return expression
   (was 23 locals / 15 assembles); d_fn4's duplicate-wrapper and wrap/unwrap
   classes gone. `-fn1`-class `__host__ __device__` bodies are func.rs's lane —
   WP-C.

## 3c. As-built (WP-C, S22 — orchestrator inline, same session)

`FnEmit` (host + `__host__ __device__` lane) gains the identical mechanism:
plan + expression memo + dissolved-`component_expr` + decl skip + input alias
`in`. Host force-Named set: Call targets (host callees trap via `flow_trap`
inside — position is semantic) AND every bulk-op target (launch/readback
machinery needs an lvalue: `cudaMemcpy(&oN, …)`), plus product-typed Inline.
Scalar launch args inline (a fold seed constant rides the launch call:
`k0_0<<<…>>>(t1, 0, o5, 10)`). Same Div/Mod slot-fetch reorder as WP-B. Phi
strict-select discipline unchanged (arms as temps — an inlined Phi RESULT is
fine; arms never re-form). Print's residual-erased token-product keeps one
local (`oN = (expr); flow_print(oN)`) — the plan excludes token-carrying
objects by design; recorded headroom with the array-handle aliases.
Exhibits: sepia channel = one line per coefficient row; fn1 (matmul v2 body)
= one return expression (was 10 locals / 12 statements); golden corpus
re-pinned +83/−432, every diff hand-read (prelude deletions, wrapper
dissolutions, inline substitutions — guards/selects verbatim).

## 4. #16 (loop-invariant hoisting) rides the same query

A per-thread-loop body subterm whose leaves are all loop-invariant (no dependence
on the iterator or carried state) hoists to a Named point before the loop —
d_fn4's per-iteration `pair.f0..f3` re-assembly collapses to preloop. Same
classification machinery, one extra rule: `Invariant(subtree) ∧ inside-loop ⇒
hoist boundary`. Shipped as the LAST WP (it changes where, not whether, text
appears — smallest semantic surface).

## 5. Gates (every WP; the mandate demands differential at every step)

1. `cargo test --workspace` green; fmt clean.
2. cuda goldens re-pinned + **hand-read** (S13 discipline) — the point of the
   change IS the text; every re-pin is reviewed against the exhibits.
3. llvm differential unaffected (no llvm change until its assessment WP).
4. cuda differential: local textual gates + the next box leg runs the full
   10-example + 320-testgen raw+rewritten sweep (S21 recipe, clang≥15 guard).
5. Exhibit acceptance: regenerate `matmul512_cap.cu` — d_fn3 collapses to the
   single-expression form (zero `o{n}` hangers; named points only at genuine
   splits), d_fn4's wrap/unwrap round-trip (`o10→o11/o12`) gone.

## 6. WP sequencing (codex codes; orchestrator designs/reviews/reconciles)

- **WP-A** `flow_ir::emission_plan` + tests (classification goldens on the 10
  examples + matmul4_cap; proptest R-NODUP/totality over testgen).
- **WP-B** `DevEmit` consumer (device twins) + golden re-pins.
- **WP-C** `FnEmit` host + kernel bodies + loops.rs cones + re-pins.
- **WP-D** #16 hoisting + re-pins.
- **WP-E** llvm assessment (measure: post-rewrite `.ll` for the exhibit set;
  expected no-op — record either way in backend-llvm/suggestions.md).
- Box leg after WP-C and again after WP-D (one combined run if timing allows —
  the S21 one-box pattern).

## 7. Non-goals

- No region formation, no launch-count change, no residency change (that is
  region emission v2, S22 item 3 — sequenced after).
- No IR/rewrite change; the graph is untouched. This is emission only.
- No llvm text change in WP-A..D.
