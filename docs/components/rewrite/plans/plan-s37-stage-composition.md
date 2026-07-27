# plan-s37 — stage composition: the intermediate array is a stored copy of a deduced morphism

> **SUPERSEDED (same session) by
> [`../../ir/plans/plan-s37-stage-structure.md`](../../ir/plans/plan-s37-stage-structure.md).**
> Kept for the reasoning trail; do not implement this design.
>
> What it got wrong, in order of importance:
> 1. **A destructive rewrite, not a query.** It normalises into "Map over an Iota" and drops the
>    intermediate — which makes the store-vs-recompute decision that belongs to the backend, and
>    forbids deliberate materialisation (S27 rung-3 packing).
> 2. **Four match arms in a trenchcoat.** It special-cases per producer op instead of carrying a
>    per-element structure, so the arm count grows with the op count.
> 3. **`Map` treated as a peer of `Iota`/`Zip`.** `Map` carries an arbitrary `FuncId` body; the four
>    bodyless ops do not. Only the latter are one closed structure.
> 4. **No `tile_site` migration.** `tile_iota_size` matches a literal `Operation::Iota` op tag
>    (`algo.rs:1016`), so eliding an iota silently de-tiles matmul — a measured 4.0× cliff.
>
> Its §2 corrections (saxpy's timed window holds a `Zip`, not an `Iota`; tiled sites already skip the
> element load) were verified and carried forward into the successor.

Status: **SUPERSEDED** — not implemented. Written S37 opening, from Sapir's framing.
Component: `rewrite` (the law) · consumers: `ir` (one query), `backend-llvm`, `backend-cuda`.
Supersedes the framing of: S36c §3(b) "iota as an index law", next-session.md §2 second bullet.
Related: `plan-rewrite.md` §4 (layer 1 functor laws), ADR-0027 (capture scoping), ADR-0029
(`Iota`/`Fill` as Core ops), ADR-0018 (`Zip`/`Enumerate`), ADR-0032 (backend-genericity).

## 0. Sapir's framing, which is the general rule

> "In a graph we have object a, a fanout creates object b, a fanout creates object c — we can
> compose the fanout from a→b and b→c into a single fanout, skipping the b object. This general
> rule should solve the iota array creation problem if we prove the fanout points combine."

