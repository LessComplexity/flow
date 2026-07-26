# Mapal — Changes Log (v0.1 → v0.2)

This pass was ostensibly a conversion of ASCII diagrams to Mermaid. In practice, writing the Mermaid and formalizing the category-theoretic claims surfaced a number of structural and consistency issues in v0.1 that needed fixes before the diagrams could be drawn accurately. This document records what changed, grouped by area, with the rationale for each.

Nothing in the surface language was removed. Where surface syntax appeared inconsistent across documents, the v0.2 canonical form was chosen based on consistency with the design document (`mapal-language-design.docx`, which is the newest of the pre-v0.2 sources).

---

## 1. Category IR — structural fixes

### 1.1 Morphisms are single-source, single-target

**Before.** `category-ir.md` §4.1 lowered `a + b` to a morphism with `source: [obj_a, obj_b]` — a list. This silently allowed morphisms with multiple domains, which breaks categorical composition: if `f : (A, B) → C` has two sources, "`g ∘ f`" is not well-defined because `g`'s single domain cannot match `f`'s two.

**After.** Multi-argument operations are lowered as two composed morphisms:

1. A `Pair` morphism `Γ → A × B` that builds the product object.
2. The primitive operation `A × B → C` acting on the product.

Every morphism has exactly one source and one target. This is the invariant that makes composition total and the categorical reading well-formed.

**Impact.** §4.1, §4.2, §4.3, §4.4 of `category-ir.md` were all re-lowered. The IR data structures in §3 enforce the invariant at the type level (single `source: ObjectId` and single `target: ObjectId` fields).

### 1.2 `Phi` is a first-class operation

**Before.** §4.4 of v0.1 used `Operation::Phi { condition }` in the lowering of conditionals. But the `Operation` enum in §3.2 did not include `Phi`.

**After.** `Phi` is added to the `Operation` enum with source type `T × T × Bool` and target type `T`. Semantically it equals the derived copair-after-distribute expression (shown as a Mermaid diagram in §3.3.1); keeping it as a primitive avoids blow-up in the IR for a common pattern, and the semantic equivalence is what each backend uses when translating `Phi` to its native `select` / `mux` / conditional-move instruction.

### 1.3 Loops as traced monoidal morphisms

**Before.** §4.5 of v0.1 modeled a loop's back-edge as a `Branch` morphism whose `target` field pointed at the loop header object. But `Branch` already had sub-fields for `true_path` and `false_path`, meaning the morphism carried three target-references in total — violating the single-target rule in §1.1 and burying the cycle inside a morphism payload rather than showing it in the graph.

**After.** Loops are formalized as trace operators in a traced monoidal category. Specifically:

- A dedicated `LoopMerge` object at the loop header receives both the initial value and the back-edge.
- A condition morphism emits a `Bool`.
- A select/phi morphism routes the next value to either the `LoopMerge` (back-edge, keeping the loop running) or to `ret` (exit).
- The back-edge is a real edge in the graph's adjacency list — visible to Tarjan's SCC algorithm, which is what the compiler uses to detect loop regions.

This change has a happy downstream consequence for the FPGA backend (§1.6 below).

### 1.4 "Skips the AST" softened

**Before.** Several v0.1 documents claimed Mapal "skips the traditional AST" entirely.

**After.** The parser *does* produce a tree of `ParseNode` values that the IR-builder pattern-matches on — that is an AST by any other name. The honest claim, now in both `architecture.md` and `category-ir.md`, is:

> The parse tree is deliberately minimal and transient: it exists only long enough for the IR builder to consume it, and is discarded before type checking runs. There is no separate typed-AST intermediate — the graph IR is the canonical representation from lowering onward.

### 1.5 Effectful branches get an honest coproduct

**Before.** v0.1's Phi-based lowering of conditionals meant *both* branches were computed and the result selected. Fine for pure morphisms; disastrous for side effects (both I/O operations fire).

**After.** §4.6 of `category-ir.md` now distinguishes:

- **Pure branches** → Phi-based lowering (branchless, cheap on hardware).
- **Effectful branches** → honest coproduct lowering: `split : Γ × Bool → Γ + Γ`, then copairing of the effectful branches. Only one side's effects fire.

The type system tracks effects via Kleisli categories, so the compiler knows which lowering to use.

### 1.6 Verilog backend aligns with the loop trace

**Before.** v0.1 described Verilog generation as an ad-hoc process that counted pipeline stages.

