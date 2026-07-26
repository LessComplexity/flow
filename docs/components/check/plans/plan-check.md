# Plan — mapal-check (P3 completion: the owed checks)

Written: 2026-07-16 · Session 10 · status: **accepted & built** (proceed chosen; outcome: DESIGN.md + crate, 25 tests, workspace 448 green — see `../reviews/review-check.md`. Built-reality deltas vs this plan: exclusivity went strict-no-exception (CK3 refines D-B); `check` gained the `source: &str` param (design review B1); E3 zero-code stance kept (D-E confirmed))
Inputs: interp/DESIGN §9 + lower/DESIGN §12 (the owed ledger); ADR-0003 (E2), ADR-0004 (E3),
ADR-0013 (realized op set), ADR-0018 (zip/enumerate); ir/DESIGN §9/§17 (obligations, I-RET);
spec architecture.md §2.2.4/§2.2.5, user-guide §5/§6/§7, category-ir §2.6/§10.
Session-10 read fan-out: 7 reader reports (workflow `wf_51a1970c-ad9`).

## Goal

Discharge the four obligations every downstream component currently *assumes*
(interp IN3 trusts them; ir §17 and lower §12 delegate them): Return exclusivity,
E2 effect legality, full-typing residual, E3 lifetime scope — as `mapal-check`, a
pass over lower's output. P3's DoD ("type/effect/lifetime checks for Core") closes.

## Component(s) touched

- `check` (`crates/mapal-check` — today a 1-line stub crate, no deps): all new code.
- `docs/components/check/{DESIGN,IMPLEMENTATION,STATUS,suggestions}.md` + INDEX row flip.
- No changes to ir/lower/interp code. (Acceptance tests add `calc.mapal` coverage — test-only.)

## The four obligations — what each actually is (grounded)

| # | Obligation | Source | Reality on the sealed graph | Check's job |
| - | --- | --- | --- | --- |
| 1 | Return exclusivity | ir §17 (I-RET permits ≥1 full-value writers); interp IN3 assumes exactly one fires | Lower's own output is single-writer (L1405 upstream); multi-writer graphs arise from **hand-built IR** (IrBuilder users, future producers) | Static exclusivity rule (below) |
| 2 | E2 effect legality (`print` only in seq context) | ADR-0003:36-38 "mapal-check runs an effect analysis…"; lower §12 defers it | **Graph-invisible**: lower token-threads even illegal fanout branches, so print-in-fanout ≡ print-in-seq as sealed graphs. Fanout membership exists only in the tree | Tree×graph pass (below) |
| 3 | Full typing beyond lowering needs | lower §12 "mapal-check re-walks the sealed graph" | **Discharged by construction**: builder I2 per-call + `validate::edge_type_ok` re-derive §5.1 independently; lower guarantees validate-clean output. Residual ≈ ∅ | `validate()` as boundary assert; no new typing pass |
| 4 | E3 lifetime/escape | ADR-0004 scope; lower §12 | **Vacuous for Core**: realized IR has no `Alloc/Free/Load/Store` (ir/DESIGN:296 "no heap in Core — E3 scope"); `Ty` has no ref variant; a violating graph is unconstructible | Documented vacuity proof in DESIGN + STATUS; **zero code** (YAGNI). Becomes real with the first heap op (post-Core ADR) |

## Model delta (preview of the DESIGN.md categorical-model section)

Level B (ADR-0014 firewall); physical pair **degenerate** (`Dat` + `Alg` only, per
categorical-model.md:110). New objects/morphisms:

New objects (`Dat`): `Checked` (= evidence: empty `Diagnostic*`), `TCode` (discrete
category of check diagnostics), `EffectSig = FuncId → 𝔹` (deduced), `WriterSet(f)`
(deduced fibre), `FanoutRegion` (tree datum, held as data).

| Morphism | Signature | Partiality | Semantics |
| --- | --- | --- | --- |
| `check` | `Program × CategoryIr → Diagnostic*` | Total | the component; empty output = accept. Completes the partial pipeline functor `lower/check : 𝒮 ⇀ Core` (categorical-model.md:230) |
| `exclusivity` | `CategoryIr → Diagnostic*` | Total | static Return-writer rule (obligation 1) |
| `effects` | `Program × CategoryIr → Diagnostic*` | Total | E2: effectful node in fanout branch → T-code (obligation 2) |
| `effectful?` | `FuncId → 𝔹` | Deduced | `ty_contains_token(input) ∨ ty_contains_token(output)` of the lowered `FuncDef` — **deduced from signatures, never recomputed from the tree** (no dup of lower Pass B) |
| `writers` | `Return-object → 𝒫(MorphismId)` | Deduced | `in_edges(ret)` fibre — read off adjacency, not stored |
| `boundary` | `CategoryIr → 𝔹` | Total | `validate(ir).is_empty()` — the one trust boundary (obligation 3) |

Composition rules: (1) check accepts only sealed, validate-clean IR — validate is
the boundary, not re-derived; (2) `effectful?` is deduced through lowered signatures
(deduce-don't-store); (3) diagnostic order deterministic (tree walk order, then
insertion-order graph walk — D2 discipline); (4) `check = ∅` ⇒ interp IN3
assumptions hold (the contract this component exists to discharge).

## Consolidation check (§3)

**Extend, not add — and explicitly NOT a twin of `validate`.** `validate` certifies
graph-shape invariants (I-ledger); `check` certifies exactly the semantic layer
validate's §9 scope note disclaims (exclusivity, surface effect legality). Zero
overlapping rules; check *calls* validate at its boundary instead of re-deriving any
arm. The §5.1 typing table stays single-sourced in ir (golden oracle untouched,
test-only). E2 effect facts are deduced from lowered token signatures rather than
re-running lower's Pass B inference — one source of truth for effects.