That is the whole plan. S36c wrote the same win as an emitter special case ("replace the element
load with `trunc i64 %iv to i32`"); stated as stage composition it is **one law with five
instances**, it lands in `mapal-rewrite` where the existing `map g ∘ map f` law already lives, and
it is backend-generic by construction (ADR-0032) rather than emitter-local cashing.

## 1. Why categorically — §5 "deduce, don't store", verbatim

An elementwise stage's output array is a **stored copy of a deduced morphism**. `iota n` stores
`[0,1,…,n-1]`, a value fully determined by `i`. `zip(a,b)` stores `[(a[0],b[0]),…]`, fully
determined by `i` and its two inputs. FRAMEWORK §5 says storing a deduced morphism is a tradeoff
with a cost, and here the cost is measurable and paid twice:

1. an N-element store pass to build the array, plus its frame footprint;
2. an N-element load pass in the consumer — and worse, the array is an **opaque memory object**, so
   `-Rpass-analysis=loop-vectorize` reports `cannot identify array bounds` and the consuming loop
   stays scalar (S36c §3(b)).

The consolidation (§3) is the proof that this is one law, not five optimisations. Write the
morphisms out with their targets:

| stage | signature | element law `out[i] =` | arrays read at `i` |
| --- | --- | --- | ---: |
| `Iota` | `ℕ → [ℕ; n]` | `i` | 0 |
| `Fill` | `(T, ℕ) → [T; n]` | `x` (a capture) | 0 |
| `Map{f}` | `[A; n] → [B; n]` | `f(a[i])` | 1 |
| `Zip` | `([A;n],[B;n]) → [(A,B); n]` | `(a[i], b[i])` | 2 |
| `Enumerate` | `[A;n] → [(ℕ,A); n]` | `(i, a[i])` | 1 |

Every row lands in the same object (`Array[T;n]`) and every row's element law is a pure function of
`i` and the `i`-th elements of its sources. They differ only in **which** morphisms are defined —
§3's exact criterion. So they are one object with extra structure: an **elementwise stage**

```
Stage{body, sources} :  out[i] = body(captures…, i?, src₀[i], …, src_{a-1}[i])
```

and `Map` is the instance the rewriter already knows. The functor law it already applies —
`map g ∘ map f = map (g ∘ f)` (`functor_laws.rs:analyze_map_fusion`) — is the `a=1` case of

**L1 (stage composition).**  `Map{g} ∘ Stage{f} = Stage{g ∘ f}`, same sources, same `n`.

The intermediate array object `b` never appears on either side of L1. Dropping it is not an extra
step; it is what the equation says.

### 1.1 Why the pass fires on nothing today

`analyze_map_fusion` (`crates/mapal-rewrite/src/functor_laws.rs:98`) reads the producer of the
intermediate and bails on anything that is not a `Map`:

```rust
let (f_body, f_caps) = match f_morph.op {
    Operation::Map { body, captures } => (body, captures),
    _ => continue, // mid not produced by a Map — nothing to fuse.
};
```

`Iota`, `Fill`, `Zip`, `Enumerate` all hit the `continue`. S36c's "map fusion is already shipped and
fires on nothing" has this as its mechanical cause: in the shape corpus, essentially every
intermediate array is produced by a stage from the four rows the match arm excludes.

## 2. Where it actually bites — measured from the sources, not projected

`grep`ed out of `benches/shapes/*.mapal`, timed windows only (`() -> time -> t0` … `t1`):

| shape | timed stage | its source object | producer of that source | in-window intermediate |
| --- | --- | --- | --- | --- |
| transpose_1024 | `ib -> map {…}` | `ib` | `1048576 -> iota` | **`Iota` array** |
| gather_1048576 | `idx -> map { j -> x[j] }` | `idx` | `ix -> map {…}`, `ix` an iota | `Map` array (fuses today) over an **`Iota` array** |
| fir_1048576 | `ts -> map { fold over kr }` | `ts` | `1048576 -> iota` | **`Iota` array** (+ `kr`, a 64-iota, folded) |
| conv2d_1024 | `ts -> map { fold over kr }` | `ts` | `1048576 -> iota` | **`Iota` array** (+ `kr`) |
| saxpy_1048576 | `(x,y0) -> zip -> map {…}` | the zip result | `Zip` | **`Zip` AoS array**, 8 MB |
| reduce_1048576 | `(0.0,x) -> fold {…}` | `x` | generation leg, outside | none |

Two things follow that the inherited framing had wrong, and both are corrections:

**(a) saxpy's timed window contains no `iota`.** Its intermediate is the `Zip` result — a
materialised `[1048576 × {float,float}]` array of structs (`func.rs:6571 emit_zip`) that the
consuming map then loads a struct at a time (`func.rs:6482`). S36c §3(b) attributed its 3.1× probe
to the iota law and quoted saxpy 1t; the *pattern* the probe measured is real, but the instance in
saxpy's bracket is `Zip`, not `Iota`. S36c §4 half-caught this already ("the scalar cases are maps
over a **zipped** array or over an **iota**") without following it back into the attribution.
**Consequence: L1 must cover `Zip` for the saxpy cell to move at all.**

**(b) The tiled sites already skip the in-kernel iota load.** `emit_tiled_map` (`func.rs:2687`)
reads its operands from `site.a.slot` / `site.b.slot` — the *captures* — and derives `i`/`j` from
the loop counters; it never loads the mapped element. So for fir, conv2d and matmul the per-element
indirection is **already gone**, and what L1 removes there is the materialisation pass and the frame
footprint, both outside the timed bracket. The 3.1× lives in the **untiled** maps: transpose and
gather in-window, and every generation leg everywhere.

So the honest expected-value split, to be replaced by measurement:

| site class | shapes | what L1 removes | where it shows |
| --- | --- | --- | --- |
| untiled map over `Iota` | transpose, gather, all generation legs | per-element load + the vectorisation barrier | in-window for transpose/gather |
| untiled map over `Zip` | saxpy | AoS materialisation + strided struct loads | in-window |
| tiled map over `Iota` | fir, conv2d, matmul | the materialisation pass + frame footprint | untimed; total wall time |

`0.3043 → 0.0972` and the `saxpy 1t 0.0972 vs C++ 0.0945` parity claim are **probe numbers**
(S36c §3), not compiler measurements. They are the prediction this plan is pre-registering, not a
result. Pre-register the split above too: transpose and gather should move in-window, fir and
conv2d should not.

## 3. The normal form — one new thing, not five

L1 needs a target representation for `Stage{g ∘ f}` when `f` reads 0 or 2 arrays. The minimum is a
map whose element input is **the index** rather than a loaded array element:

- `Map{g} ∘ Iota(n)` → an index-map with body `g`. **This is already writable in Core**: it is
  exactly a `Map` whose source is an `Iota` output. Nothing new.
- `Map{g} ∘ Zip(a,b)` → an index-map with `a`,`b` as **array captures** (ADR-0027 already threads
  Array captures by reference — `func.rs:body_call_arg`), body `λ i -> g(a[i], b[i])`.
- `Map{g} ∘ Enumerate(a)` → index-map, capture `a`, body `λ i -> g(i, a[i])`.
- `Fill(x,n)` consumed by a map → index-map, capture `x`, body `λ i -> g(x)`.

Every right-hand side is **`Map` over an `Iota`**. That is the normal form, and it needs no new
`Operation`: `Iota` stops being an array anyone reads and becomes the marker that this map's element
input is its index. One fact makes it free — and it is a graph fact, so it is a `mapal-ir` query
consumed by both backends, ADR-0032 rule 1 rather than emitter cashing:

**Q1 (`index_source`).** For a `Map{body, captures}` edge whose mapped array is produced by an
`Iota` whose output has **that map as its only consumer**, the element is `i`. The emitter then
substitutes the loop counter for the element load and elides the `Iota` materialisation.

`tile_iota_size` (`crates/mapal-ir/src/algo.rs:997`) already computes the range half of this and is
private to `tile_site`; Q1 is that predicate promoted to the plan set, which is what S36c asked for.

**Open question (needs answering in step 1, not assumed).** `Iota`'s count rides the *target array
type*, tied by `validate`'s `IotaCountMismatch` (`validate.rs:78`). If a fused index-map elides the
`Iota` object, where does `n` live? Candidate: it already lives on the map's own target type
(`array_parts(&tgt_ty)` at `func.rs:6460`), so the `Iota` object can be dropped outright. Confirm
against `validate` before writing code; if it does not hold, the `Iota` object stays in the graph as
a typing witness with zero emission, and that is a documented `Note:` not a silent divergence.

## 4. Composition rules the implementation must preserve

1. **R1 (byte-equality).** Every rewrite here is value-preserving by the functor law, so stdout must
   be byte-identical to the interpreter oracle at `-O0` and `-O2`, at every `MAPAL_PAR` width. The
   1,280-run differential is the gate, unweakened. Unlike the `Fma` question (S36d §3), stage
   composition changes **no** rounding: it reorders no arithmetic and fuses no operations. It is
   pure dead-store elimination on a deduced value.
2. **R2 (P3 divergence guard, inherited).** Both composed bodies stay transitively loop-free and
   lower-canonical — `is_loop_free_fn` / `single_full_return_writer` already enforce this and the
   new arms must call them, not bypass them.
3. **R3 (trap totality, S34's lesson).** `Iota`/`Fill`/`Zip`/`Enumerate` are total and trap-free, so
   composing them deletes no trap. But the *consumer* body may trap, and its per-element order must
   not move. Composition preserves index order, so this holds by construction — state it, pin it.
   Note the standing exception this touches: `is_identity_body` deliberately refuses bodies
   containing `Widen`/`Iota`/`Fill` because they sit outside `is_pure` (`functor_laws.rs:163`).
   L1 must not quietly widen `is_pure`; if the new arms need those ops admitted, that is its own
   change with its own pins (next-session.md gotcha, standing).
4. **R4 (single-consumer).** The intermediate drops only when its **only** consumer is the composing
   map — `ir.out_edges(mid).len() != 1` already encodes this and stays. A shared `iota` (fir uses
   `ts` once but `kr` inside a fold) must keep its array.
5. **R5 (`Fold ∘ Iota` is out of scope for L1).** fir and conv2d fold over `kr`, a 64-element iota.
   `Fold` is a different functor (it has an accumulator; the law is not `map g ∘ map f`). Record as
   headroom, do not smuggle it in.

## 5. Steps, in dependency order

Each step is independently shippable, gated, and measured. **Do not batch them** — S36c's own
lesson is that a probe's attribution has to be checked per instance.

**Step 1 — `Map ∘ Iota`, the normal form.** Q1 in `mapal-ir` + both emitters consume it. Answer §3's
open question first. Wins: transpose + gather in-window; every generation leg; the materialisation
pass on fir/conv2d/matmul.
Done when: `!` the emitted transpose/gather loop has no load from the iota array, the `Iota`
materialisation is absent (or justified per §3), the 1,280-run differential is green at both opt
levels, and transpose/gather 1t are re-measured with min/median per cell.

**Step 2 — `Map ∘ Zip` into the normal form.** The rewrite rule, in `functor_laws.rs`, with `a`/`b`
becoming array captures. This is saxpy's cell. Highest single-cell value in the tree, and the one
the inherited framing missed.
Done when: no `[n × {A,B}]` in the emitted saxpy, saxpy 1t re-measured against
`clang -O3 -march=native` on the same machine, differential green.

**Step 3 — `Map ∘ Enumerate`, `Map ∘ Fill`.** Same law, two more arms; no new machinery. Cheapest
step, do it last so the interesting ones are not blocked behind it.

**Step 4 — reconcile.** ADR-0029 and ADR-0018 both described their ops as materialising arrays;
after this they describe stages whose materialisation is conditional on having a non-composing
consumer. That is a doc change in the same commit (§6.3), plus `ir`/`rewrite`/`backend-llvm`/
`backend-cuda` IMPLEMENTATION + STATUS rows.

## 6. Risks, named

- **CUDA must not silently diverge.** Q1 is a `mapal-ir` query, so the CUDA emitter has to consume
  it too or explicitly not (`kernel.rs:2607 emit_iota`). A graph fact one backend honours and the
  other ignores is a value-identical but structurally forked pair; the 640-run remote differential is
  the check, and it needs hardware. If the box is unavailable, ship llvm-only with the CUDA arm
  recorded as an explicit ✋ cell, not left ambiguous.
- **The tiled path's `site.a.slot` accounting.** `tile_plan` recognises sites partly *because* the
  mapped array is a provable iota (`tile_iota_size`). Changing what the iota object is could move a
  site out of recognition and silently de-tile fir/conv2d — a large regression wearing the costume
  of an optimisation. Pin the existing tile recognition on all seven shapes **before** step 1, and
  treat a lost tile site as a step-1 blocker.
- **Frame footprint change reshuffles `%Frame`.** Dropping iota arrays changes `build_frame_layout`'s
  field assignment, which is the other P0's subject matter. Sequence: do the two P0s one at a time
  and re-measure the second on top of the first — their wins were probed independently and do not
  compose additively (§2).
- **A published-number claim.** Everything in §2's right-hand column is a prediction. Per
  `CONTRIBUTING.md`, this change arrives with the measurement of what it did and names the cells it
  moves — `docs/performance/shape-ladder-v2.md` and the README's shape rows.