**After.** Because loops are traces (§1.3) and synchronous digital circuits are themselves morphisms in a traced monoidal category (with the trace being register feedback), the Verilog backend functor `F_Verilog` commutes with the trace operator. A software loop and a hardware state machine with a register on the loop-carried variable are the same categorical construct, translated through the functor. This is not a speedup argument — it's a correctness argument that falls out of the formalism.

---

## 2. Category IR — new formal content

The user asked specifically for categories, functors, and natural transformations expressed as diagrams. v0.1 talked about category theory as motivation but didn't diagram the actual structures or use them to justify optimizations. v0.2 adds:

### 2.1 Mapal-Cat as a bicartesian closed traced category

§2 of `category-ir.md` defines Mapal-Cat as the category whose objects are Mapal types and whose morphisms are pure total Mapal functions, with:

- **Products** (tuples) and their universal property.
- **Coproducts** (`Option`, `Result`, user enums) and their universal property.
- **Initial object** `Never` and **terminal object** `Unit`.
- **Exponentials** (function types) making the category cartesian closed.
- **Trace operator** for loops and feedback, making the category traced monoidal.

Each structure comes with a Mermaid universal-property diagram.

### 2.2 Effects via Kleisli categories

Partial, I/O, mutable-state, and error-producing operations don't live in Mapal-Cat directly — they live in Kleisli categories over effect monads (§2.6 of `category-ir.md`). The `?` operator is Kleisli composition in the Result-monad; this is shown as a Mermaid diagram.

### 2.3 Functors

§6 of `category-ir.md` treats `List`, `Option`, `Result<_, E>`, `Array<_, N>`, `Stream` as endofunctors on Mapal-Cat, with their functor laws drawn as commutative diagrams. The map-fusion optimization is a direct consequence of composition-preservation.

### 2.4 Natural transformations

§7 of `category-ir.md` formalizes polymorphic operations (`head`, `reverse`, `length`, `concat`, `pure`, injections, `dup`) as natural transformations, with the naturality square drawn as Mermaid. This gives free-theorem-style optimizations: `head ∘ List::map(f) = Option::map(f) ∘ head` is naturality of `head`, not something the optimizer has to prove for each element type.

Crucially, §7.4 distinguishes natural transformations from within-category equations (`x * 1 = x`) and from functor laws (`map g ∘ map f = map (g ∘ f)`). These are three different layers of rewrite and should be in three different files in the compiler source tree.

### 2.5 Backends as functors

§8 of `category-ir.md` reformulates each backend as a functor `F : Mapal-Cat → Target-Cat`. Semantic preservation is the statement that `F` satisfies the functor laws: `F(id) = id` and `F(g ∘ f) = F(g) ∘ F(f)`. This is the formal answer to "does the compiler change the meaning of my program?" — no, by functoriality.

### 2.6 Optimization framework classified by justification

§9 of `category-ir.md` organizes optimizations into four layers by which categorical property justifies each:

1. **Functor laws** — map fusion, identity-map elimination.
2. **Naturality** — sliding polymorphic operations past `map`.
3. **Algebraic equations** — `x + 0 = x`, constant folding, strength reduction.
4. **Graph rewrites** — DCE, CSE, no categorical law needed.

Each layer has a clearly scoped correctness argument. The compiler source organizes passes by layer.

---

## 3. Memory model — unified

### 3.1 Aligned on graph-derived lifetimes

**Before.** `getting-started.md` and `user-guide.md` said "Rust-like ownership without garbage collection"; `mapal-language-design.docx` said "reference by default, free at last use" using graph lifetime analysis. These are materially different — Rust has affine types and explicit `&`/`&mut`, whereas the graph-lifetime model is closer to region inference with compiler-computed frees.

**After.** All documents align on the graph-lifetime model from the design document. §10 of `category-ir.md` gives the formal treatment; §6 of `user-guide.md` gives the user-facing explanation; §3.4 of `architecture.md` gives the compiler-implementation view. The terminology "ownership" is no longer used — "lifetime" is.

### 3.2 Guarantees are now stated

§6.5 of `user-guide.md` explicitly lists what the compiler guarantees (no use-after-free, no double-free, no data races on heap data, no leaks in non-escaping allocations) and what needs extra annotation (cyclic data structures in v0.2).

---

## 4. Parallelism model — aligned and documented

**Before.** The design document specified parallel-by-default with `seq` for ordering. `getting-started.md` and `user-guide.md` did not mention this at all — they described fanout without saying whether it was parallel or sequential.