## Composition rules / invariants to preserve (§4.5)

- All six coherence laws remain trivially satisfied (degenerate physical pair;
  single-process pipeline, no new `Loc`/`Trm`).
- ir I-invariants untouched (check is read-only over `&CategoryIr` — matches
  architecture.md:169 `TypeChecker::new(&ir)`; no `&mut` needed since E3 inserts nothing).
- Gotchas honored: `typing_table_golden` stays test-only; `ty_contains_token` reused,
  not re-rolled; `LineIndex<'a>` untouched.

## API + crate shape (decisions to ratify)

- **D-A. Entry:** `pub fn check(program: &mapal_syntax::Program, ir: &CategoryIr) -> Vec<Diagnostic>`.
  The tree parameter is forced by E2's graph-invisibility (obligation 2). Rejected
  alternatives: (a) IR fanout annotation — new IR surface + ADR, heavier, serves only
  this check today (ir §17 says escalate only if *genuinely* needed as data);
  (b) E2 inside lower — contradicts ADR-0003's explicit assignment;
  (c) recomputing effects on the tree — duplicates lower Pass B.
- **D-B. Exclusivity rule (static, conservative):** let `W` = full-value writers of a
  function's Return. `|W| ≤ 1` → ok. `|W| > 1` → every writer must be fed from a
  `LoopExit` cone of one shared loop SCC (mutually exclusive exits — at most one fires
  per E1 semantics); anything else → T-code error. Sound over-approximation; matches
  every graph lower can emit and every committed hand-built fixture.
- **D-C. E2 detection:** walk tree fanout blocks; a branch is illegal iff it contains
  a `print`/`println` call or a call to an `effectful?` function (transitively closed
  already, since token-in⇒token-out synthesis puts the token in every effectful
  signature). Error text points at `seq` (ADR-0003:38 mandate). Nested fanouts: rule
  applies per branch at every level. Map/fold inline blocks: effectful bodies are
  already unrepresentable (I4 token-free bodies; lower rejects upstream) — noted, not
  re-checked.
- **D-D. Diagnostics:** reuse `mapal_syntax::Diagnostic`; band **`T####`** (already
  reserved in mapal-syntax diag.rs:11). Sub-bands: T0001 boundary (non-validate-clean
  input — internal/producer bug, the L1901 analogue) · T01xx exclusivity · T02xx
  effects (E2) · T03xx reserved E3 (unallocated until heap ops exist). Mirror lower's
  pattern: `enum TCode` + `code()` + free `diag()`; severity always Error, fix None.
- **D-E. E3:** no `lifetime.rs`, no dead pass. DESIGN carries the vacuity proof
  (no heap ops in realized op set + no ref `Ty`) and the reopen trigger (first
  `Alloc`-class op ADR must add the frontier pass per category-ir §10). STATUS row
  says exactly this.
- **D-F. Deps:** `mapal-ir` + `mapal-syntax` (Diagnostic + Program); dev-deps
  `mapal-lower` (parse→lower fixtures, same as interp). No slotmap need unless a
  SecondaryMap shows up (likely not — passes are fold-over-iterators).

## Build → test recipe

1. `diag.rs` (TCode + `diag()`) → unit: code strings stable.
2. Boundary: `check()` orchestration + T0001 on dirty input → test: hand-built
   validate-violating graph yields T0001 (not a panic).
3. `exclusivity.rs` (D-B) → tests via IrBuilder: two unconditional writers → T01xx;
   two same-loop LoopExit writers → clean; slot-writers (I-RET slot form) → clean.
4. `effects.rs` (D-C) → tests via parse→lower→check: print in fanout branch → T02xx
   naming `seq`; effectful *call* in fanout branch → T02xx (transitive case); print
   under `seq` → clean; pure fanout (`fanout.mapal`) → clean; nested fanout with inner
   print → T02xx.
5. Acceptance: all 8 golden examples + `calc.mapal` (currently untested through lower —
   found in read phase) pass check with **zero diagnostics**; determinism (run twice,
   identical output).
6. Reconcile: DESIGN (model lead-section per ADR-0014 template) written **before**
   step 1 code; INDEX row `planned → modeled` same change; IMPLEMENTATION.md rows,
   STATUS flips, docs/STATUS.md roll-up; review-check.md vs §4.5. No bench (no
   perf-relevant pass; two O(V+E) walks — note in STATUS, matches "no benches run"
   precedent for non-perf work).

Orchestration: DESIGN by orchestrator (hard design); implementation + volume tests by
Opus workflow agents (one per pass cluster) + adversarial reviewers; coherence-law
verification by cheap independent verifiers — same shape as S08/S09.

## Risks / trade-offs / open questions

- **Tree-parameter precedent (D-A):** widens check's input beyond "sealed graph only"
  (lower §12's re-walk phrasing). Honest driver: E2 is not decidable on the graph
  lower emits today. The alternative that keeps graph-only purity is an IR
  fanout-region annotation — an ADR against ADR-0013's lean surface. Recommend D-A;
  flag in DESIGN as revisable if/when channels land (channels will force effect
  structure into the IR anyway).
- **D-B strictness:** conservative rule may reject exotic-but-sound hand-built graphs
  (e.g. writers guarded by disjoint Phi conditions). Acceptable: no producer emits
  those today; loosening is additive later.
- **calc.mapal through lower is unverified** — if it fails to lower, that's a
  discovery logged to syntax/lower STATUS, not a check bug.
- **IN6/IN7 (float ÷0 amendment, integer overflow)** stay parked — they are
  ADR-0013-amendment / backend-ADR items, not check passes (interp §14).
