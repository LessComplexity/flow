# ADR-0013: IR realization — all dataflow is edges; Core operation set; loops as inline cycles; IO as a linear world token

Date: 2026-06-12 · Status: accepted (autonomous session); **ratified by Sapir 2026-07-18 (Session 13)**, revisable by superseding ADR

> **Amendment (S13, ratified with the ADR — closes interp IN6):** "division by zero traps" is *integer-only*. Integer `Div`/`Mod` by zero ⇒ `Trapped(DivZero)`; **float ÷0 follows IEEE 754** (±inf / NaN, no trap). This makes interp's IN6 reading normative for all backends.

## Context (what forced the decision; spec refs)

Designing `flow-ir` against `category-ir.md` §3/§5 exposed one internal inconsistency and
four gaps that the data structures cannot leave open:

1. **Where dataflow lives.** §4.1 says the `Pair` morphism's *metadata* "records which
   projections of the ambient environment to bundle", and §5.3's serialization example
   fuses a constant into a primitive (`"op": {"Mul": {"rhs_const": 2}}`). But §4.4/§4.5's
   own diagrams draw every component as a real in-edge (`tr/fr/c → triple`), §5.1 defines
   merge detection as `in_edges.len() > 1`, and §9.4 (last-use), §9.5 (parallelism via
   reachability), and §10 (lifetime frontier) all read adjacency only. Dataflow hidden in
   payloads would be invisible to every analysis the IR exists to serve.
2. **Missing Core operations.** §3.3's `Operation` enum has no array indexing (§4.2 uses
   an `index` morphism it never declares), no `map`/`fold` primitives (ADR-0009/LC-2:
   "Pair then the fold primitive"), no `print` (Flow-Core's only effect, ADR-0001), and
   no unary negation (the parse tree has `UnOp::Neg`; `0 − x` is not IEEE negation —
   `fneg` differs on `−0.0`).
3. **Two loop representations.** §3.3 carries `Trace { body: CompositionId }` (the loop
   as an opaque payload) while §4.5, §5.2 and CHANGES §1.3 demand the back edge be a real
   adjacency edge visible to Tarjan SCC ("not a special field on any morphism").
4. **Effect ordering.** E2 (ADR-0003) makes deterministic effect order non-negotiable,
   but two `Print` morphisms with no dataflow between them would be reorderable by any
   scheduler — the graph itself must order them.
5. **Constants.** §3.3 has `Const(Value) : 1 → A` (terminal-object plumbing) while §3.2's
   `Object.value: Option<Value>` already models constants as objects.

## Decision (one paragraph, imperative)

All dataflow in the IR is adjacency edges; no morphism payload may carry value flow.
Product formation (tuple, struct, fixed array) is per-slot edges: an n-ary product object
receives exactly n `Pair { slot, arity }` morphisms, one per component. Constants are
source objects (`kind: Constant`, `value: Some`, in-degree 0); `Operation::Const` is not
materialized. Loops are inline cycles: a `LoopMerge` object receives exactly one
`LoopEnter` (initial state) and ≥1 `LoopBack` edges (the back edges — real, SCC-visible),
and `LoopExit` edges leave the cycle; `Operation::Trace` is not materialized — the trace
*is* the cycle (CHANGES §1.3), and loop regions are recovered by SCC. Kleisli(IO) is
realized as **linear world-token threading**: `Ty::IoToken`, with
`Print : (IoToken × P) → IoToken`; token-bearing objects are linear (at most one
token-bearing consumer, excepting the structural loop fork into one mutually-exclusive
`LoopBack`+`LoopExit` pair), tokens may not pass through `Phi` (both branches compute),
and map/fold bodies are token-free — effect order is thereby dataflow and E2's
determinism is structural. Three token rules are law, not lowering policy (design-review
findings F1/F2/L2-01/L3-1): (i) *signature synthesis* — an effectful function's surface
signature `A → B` lowers token-threaded as `(IoToken × A) → (IoToken × B)`, degenerating
to `IoToken → IoToken` (surface `fn main()` declares as `main : IoToken → IoToken`; the
input Parameter is the seed token); (ii) *token sink* — a token-bearing object with no
token-bearing out-edge must be the function's Return object (tokens are never dropped;
the effect chain's tail is thereby distinct from dead code); (iii) *token-in ⇒ token-out
for loops* — when the carried state contains the token, every `LoopExit` of that merge
carries it out. The v1 `Operation` set is
the Core subset of §3.3 **plus** `Neg`, `Index`, `Map{body}`, `Fold{body}`, `Print`,
`LoopEnter`/`LoopBack`/`LoopExit`, and `Output` (the single identity-shaped morphism,
existing only as an explicit write into a `Return` object), and **minus** `Identity`
(§2.1.1: never emitted), `Const`, `Trace`, and all out-of-Core variants (`Inject`,
`Copair`, `Distribute`, `Apply`, `Load`, `Store`, `Alloc`, `Free`, `Return`, `Bind`) —
each returns when its feature lands. In Core, division/modulo by zero and out-of-bounds
`Index` are runtime **traps** (defined outcomes, the §2.6 `Err`-monad reading); the
honest `Kleisli(Result)` lift waits for Core+1 coproducts. `Composition.morphisms` is
read as the body's morphism *set* in a valid construction order, not a path (bodies are
DAGs with fanout).

## Consequences (tradeoffs, implementation impact)

- §5.1 merge detection, §9.4 last-use, §9.5 reachability, and §10's frontier work on
  adjacency alone, unmodified; Mermaid dumps render §4.4/§4.5's diagrams faithfully.
- Graphs carry more objects/morphisms than §4.1's compact tables (component edges are
  explicit). Constant folding becomes a pure graph rewrite (Const inputs are visible
  objects, not buried payloads). The criterion bench records the size/speed cost.
- A one-definition rule emerges (SSA-like): every object has exactly one defining
  in-edge, except `Return` (multiple contributors, user-guide §3.2) and `LoopMerge`.
  `mut` updates allocate fresh objects; the back edge routes them to the merge.
- E2 becomes partly structural (token linearity); flow-check still owes the surface
  seq-context rule. Parallelism analysis serializes effects with no special casing.
- The IR is buildable only through an invariant-enforcing builder and sealed by global
  validation (HANDOFF §5 item 3); an independent `validate()` is the property-test
  oracle. Details: `docs/components/ir/DESIGN.md` §9–§12.
- If a backend ever needs an opaque loop summary, reintroducing a `Trace` view is purely
  additive (SCC + LoopMerge already carry the information).

## Spec impact (exact files/sections to patch; patched? yes — Session 04)

`docs/spec/ERRATA.md` gains **LC-4** (the dataflow-is-edges correction). 
`docs/spec/category-ir.md`: §4.1's Pair-metadata sentence and §5.3's `rhs_const` example
each get an inline LC-4 marker blockquote; §3.3 gets a one-line pointer blockquote noting
the enum is the long-horizon shape and the Core-realized set lives here (so a §3.3 reader
is not misled by `Trace`/`Const`/`Identity`) — the enum text itself is **not** rewritten.
CHANGES §1.3 already mandates the inline-cycle reading; no patch needed there.
patched? yes — Session 04.