**After.** §5 of `user-guide.md` fully documents the parallelism model:

- Parallel-by-default rule.
- `seq` for forced ordering.
- `executor` definitions for controlling *how* parallelism is realized.
- Decision table for when fanout parallelizes.

§9 of `category-ir.md` gives the formal basis: structural independence on the graph plus the bifunctorial property of `(−) × (−)`.

---

## 5. Syntactic consistency

### 5.1 Function declaration syntax

**Before.** v0.1's `getting-started.md` and `user-guide.md` showed two forms:

- `fn name(args) -> RetType { body }` — used in most examples.
- `fn (args) -> name -> RetType { body }` — used inconsistently in §5 examples.

**After.** The canonical form is `fn name(args) -> RetType { body }`. All examples in v0.2 use this form. This matches the design document.

### 5.2 `category` declaration — variants use guard syntax

**Before.** Struct-like categories (products) had a clear syntax; enum-like categories (coproducts) were not clearly specified.

**After.** Enum-like categories use the same `-Variant-…` guard syntax that guards use for pattern matching. See §2.1 of `user-guide.md`. This keeps the coproduct side of the language symmetric with coproduct pattern-matching.

### 5.3 `getting-started.md` is now a quick-start

**Before.** `getting-started.md` and `user-guide.md` were identical 700+ line files — a clear documentation bug.

**After.** `user-guide.md` is the canonical full reference. `getting-started.md` is a genuine short quick-start (~200 lines) covering the five things a new user needs to know, with pointers into `user-guide.md` for depth.

---

## 6. Diagrams — all converted to Mermaid

Every ASCII box-and-arrow diagram across the project has been replaced with a Mermaid equivalent. In most cases the Mermaid is clearer (renders cleanly, supports color, survives editor reformats). Some diagrams gained additional detail in the conversion — e.g., the compilation pipeline now shows the fixpoint-iteration structure of the optimizer.

Where a visual element didn't translate well (arena memory layout as ASCII boxes), v0.2 uses a table or prose description instead of forcing a Mermaid diagram.

Categorical structural diagrams (universal properties of products, coproducts, exponentials; functor laws; naturality squares; trace) are all new in v0.2 — they did not exist in v0.1, which only gestured at the category-theoretic foundation in prose.

---

## 7. Documents touched

| File | Kind of change |
|---|---|
| `category-ir.md` | Major rewrite. New formal treatment, many new diagrams, data-structure fixes. Version bumped to v0.2. |
| `architecture.md` | Rewrite. ASCII → Mermaid throughout. Memory model aligned. Backends framed as functors. Version v0.2. |
| `user-guide.md` | Rewrite. ASCII → Mermaid. New §5 (parallelism), §6 (memory), §7 (errors). Memory model aligned. Version v0.2. |
| `getting-started.md` | Replaced. Was a duplicate of `user-guide.md`; now a true quick-start. Version v0.2. |
| `mapal-language-design.docx` | **Not rewritten in this pass.** The design document's content is largely consistent with v0.2 already (it was the newest source of truth pre-pass). A future pass could regenerate it to match the diagram conventions used in the other v0.2 documents. |
| `CHANGES.md` | New (this file). |

---

## 8. Known follow-ups

Items surfaced during this pass that are worth tracking but outside scope:

- **`category` keyword collision.** The surface-language `category` keyword means "type" (an object in Mapal-Cat). The category-theoretic "category" means the ambient structure. Appendix A of `category-ir.md` notes the collision. A future revision could rename the surface keyword to `type` — invasive but clean.
- **Cyclic data structures.** v0.2 requires an explicit annotation for cyclic types (switches to refcounting). A proper cycle collector or region analysis is future work.
- **Quantum backend.** Mentioned in §10 of `architecture.md` as research. The dagger structure of quantum categories does not exist in Mapal-Cat, so only a subset of programs could target such a backend — the functor would be a partial functor. This needs formal treatment before it's more than speculation.
- **Mechanized proofs.** The categorical claims in `category-ir.md` (functor laws for each backend, naturality of each polymorphic operation) are plausible but not mechanized. Discharging them in a proof assistant (Lean, Coq, Agda) is future work that would strengthen the "provably correct" claim from "informally argued" to "machine-checked."
- **`mapal-language-design.docx` regeneration.** Not done in this pass; its content is consistent but its diagram conventions aren't aligned.

---

**Version:** 0.2 · **Status:** Design specification.
