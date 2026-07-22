# ADR-0024: Templates — C++-style monomorphizing generics (candidate)

Date: 2026-07-18 · Status: candidate — proposed 2026-07-18 · NOT decided · number provisional · changes nothing until accepted

## Context (what exists today; why monomorphization)

**What parses and rejects today** (verified against the parser, not the docs):

- **Fn-position generics are not in the grammar at all.** `parse_fn_decl` expects `(` immediately after the name (`flow-syntax/src/parser.rs:467`); `fn zip<A, B, N>(…)` draws a generic **P0001** "expected `(` after function name", recovered. No P-code reservation, no out-of-Core scope message — the syntax space is unclaimed and free to choose.
- **Type-position generics parse precisely and are scope-rejected.** `parser.rs:644–657` skips the balanced `<…>`, keeps the base name, and emits **P0103** "generic type arguments are out of Flow-Core (HANDOFF §4); planned for Core+1".
- **Size variables in array types are parse errors.** The length must be an INT literal (`parser.rs:686`, P0001 "array length must be an integer literal"); Core sizes are literals ≥ 1 and live in the type (`flow-as-implemented.md` §2.1, L1208).
- The aspirational exhibit is `examples/vector.flow` (badged header; excluded from the in-Core example set, `flow-rewrite/tests/identity.rs:22` — "the out-of-Core generics sketch"). Its remaining gaps — the `[0..N]` range literal, the destructuring op-block parameter (P0116), the call-expression form (P0108) — are **not** this ADR's business.
- Generic **type declarations** (`type Result<T, E>`) are not in the grammar either (user-guide badge, `user-guide.md:730`).

**The IR is monomorphic — verified directly.** `flow-ir/src/ty.rs` `enum Ty` = `Int | Float | Bool | Unit | Str | IoToken | Tuple | Struct | Array { elem, size: u64 }`. There is no type-variable variant, and the array size is a concrete `u64`. Every downstream consumer — `validate`, the oracle (`flow-interp`), `flow-rewrite`, `flow-backend-llvm` — matches on this closed enum. The schematic letters in the docs' typing tables (`zip : ([A;n], [B;n]) → [(A,B);n]`, ADR-0018) are metalevel op schemas: the builder checks concrete type equality and equal literal `n`. One spec nuance honestly recorded: category-ir §3.4 sketched `size: Option<usize>` — the spec's IR anticipated unknown sizes — but the implemented IR is `u64`, monomorphic. Reopening that option is ADR-0023's substance, not this one's.

**Why monomorphization and not Hindley–Milner:**

