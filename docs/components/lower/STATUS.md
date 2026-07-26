# Component: lower

Status: tested
Last updated: 2026-07-25 · **S29 plan-time-builtin**: the `time` builtin — `() -> time` is the first **wire-LESS** stage (the `()` head seeds no wire; syntax now parses it to `ExprKind::Unit` instead of P0001), effectful like `print` (`IoToken → (IoToken, f64)` ms, token rebound from slot 0, the f64 taking the §8.1 lookahead Dest — `emit.rs:Emitter::emit_time` behind `lib.rs:is_time_builtin`, LD28). **No new L-code**: `()` in a value position reuses L1301, a wired `time` reuses L1302, `fn time` is L1009, `time` in a map/fold body is L1605. A 2-of-4 effect-detector gap found while reconciling was closed in the same session (`emit.rs:scan_phi_arm`, `emit.rs:effect_chain` now test `time` too) — see *known issues*. S22 ADR-0031: `iota`/`fill` leave the `ExprKind::Call` path and join the `is_pure_builtin` stage family — `n -> iota` / `(x, n) -> fill` via `emit_iota_stage`/`emit_fill_stage` over the builder's `iota`/`fill_from` (the S21 replay entry doubles as the surface spine); static-n is builder-owned (`NonStaticCount`→ reworded L1612/L1613 teaching diagnostics; oversize literals are width-owned, L1202); a name bound to a literal is the SAME `Constant` object, so `4 -> n; n -> iota` is legal — strictly more expressive than the old AST check (positive pin `iota_bound_literal_count_lowers`); typing threads the previous stage expr (`prev`) for `WTy::Array` size synthesis; `FnBuilder::ty_of` made pub (lower reads builder-owned result types, never re-derives). S21 ADR-0029 amendment: the `widen_i64`/`widen_f32`/`widen_f64` builtin family as bare pipeline stages (`is_collection_builtin` generalized to `is_pure_builtin` + `widen_target` — one predicate across all four routing sites; typing synthesizes the target; `emit_widen` owns **L1614** with the teaching lattice message; L1009 reserves the three names). S20: iota/fill surface (L1612/L1613). S13: ADR-0021 element-update `c[i] <- x`
Spec references: category-ir.md §4 (lowering rules, as corrected by ERRATA LC-4) + §11.1; ERRATA LC-2 (map/fold law); ADR-0013 (realization: edges-only, inline-cycle loops, IO token laws); ADR-0015 (print/println builtins); ADR-0018 (zip/enumerate pure collection builtins); ADR-0019 (`seq` statement block — no IR footprint); ADR-0021 (array element update — `c[i] <- x` desugars to `Update`-then-rebind);
plans/plan-time-builtin.md (the `time` stage builtin — model, composition rules, work items); user-guide §3/§5; lower/DESIGN.md §0.1 pins 1–5 (binding).
Depends on: syntax, ir Depended on by: check, interp, rewrite, backend-llvm, backend-cuda, backend-verilog, cli

## What works

