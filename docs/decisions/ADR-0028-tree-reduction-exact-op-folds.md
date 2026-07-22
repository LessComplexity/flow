# ADR-0028: Tree reduction for exact-op folds — associativity as a graph property

Date: 2026-07-22 · Status: **accepted — delegated decision** (S20 optimization-marathon mandate: "for the adr suggestions no need for my approval, let the best option take the case for now") · Implementation queued as a marathon wave.
Motivation: backend-cuda suggestion #10 (parallel `Fold` — was "Sapir's call"; the mandate assigns it to the marathon) · Bend/HVM evidence (parallelism is the dependence structure, not an annotation) · Futhark PLDI'17 §2.1 (`sFold` chunked fold, well-defined iff ⊕ associative) + the folding-floats blog (compiler ≡ interpreter discipline — Futhark *disables* float opts its reference interpreter doesn't do; Flow adopts the same rule verbatim).

## Context (what forced the question)

`Fold` is a **dependence chain**: the oracle is a strict left fold (`flow-interp/src/eval.rs:252` — "(c₁…cₖ, acc, e) per step, strictly in-order"), and every backend pins that order (CUDA: one `<<<1,1>>>` kernel per standalone fold, BC4; twins: per-thread sequential loops). For a *standalone* fold over n elements this is O(n) serial on the GPU — the one Core op with no parallelism story. The capture-form matmul does **not** care (its fold is a per-thread sequential dot product inside N² parallel map threads — already the right shape); a standalone `sum`/`product`/`min`/`max` over a big array is the case that loses.

The blocker was never engineering, it was a *semantics* question: reordering a fold's combination changes results in general (f64 non-associativity; trap order). So the decision splits by operator class.

## Decision

**D1 — Exact-op fold bodies may be tree-reduced with zero oracle change.** Where the body graph is exactly `(acc, e) → acc ⊕ e` (or `e ⊕ acc`) with ⊕ ∈ {wrapping `Add`, wrapping `Mul`, `Min`, `Max`, `And`, `Or`, `Xor`} over any integer width or `Bool`, the result is **bit-identical under every parenthesization** (mod-2ⁿ arithmetic is a ring; min/max/and/or/xor are associative). A *fixed canonical tree* — K-element sequential chunks per thread, then a fixed pairwise merge — needs **associativity only**; commutativity is never required, because the tree shape is pinned, not data-dependent.

**D2 — Eligibility is a graph property, recognized, never annotated.** A fold is tree-eligible iff its sealed body graph is *exactly* the ⊕-application (no other morphisms) and **syntactically trap-free** (no `Div`/`Mod`/`Index`/`Update` anywhere in the body graph — tree order changes *which* guard fires first, and trap-kind/first-trap-wins is pinned). No user annotation; the recognizer reads the graph (ADR-0027 D2b's "parallel-vs-series is read off the graph" applied).

**D3 — Realization per backend.** CUDA: the Futhark `stream_red` shape — grid-stride outer, per-thread sequential accumulation over a contiguous chunk in a register, shared-memory pairwise merge; chunk count a compile-time function of the static n (all sizes static in Core). LLVM: the canonical tree as a chunked loop nest — clang vectorizes *integer* reductions exactly at `-O2`; no fast-math, no FMF, ever (oracle discipline). Interp: unchanged — the oracle's left fold is already bit-identical to the tree for D1's operators, so R1 needs no new rule (the proof obligation is D1's algebra, pinned by property tests over testgen).

**D4 — Float folds stay sequential-pinned.** f32/f64 `Add`/`Mul` bodies are NOT eligible — reordering changes bits, and Flow's oracle pins order (the Futhark contract "some order of application" is unavailable). The escape hatch is the recorded *canonical-tree re-pin* candidate (the oracle would compute the same fixed tree the backends emit — a language-semantics change); it stays **deferred** until a standalone float-reduce benchmark justifies it. None exists on the current matrix.

## Semantics notes

- **Trap order preserved by exclusion:** D2's trap-freedom check is what makes reorder unobservable (no guard can fire anywhere in the body). This mirrors the CUDA emitter's `TrapCaps` syntactic capability analysis (S20) — same rule, second consumer.
- **Captures orthogonal:** a capturing fold body is eligible iff the *body graph* matches D2 (capture reads are broadcast edges, not combination operations).
- **Determinism (L2):** the recognizer is a pure function of the sealed graph; the emitted tree shape is a pure function of n.
- **Fold with non-neutral init:** `fold ⊕ init arr = init ⊕ tree(arr)` by associativity — the init is combined once, last, on the left. No neutral-element detection is needed (the fold's explicit init rides).

## Alternatives weighed

| Option | Verdict | Why |
| --- | --- | --- |
| Always-sequential fold (status quo, BC4) | rejected for exact ops | leaves standalone int reductions at one thread — the one Core op with no parallelism |
| Futhark's loose float contract ("some order") | rejected | oracle pins order; compiler ≡ interpreter (the folding-floats rule) |
| User annotation (`par fold`) | rejected | eligibility is deducible from the graph; an annotation is a second source of truth that can be wrong |
| Canonical-tree re-pin for floats (oracle computes the tree) | **deferred** (recorded candidate) | changes language semantics for a case the benchmark matrix doesn't exercise; revisit with a standalone float-reduce bench |

## Consequences

- New wave: the body-shape recognizer (shared, flow-ir or rewrite — one place, per the BL7 one-source rule) + the two backend realizations + property tests (tree ≡ left-fold over random exact-op bodies at multiple widths, and chaos-monkey `Mod`/`Index` bodies must *not* be recognized).
- Expected impact class: parallelism for standalone int/bool folds (O(n) serial → O(n/p + log n)); **zero** matmul-table movement (recorded honestly — matmul's folds are per-thread already).
- suggestion #10 (backend-cuda) closes when the CUDA realization lands; the parallel-`Fold` standing item leaves "Sapir's call" via the marathon mandate.
