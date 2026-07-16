# Plan — `zip` + `enumerate` (ADR-0018), cross-component increment

Authority: [ADR-0018](../../../decisions/ADR-0018-zip-enumerate-core.md) ·
Derivation: [notes/2026-07-16-sizes-generics-execution-graphs.md](../../../notes/2026-07-16-sizes-generics-execution-graphs.md) §2a ·
Components touched: **ir, lower, interp** (+ examples, HANDOFF §4.1, global STATUS).
syntax is untouched — `zip`/`enumerate` already parse as call-shaped stages.

## Categorical model of the change (Dat + Trn deltas)

Level B firewall: these are compiler-op additions; the Flow-Cat story (naturality) is
recorded as future rewrite laws only.

**New `Operation` objects (discrete category gains two elements):**

| op | source ty | target ty | extra conditions |
|---|---|---|---|
| `Zip` | `([A;n], [B;n])` 2-tuple, sizes equal | `[(A,B); n]` | element tys arbitrary Core tys; result elem is `Tuple[A,B]` (depth bound I9/LD12 applies unchanged) |
| `Enumerate` | `[A;n]` | `[(i32, A); n]` | index component pinned `i32`; `n ≤ i32::MAX` (builder rejects; validate re-derives independently — F4/SND-3 precedent) |

Both **pure** (no token; legal in parallel fanout; Phi may select their results).
Single-source/single-target holds: `Zip`'s source is the 2-tuple product object
(Pair-then-primitive, exactly like `Add`).

**Composition rules (recorded for P4, NOT implemented now):**
1. `zip ∘ (map f × map g) = map (f×g) ∘ zip` (naturality in both arguments)
2. `enumerate ∘ map f = map (id_i32 × f) ∘ enumerate`
3. `map π₁ ∘ enumerate = id`
4. deduction: `iota_n = map π₀ ∘ enumerate` — why there is no `Iota` op.

**Interp denotation (oracle-normative):**
- `Zip`: `⟦zip⟧((a, b)) = [(a[0],b[0]), …, (a[n-1],b[n-1])]`
- `Enumerate`: `⟦enumerate⟧(a) = [(0, a[0]), …, (n-1 as i32, a[n-1])]`
Total on their typed domain — no traps, no effects, deterministic trivially.

## Work packages (order matters: ir → {lower, interp} → examples/goldens)

### WP1 — flow-ir
1. `Operation::Zip`, `Operation::Enumerate` (graph.rs), with doc comments in the
   existing style (cite ADR-0018).
2. Builder: `zip(...)`/`enumerate(...)` constructors following the existing primitive
   pattern (mint target object, I2-check per the table above, I9 intake). New
   `IrError` variant(s) for the enumerate size bound (builder side).
3. `validate.rs`: `edge_type_ok` arms (independent re-derivation — do NOT share code
   with builder checks); enumerate bound re-derived in the appropriate validate pass;
   **typing_table_golden gains rows for both ops in the same change** (positive +
   negative: wrong elem ty, size mismatch, non-tuple source, non-array, wrong index
   component, oversize enumerate — follow the existing row style).
4. `mermaid.rs`: labels for both ops (lint rules: quoted labels).
5. Tests: builder happy-path + rejection matrix additions; proptest generator
   extended to emit `Zip`/`Enumerate` in valid graphs (seal⇒validate-empty must cover
   them); golden Mermaid for a zip-shaped graph if the existing golden set style
   calls for it.
6. Docs in the same change: ir/DESIGN.md §5.1 table + op-set list + composition-rules
   additions (the 4 laws above, marked Future/P4) + ledger note (ADR-0018);
   ir/IMPLEMENTATION.md rows; ir/STATUS.md counts.

### WP2 — flow-lower (after WP1 compiles)
1. Builtin routing for `zip`/`enumerate` at call-shaped stages, mirroring
   `is_print_builtin` (lib.rs:152) — a `collection_builtin(name)` or extension of the
   existing router; user `fn zip`/`fn enumerate` collision handled exactly as `print`
   (mirror the existing precedent, whatever it is — read the code first).
2. Typing: synthesize target ty from input ty per the table; emit `Zip`/`Enumerate`
   ops instead of `Call`.
3. New L-codes (next free numbers in the L13xx-style catalogue; follow DESIGN's
   catalogue conventions): zip on non-2-tuple / non-arrays / size mismatch;
   enumerate on non-array; enumerate size > i32::MAX.
4. Contexts: pipelines, fanout branches, seq, map/fold body interiors — no special
   casing expected (pure stages); add tests proving fanout legality.
5. Tests: golden IR dumps for the new example forms; rejection tests per L-code.
6. Docs in the same change: lower/DESIGN.md (builtin section, L-catalogue, morphism
   table), lower/IMPLEMENTATION.md, lower/STATUS.md.

### WP3 — flow-interp (after WP1 compiles; parallel with WP2)
1. Eval arms per the denotation above (follow existing per-op cost/fuel convention).
2. Tests: value contracts — zip'd add: `c[0]=100`, `c[15]=115` (the zip_demo
   contract); enumerate: indices 0..n-1 as i32 paired correctly; determinism suite
   untouched; a zip/enumerate-in-fanout acceptance case.
3. Docs: interp/DESIGN.md eval-arm note + IMPLEMENTATION.md + STATUS.md.

### WP4 — examples + acceptance (after WP2+WP3)
1. Rewrite `examples/zip_demo.flow` as the builtin showcase:
   `(a,b) -> zip -> map { p -> p.0 + p.1 } -> c` (+ an `enumerate` demo block);
   expected outputs preserved (`c[0]=100`, `c[15]=115`).
2. Rewrite `examples/vector_add.flow`: header comment updated (the unroll rationale is
   now historical — say "pre-ADR-0018"), body uses zip form.
3. Both examples run green through parse→lower→interp; golden trees/IR/interp outputs
   added per each crate's golden conventions.

## Test matrix (minimum)

| Layer | Positive | Negative |
|---|---|---|
| ir builder | zip i32/f32 arrays, enumerate, zip result into map | size mismatch, non-array, arity≠2, oversize enumerate, wrong target ty |
| ir validate | golden-oracle rows both ops | same set, independent |
| lower | zip_demo golden, fanout use | each new L-code |
| interp | value contracts, enumerate indices | (total — no trap cases) |

## Reconcile checklist (HANDOFF §7.2 step 7 + ADR-0017)

- [ ] Each touched DESIGN's categorical-model/morphism tables updated with the code
- [ ] IMPLEMENTATION.md rows per crate (State=built)
- [ ] STATUS.md per crate + global STATUS (test counts, capability-matrix row
      `zip / enumerate`: interp ✅, llvm/cuda/verilog planned)
- [ ] HANDOFF §4.1 Collections line (done with ADR)
- [ ] ADR ledger row in global STATUS
- [ ] FRAMEWORK §8 sweep: no new parallel objects; deduced iota noted; diagram⇔table