`pub fn lower(source, &Program) -> Result<CategoryIr, Vec<Diagnostic>>` — the full
Mapal-Core surface, end to end: all eight `examples/*.mapal` lower to sealed,
validate-empty, lint-clean IR matching the ir-golden shapes (DESIGN §9 contracts hold:
sum_to_n's exit reads the merge view — the 55-not-66 snapshot regression is pinned;
abs folds `-1` with no `Neg`; sepia's `0.0` fold seed resolves f32 via literal-width
unification; countdown reproduces ir golden h; effectful calls thread the token with
the degenerate `tok := r` when B is absent). Five passes per DESIGN §2 (type table →
effects/call-graph → declare → per-fn typing walk + body emission + outer emission →
seal). 63 L-codes (L1000–L1901) with ≥1 rejection test each (except L1901, internal by
construction); S29 added none — the `time` builtin's two misuses reuse L1301/L1302. The pure collection builtins `zip`/`enumerate` (ADR-0018) route at
call-shaped stages like `print` (`is_collection_builtin`) but carry no token — legal in
parallel fanout and map/fold bodies; emit owns L1606–L1610, the mapal-ir builder re-derives
the shapes/bound defensively (LD12/LD26). **The array-construction builtins `iota(n)` /
`fill(x, n)` (ADR-0029)** route at `ExprKind::Call` stages (the P0108 carve makes them the
only legal call expressions — `emit.rs:Emitter::{emit_iota, emit_fill, static_count_arg}`,
typing synthesizes `WTy::Array` so annotations resolve the literal width); the count is a
positive literal ≤ i32::MAX (the static-n rule — L1612/L1613 own arity/count misuse; a
runtime size is out of Core, ADR-0023 territory). `seq { … }` (ADR-0019) lowers as an ordered
statement block (`emit_seq_block`) with **no IR footprint** — its ordering guarantee is
the token thread source-order lowering already produces (pin d); statements land in the
enclosing scope (bindings escape), the tail is the value, and a seq that continues with
no tail draws L1611. `FanoutKind` shrank to `Plain | Void`. **Element update `c[i] <- x`
(ADR-0021, S13):** an indexed `BindStmt` is a **rebind** of `c` — emit takes an
`Update(cur,i,x)`-then-`rebind()` path (never `bind_new`), reusing existing diagnostics
(non-`mut`/L1104; no new L-codes); the three enclosing-scope sub-passes each recognize it
(carried-set `collect_assigns_stmt`, Phi-arm `scan_stmt` → L1408); typing unifies the value
with the array element type (LD27). **U3-1 capture fix:** `typing.rs:capture_stmt`'s
indexed-bind branch capture-checks the target/index/value as reads without registering the
target as a fresh body-local — so a target/index that captures an enclosing local draws
L1108, not a misleading L1101. Golden: `array_update_straightline` + loop-carried
`mut c` element writes (`array_update_loop_carried_rides_merge` /
`array_update_emits_no_token_edges`); pure (no token). **The `time` builtin
(plan-time-builtin, S29):** `() -> time` — the one stage that takes **no wire**
(`emit_chain` seeds `cur = None` for an `ExprKind::Unit` head; feeding it a value is
L1302, and `()` anywhere else is L1301 with a message naming the one legal use). It is
an effect like `print` — `emit_time` consumes the token register, emits `TimeMs`, then
splits the `(IoToken, f64)` pair (slot 0 → the new token, slot 1 → the milliseconds the
chain carries on, taking the §8.1 lookahead Dest, so `() -> time -> t0` names it and
`-> ret` writes Return). One predicate (`lib.rs:is_time_builtin`, LD25's rule) drives
the reserved-name check (L1009), Pass B's direct-effect walk, D1's `→ f64` row and the
L1605 body check. Two clock reads therefore ride one token chain and can never be
reordered against each other or against the prints between them
(`structural.rs::time_reads_thread_the_io_token`), which is what makes `t1 - t0` an
honest elapsed —
the bench shapes' `iter ms=` line.

## What does not / known issues

- The interpreter does not exist yet — all semantic contracts are pinned structurally
  (graph shape), not by execution. The 55-contract's value half waits for interp.
- Core-minimal restrictions chosen over invented semantics (each an OQ in DESIGN §16):
  routing guards = exactly two bool arms (L1409); nested loops only in the
  inner-exits-via-ret shape (L1504); no general-expression stages (L1302); no
  infinite loops (L1501, the E1 tension); ≤1 surface return site in effectful fns
  (L1307).
- **`time` is an effect at all four of its seams (S29 — the 2-of-4 gap was found in
  reconcile and closed in the same session).** `emit.rs:scan_phi_arm` (L1404) and
  `emit.rs:effect_chain` (`loop_body_has_effect`) had kept testing `print` alone, which
  hoisted a loop-body `() -> time` out of the cycle (one timestamp, not one per iteration,
  `validate` empty — the ATK-02 failure mode). Pinned by
  `time_inside_a_loop_stays_inside_the_loop` (llvm golden). The one-predicate refactor
  remains open as suggestions.md #3.
- D1 is deliberately not a full type checker: the builder is the second-line type
  authority and user-diagnosable `IrError`s map to L-codes (TypeMismatch→L1201 etc.);
  emission keeps a `BTreeMap<ObjectId, Ty>` side table only for recipe dispatch.

## Invariants enforced (and where in code)

- Clean-tree precondition J3 → L1000 defensively (`lib.rs`).
- One-definition `mut`-SSA, scope/snapshot discipline (LD8): `scope.rs` + `emit.rs`
  (routing-guard arms lower against a snapshot; Phi-arm enclosing-mut writes L1408).
- Token laws TL-1/2/3: signature synthesis table (`effects.rs`/`emit.rs` declare);
  current-token register with consume-once (`emit.rs`, L1307 on reuse); loop token
  carried last + exits via every LoopExit (loop recipe).
- Canonical ret-writes (pin 2) via one-stage-lookahead `lookahead_dest` (heads
  included — names surface-bound objects per LD17); effectful full-tuple writers LD18.
- Derives-from-merge tags (L1503) and L1306 return completeness in `typing.rs` —
  every user-reachable seal error is pre-checked; L1901 is internal-only.
- Determinism: no HashMap anywhere in emission paths; span-keyed BTree side tables.

## Test coverage (golden / property / differential / skipped+why)

161 tests (`cargo test -p mapal-lower --release`, 2026-07-25: 120 rejection + 20 golden +
12 structural + 6 capture + 2 proptest + 1 unit; the inventory below is the S13 base;
S29 added the six `time` rows — `structural.rs::time_bracket_types_f64` (two `TimeMs`,
each `IoToken → (IoToken, f64)`, and the bracketed `t1 - t0` types f64) and
`time_reads_thread_the_io_token` (the second read's token is `Proj 0` of the first read's
pair), plus `rejection.rs::{l1009_reserved_time, l1301_unit_as_value, l1302_time_with_a_wire,
l1605_body_time}`; S20 added the iota/fill surface rows, S21 added `golden_widen_builtins` — the four lattice edges as named, direct `Widen` morphisms, incl. the 2^24+1 f32-rounding value — plus `l1614_invalid_widen_sources_reject` (i64 source / array source / f64→f32 narrowing) and `l1614_message_names_the_legal_lattice`, and the L1009 rows for the three widen names): 18 golden Mermaid snaps (8 examples incl. zip_demo + vector_add,
ADR-0018 zip form; + countdown + effectful-call + zip_builtin + enumerate_builtin +
4 seq: two-printlns/mid-chain/return-tail/explicit-ret, ADR-0019 — the two-printlns snap shows the
token thread alone ordering the prints with no seq node; explicit-ret pins that a seq
followed by `-> ret` routes through `emit_ret_existing` with no double-write; +
`array_update_straightline` (ADR-0021) pinning the `Update` op takes no token in-edge;
every snap hand-read against DESIGN §9), 1 fanout-legality
acceptance (pure zip/enumerate in a parallel fanout lower + validate clean), 10 structural
shape assertions (55-contract, token order, Phi counts, signature table; + ADR-0021
`array_update_loop_carried_rides_merge` — a loop-carried `mut c` element write rides the
merge, one `Update` emitted — and `array_update_emits_no_token_edges` — the `Update` op is
token-free even in an effectful `main`), 108 rejection-matrix
tests (all L-codes incl. L1606–L1611 + `fn zip`/`fn enumerate` L1009 collision parity + ATK-finding regressions from the soundness attack + 11 seq: L1611 continues/return + effectful-return-position, valued-effectful-return positive, L1404 effectful seq/fanout in a Phi arm, L1108 capture in seq in map body, effectful-seq-in-fanout L1305 parity, empty/bindings-escape/headless-seed positives),
2 bounded proptests (never-panics + Ok⇒validate-empty+lint-clean; literal-width vs
annotations). The seq `sum_to_n`-reassign value contract (55) lives in
`mapal-interp/tests/acceptance.rs`. Implementation survived 2 adversarial code reviews + a
soundness attack + the WP2 seq-block fixer pass: 25 distinct confirmed findings, all fixed
with named regressions (highlights: ATK-02 effectful-call loops now carry the token; ATK-05
loop-exit bindings land in the enclosing scope; LOWER-RETK-TRUNC u64→u32 ret.k truncation;
WP2 the three enclosing-scope sub-passes — phi-arm scan, loop carried-set, map/fold capture —
now descend into fanout **and** seq bodies, closing two validate-clean miscompiles).

## Performance notes (numbers + bench name + date; regressions flagged)

`lower_scale` (criterion, 2026-06-12): lower_pipeline_32 ≈ 43.6 µs,
lower_vmatch_16 ≈ 40.9 µs. Baseline only; nothing to flag.

## Open questions (→ ADR candidates)

DESIGN §16 OQ1–OQ8, headline ones: OQ1 infinite loops are IR-unconstructible
(`end_loop` requires an exit) though E1 calls them legal; OQ7 multi-route routing
guards + general nested loops need an mapal-ir ADR (I4 token-fork widening, per-arm
cond composition); OQ2 E4 general-expression-stage semantics; OQ8 fn-body tails as
return values (W11 reading — implemented, one-line swap if vetoed).
