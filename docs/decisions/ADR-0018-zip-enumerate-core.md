# ADR-0018: `zip` and `enumerate` join Flow-Core as collection primitives

Date: 2026-07-16 · Status: accepted (decided with Sapir, Session 09 — explicit scope
change; supersedes the "no zip" clause of the Core collection set)

## Context (what forced the decision; spec refs)

Core's `map` is unary with a closed body (no capture — L1108), so elementwise binary
operations over two arrays cannot be expressed except by an unrolled array literal
(`examples/vector_add.flow`, `examples/zip_demo.flow` document the workaround and its
non-genericity). The v0.2 corpus already *plans* `zip`/`enumerate` as stdlib
(user-guide Appendix A) and *uses* `zip` opaquely in §8.4 (matmul), but neither is
specified or realized. Categorically both are natural transformations —
`zip : [A;n] × [B;n] → [(A,B);n]` is the canonical iso `A^n × B^n ≅ (A×B)^n`, natural
in `A` and `B`; `enumerate : [A;n] → [(idx,A);n]` likewise — so their naturality
squares are future layer-2 rewrites for free (category-ir §7.1/§7.2 pattern), and
every backend realizes them cheaply (FPGA: wire re-bundling; GPU: fuses into the
consuming kernel; CPU: index loop). Full derivation:
`docs/notes/2026-07-16-sizes-generics-execution-graphs.md` §2a.

`iota` was considered and **dropped**: deducible (`iota = map π₀ ∘ enumerate` on any
`[T;n]`, or an array literal) — deduce-don't-store applies to the op set itself.
Capture-in-map-bodies, ranges (`[0..N]`), and size-generics stay out (notes §2b/§2c —
later ADRs).

## Decision (one paragraph, imperative)

Add two pure operations to the realized Core op set (the ADR-0013 delta grows by two):
`Operation::Zip` typed `([A;n], [B;n]) → [(A,B);n]` (source is the 2-tuple product,
Pair-then-primitive as for every binary op; both arrays the same size `n`), and
`Operation::Enumerate` typed `[A;n] → [(i32, A);n]` with the index component **pinned
`i32`** and the extra condition `n ≤ i32::MAX` (builder rejects and validate re-derives
the bound — the F4/SND-3 precedent). Surface: `zip` and `enumerate` are **builtins**
resolved by name in `flow-lower` exactly like `print`/`println` (no new tokens, no
grammar change; the stages already parse as calls); name-collision with a user `fn`
follows the `print` precedent. Both are effect-free: legal in parallel fanout, no
token. The interpreter (oracle) defines their semantics now; backends inherit via
differential tests when built. HANDOFF §4.1 Collections is amended to "«map», «fold»,
«zip», «enumerate»". Level-A spec files stay untouched (realized-set delta, as with
ADR-0013/0015/0016).

## Consequences (tradeoffs, implementation impact)

- `flow-ir`: two `Operation` variants; builder constructors (`zip`, `enumerate`) with
  I2 typing + the enumerate size bound; `validate::edge_type_ok` arms (independent
  re-derivation) **+ typing-table-golden rows in the same change**; Mermaid labels;
  proptest generator extended so seal⇒validate-empty covers the new ops.
- `flow-lower`: builtin routing (`is_print_builtin`-style) at call-shaped stages; type
  synthesis; new L-codes for misuse (non-tuple/non-array inputs, size mismatch,
  enumerate over-`i32::MAX`); golden for the updated example.
- `flow-interp`: two eval arms (elementwise pair; `(i as i32, x)` pair); value-contract
  tests. Pure/total — no trap, no token, no fuel subtlety beyond the per-morphism cost.
- Composition rules recorded for P4 rewrites (not implemented now):
  `zip ∘ (map f × map g) = map (f×g) ∘ zip` · `enumerate ∘ map f = map (id×f) ∘ enumerate`
  · `map π₁ ∘ enumerate = id`.
- Examples: `zip_demo.flow` becomes the builtin showcase; `vector_add.flow`'s "Core has
  no zip" header is now false and is rewritten to the zip form.
- Capability matrix gains a `zip / enumerate` row (interp ✅, backends planned).

## Spec impact (exact files/sections to patch; patched? yes/no)

Level A untouched. `HANDOFF.md` §4.1 (Collections line) patched — yes. Plan:
`docs/components/ir/plans/plan-zip-enumerate.md`.
