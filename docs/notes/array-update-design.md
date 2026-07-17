# Design note: array-element write (`c[i] <- x`) — the categorical options

Written: 2026-07-17 · Session 12 · raised by Sapir while exploring matmul (S12).
Status: **discussion note → ADR candidate**. Core scope is frozen (HANDOFF §4); anything here is Core+1, by ADR, Sapir's call.

## The gap it names

Core arrays are constructible only by literal / `map` / `zip` / `enumerate` — no element write. Consequence (seen in matmul4): the i,j enumeration must be unrolled into N² named bindings; loop-driven *construction* of an array is inexpressible. The k-loop (read side, dynamic indexing) is fine — reads never mutate.

## Why raw mutation is off the table (vision grounds)

A store op (`c[i] <- x` as imperative mutation of a shared array object) breaks, in one stroke: the one-definition rule (ir I3, SSA-shape — every analysis reads adjacency once), determinism-by-dataflow (E2 — two writes with no dataflow between them = scheduler-visible order), parallel fanout safety (aliased writes = races by construction), and the E3 memory story (aliasing analysis appears from nowhere). This is the same reason the IR has no `Store` (ADR-0013 omits the heap quartet). Not recommended at any stage.

## Option A (recommended): pure `update` + the existing `mut` rebind sugar

```
Update : ([T; n] × I × T) → [T; n]     // fresh array, slot i replaced; OOB traps like Index
```

Surface `c[i] <- x;` desugars to `update(c, i, x) -> c;` — **exactly the `mut` rebind mechanism that already exists** (`acc + 1 -> acc` mints a fresh SSA object; loop back-edges route it). No new binding semantics, no comptime question at all: the index is a runtime operand of a pure op, bounds-checked like `Index` (OOB ⇒ trap, ADR-0013 class). `mut c: [f32; 16]` becomes loop-carried state like any tuple today.

**The performance answer is already in the spec.** Naive copy is O(n) per write — but the §10 last-use frontier (E3's own machinery) makes in-place lowering a *deduction*: when the source array's last use is the `update` itself (true for every `mut c` rebind in a loop — the old `c` is dead each iteration), the backend mutates in place. This is the linear-types functional-array result, and Flow already runs the identical play for the IO token (I4 linearity ⇒ effects thread as data ⇒ backends compile to plain sequencing). An array threaded through `update` chains is the *optional* (optimization) version of the token's *mandatory* linearity. CPU/CUDA get memcpy-free matmul from pure semantics; nothing new in the memory model.

Rewrite laws come free (layer 3, for the P4 table): `index ∘ update` at same `i` = the written value; at `j ≠ i` = `index` of the base; `update ∘ update` at same slot collapses. Verilog: a `mut` array carried through a single loop = a RAM block with one write port — the E1/FSM story extends, feedforward stays rejected-with-error until designed.

Fixed `n` stays in the type — `update` does NOT need dynamic arrays. With `update` alone, full loop-driven matmul at fixed N works: flatten i,j to one loop `t in 0..16`, `i = t / 4`, `j = t % 4`, `c[t] <- cell(a, b, i, j)` (single canonical loop — even the L1504 nesting restriction is untouched).

## Option B (the prettier sibling, bigger step): `tabulate` — arrays as representable functors

`[T; n] ≅ (Fin n → T)` — an array *is* a function from indices; `index`/`tabulate` are the two halves of the iso. `tabulate : (I → T) → [T; n]` builds an array from its index rule; `update` is then *deduced* (`tabulate(j ↦ j == i ? x : c[j])`), and matmul is one expression: `C = tabulate(t ↦ dot(row(a, t/4), col(b, t%4)))`. Categorically the cleanest (representability; `map`-fusion laws extend). Cost: the argument is a *function value* — requires closures/capture in inline blocks, which Core+1 has parked and which drags the whole capture-analysis story in (today `map` bodies are closed by L11xx design). B subsumes A but is a language-sized step; A is one op.

## Recommendation

**A now (as the first Core+1 array ADR when scope opens), B later if/when closures land** — B then *deduces* A's op away (deduce-don't-store applied to the op set, the `iota`/ADR-0018 precedent). A is: 1 IR op + typing row, 1 builder primitive, 1 interp arm, 1 lower desugar of `c[i] <- x` onto the existing mut-rebind path, rewrite rows for P4's table. No memory-model reopen (fixed-size, value semantics; E3 untouched — the *heap* trigger stays dynamic arrays/`Vec`, which remain a separate, later ADR).