1. **There is nothing to infer with.** Typing is discharged by construction (builder invariant I2 + independent `validate`; `flow-as-implemented.md` §3.3) — no unification engine, no generalization, no inference pass exists to extend. HM would be a new component; monomorphization is a new AST pass.
2. **The backend contract demands statics.** Clocked-Cat needs static widths on FPGA (sizes/generics note §1), and the LLVM backend today emits stack/static shapes for fixed arrays. Monomorphization means the oracle and every backend see exactly today's IR — zero change at and below the IR.
3. **The wanted feature is partly value-level anyway.** `[A; N]` puts a natural number in the type; that is C++'s non-type template parameter (`template<int N>`), which HM does not provide. Stamping `N` at instantiation keeps all types monomorphic.
4. **Auditability posture.** The instantiation set is finite and compile-time visible — enumerable per backend under the capability-matrix discipline (a backend rejects what it cannot realize; note §1's partial-functor stance).

Prior art inside the repo: the 2026-07-16 sizes/generics note §2c already names monomorphization as the realization strategy ("the realization ADR should pick monomorphization … backends then see exactly today's static IR"); this is that ADR. Roadmap coverage is thin: HANDOFF §4.2 never names generics; P0103's "planned for Core+1" message is the only pointer. Sequencing against the coproducts-first ordering is an open question below.

## Proposal (two phases; nothing normative until accepted)

### T1 — type-only templates on functions, specialized at the syntax→lower seam

**Surface:** `fn name<A, B>(p1: T1, …) -> R { … }` where `Ti`/`R` may mention `A`/`B`; array sizes stay literal. Examples that respect today's rules (map bodies closed, L1108): `fn swap<A, B>(p: (A, B)) -> (B, A)`, `fn add3<A>(x: A, y: A, z: A) -> A`, fixed-size adapters over the ADR-0018 builtins. Honest scope note: T1's standalone value is modest — its real payload is the instantiation machinery that T2 extends.

**Mechanism — specialization lives at the syntax→lower seam (the decision this candidate makes):**

- Instantiation is **call-site driven**. At a call resolving to a template name, solve the substitution by matching the formal input product against the concrete argument tuple — every type is known at lower time, so this is a structural match, not inference. First occurrence of a parameter binds it; later occurrences must agree.
- **Clone the template AST, substitute, emit a fresh monomorphic `FnDecl`** with a deterministic mangled name (`swap__i32_f64`; `BTreeMap`-ordered throughout — lower DESIGN §11 determinism). The instantiated program flows through the existing five passes A–E (`flow-lower/src/lib.rs:35`) and the entire downstream pipeline unchanged.
- **Placement (proposed):** a new Pass 0 inside `flow-lower`, before Pass A — the IR never sees a type variable, so the change is surface-local. The alternative (a separate `flow-instantiate` AST→AST crate) is open question 2.
- **Diagnostics:** a failed instantiation is a new L-code at the call site with a note at the template span ("instantiation of `swap<A, B>` with `A = (i32, bool)` failed: …"). Errors inside a cloned body are reported with the instantiation chain named — the C++ lesson, see costs below.
- **Checking discipline:** bodies are checked **per instantiation only** (C++-style, pre-concepts). With no constraints there is nothing to check an abstract body against; an uninstantiated template produces no code and no diagnostics. The trade-off is stated in the costs section, not hidden.
- **Caching:** identical substitutions dedup to one instance per program. This is baseline, not optional — see bloat.
- **Call-graph rules apply to the instantiated program** (L1008 acyclicity per instance — the note §3.1 precedent).

### T2 — size-parametric templates (non-type parameters), joint-designed with ADR-0023

**Surface:** exactly vector.flow's shape, rewritten to Core-honest forms —

```flow
fn vec_add<N>(a: [i32; N], b: [i32; N]) -> [i32; N] {
    (a, b) -> zip -> map { p -> p.0 + p.1 } -> ret;
}
```

`N` ranges over literals ≥ 1 and may appear (a) in types (`[T; N]`) and (b) as a compile-time constant value in the body. vector.flow's `N: i32` is a **type pin for that value, not a trait bound** — bounds do not exist here (non-goals).

**N-unification at call sites:** solve `N` from the first array argument whose size mentions it; every later occurrence must equal it, else a new L-code naming both sizes. Arithmetic on `N` (`N - 1`, `N / 2`) folds at instantiation.

**Sequencing with ADR-0023 (dynamic sizing): designed jointly, implementable separately.** Both proposals move sizes toward value-hood, but at different phases: T2 sizes are **instantiation-time constants** — no IR change, no heap, E3 untouched (the ADR-0021 precedent: fixed `n` stays in the type). ADR-0023's sizes are **runtime values** — heap in flow-rt, the E3 reopen. T2 is tier A of the note's ladder; ADR-0023 is tiers B/C. Neither depends on the other's code, but the surface syntax must be decided in the same session so `[A; N]` (parametric) and `[A]` (dynamic) can never be confused.

**Scope honesty:** T2 does not make vector.flow compile as written — the range literal, P0116 destructuring, and P0108 call form stay rejected. With T2 plus ADR-0018's builtins the same program is writable in Core-honest shape, and the builtins become the seed/reference instances (note §2c).

**Template-level recursion** (`msort<N>` calling `msort<N/2>`, note §3.1): permitted only with strict decrease in `N`; each instantiation is a distinct fn, so the per-instance call graph stays acyclic. First increment or follow-up — open question 3.

## Per-component impact (if accepted)

| Component | T1 | T2 |
|---|---|---|
| flow-syntax | `<…>` parameter list on fn decls; the P0001 site at `parser.rs:467` superseded by real grammar; parse tests | same, plus `N` in array-length position (`parser.rs:686` site); explicit-instantiation stage syntax only if chosen (open q 1) |
| flow-lower | **Pass 0 instantiate — the bulk of the work**; new L-codes; deterministic mangling | N-solving + value substitution + decrease check |
| flow-ir | none | none |
| flow-check | none | none |
| flow-rewrite | none (instances are ordinary fns) | none |
| flow-interp (oracle) | none | none |
| flow-backend-llvm | none semantically (instance count ↑ ⇒ codegen volume ↑, see costs) | none |
| testgen | template generation — later, not required for the increment | same |
| examples / docs | vector.flow re-badged or rewritten; user-guide + flow-as-implemented updated in the same change (ADR-0022 D1 rule) | same |

The zero-change rows are the design: everything at and below the IR keeps today's monomorphic world.

## What monomorphization costs (the C++ lessons, honestly)

- **Code bloat.** Every instantiation is a full body copy; `vec_add<N>` used at 40 sizes is 40 functions. On FPGA each instance is **distinct hardware** — wanted (static widths) but real: the bitstream grows per `(T, N)` used. Per-program dedup is baseline; growth across call sites is the user's to observe (an instance-count line in `flow dump` output is a cheap honesty device).
- **Compile time.** AST-level cloning is cheap, but `N` unfolds: `msort<2^20>` is 21 levels of doubling instances, and large `N` stamps large graphs. Guards: an instantiation-count cap (proposal: 128 per template) and a depth cap (64, the `MAX_TY_DEPTH` precedent) — exceeded means a named diagnostic, never a hang (the J1/P0011 philosophy).
- **Error-message quality — C++'s worst legacy.** Flow's mitigations: no overloading or SFINAE ever (non-goal), so the failure space is only shape mismatch; every error carries its instantiation chain; identical body errors across k instantiations dedup to one report plus a count. The residual risk stands: template code in a rarely used substitution is checked late, at the user's instantiation site, in the user's program.
- **Uninstantiated templates are unchecked.** A template nobody instantiates carries latent errors. Single-file programs (modules are P0112, not landed) bound this today; it grows when modules land.
- **Mangled names leak.** IR dumps and Mermaid show mangled names; keep a span-keyed map back to the source form for diagnostics and labels.

## Open questions for Sapir

1. **Call-site syntax:** deduction-only (all parameters solved from argument types — this candidate's baseline) or also an explicit stage form (`-> vec_add<8>`; new surface, clearer at odd sites)? The declaration side is free either way — fn-position generics are today a P0001 parse error, so nothing is claimed; vector.flow's `fn zip<A, B, N>` form is the proposal.
2. **Pass placement:** `flow-lower` Pass 0 (proposed; reuses the typing machinery, one crate's totality story) versus a separate `flow-instantiate` AST→AST crate (cleaner DESIGN §13 boundary, one more crate).
3. **Caps and recursion:** 128 instances/template + depth 64 proposed; does template recursion with strict decrease ship in T2's first increment or a follow-up ADR?
4. **P0103 interaction:** struct/type templates (`type Pair<A, B>`) stay rejected, but P0103's message ("planned for Core+1") becomes ambiguous once fn templates exist. Reserve `<…>` coherently across fn and type positions, and decide the message wording.
5. **Roadmap placement:** HANDOFF §4.2's Core+1 ordering names coproducts first and never mentions generics — does this slot before coproducts (it is smaller), after (ordering ratified in ADR-0021's non-goals), or in parallel?
6. **Uninstantiated-template checking:** a lint-level once-over (name resolution only) or nothing at all?

## Non-goals (asked and answered)

- **No type inference.** Substitution-solving at call sites is a structural match, not unification-with-generalization; no HM, no let-polymorphism under this ADR.
- **No traits / typeclasses / bounds / concepts.** `<N: i32>` is a value-type pin, not a constraint. Constraint checking does not exist because constraints do not exist.
- **No variance, no subtyping**, no co/contravariance rules.
- **No overloading or SFINAE** — one template per name, no specialization-by-case.
- **No templates on `type` declarations** (the P0103 space; struct generics are their own ADR).
- **No first-class templates** — a template is not a value; it cannot be passed, composed, or stored (L1105 stands for functions, a fortiori for templates).
- **No dynamic sizes** — that is ADR-0023's entire substance; T2 sizes are compile-time.
- **vector.flow's incidental syntax stays rejected** — the `[0..N]` range literal, destructuring op-block parameters (P0116), and call-expression form (P0108) are untouched by this proposal.

## Spec impact (exact files/sections to patch IF accepted; patched? no — candidate)

This file patches nothing. On acceptance, in the implementing change: HANDOFF §4.1/§4.2 (the Core+1 list gains templates); user-guide §2.1/§3 (template section + re-badging); `flow-as-implemented.md` §2.3 (functions) and §3.1 (the P0103 row reworded) — per ADR-0022 D1 the operative index updates in the same change; syntax/DESIGN §16 (the P0001/P0103 sites); lower/DESIGN (Pass 0, new L-codes); the STATUS capability matrix (new row); `examples/vector.flow` (re-badge or rewrite to the accepted surface).
