# Component: lower

Status: tested
Last updated: 2026-07-18 · Session 13 (ADR-0021 element-update `c[i] <- x`)
Spec references: category-ir.md §4 (lowering rules, as corrected by ERRATA LC-4) + §11.1; ERRATA LC-2 (map/fold law); ADR-0013 (realization: edges-only, inline-cycle loops, IO token laws); ADR-0015 (print/println builtins); ADR-0018 (zip/enumerate pure collection builtins); ADR-0019 (`seq` statement block — no IR footprint); ADR-0021 (array element update — `c[i] <- x` desugars to `Update`-then-rebind); user-guide §3/§5; lower/DESIGN.md §0.1 pins 1–5 (binding).
Depends on: syntax, ir Depended on by: check, interp, rewrite, backend-llvm, backend-cuda, backend-verilog, cli

## What works

`pub fn lower(source, &Program) -> Result<CategoryIr, Vec<Diagnostic>>` — the full
Flow-Core surface, end to end: all eight `examples/*.flow` lower to sealed,
validate-empty, lint-clean IR matching the ir-golden shapes (DESIGN §9 contracts hold:
sum_to_n's exit reads the merge view — the 55-not-66 snapshot regression is pinned;
abs folds `-1` with no `Neg`; sepia's `0.0` fold seed resolves f32 via literal-width
unification; countdown reproduces ir golden h; effectful calls thread the token with
the degenerate `tok := r` when B is absent). Five passes per DESIGN §2 (type table →
effects/call-graph → declare → per-fn typing walk + body emission + outer emission →
seal). 52 L-codes (L1000–L1901) with ≥1 rejection test each (except L1901, internal by
construction). The pure collection builtins `zip`/`enumerate` (ADR-0018) route at
call-shaped stages like `print` (`is_collection_builtin`) but carry no token — legal in
parallel fanout and map/fold bodies; emit owns L1606–L1610, the flow-ir builder re-derives
the shapes/bound defensively (LD12/LD26). `seq { … }` (ADR-0019) lowers as an ordered
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
`array_update_emits_no_token_edges`); pure (no token).

## What does not / known issues

- The interpreter does not exist yet — all semantic contracts are pinned structurally
  (graph shape), not by execution. The 55-contract's value half waits for interp.
- Core-minimal restrictions chosen over invented semantics (each an OQ in DESIGN §16):
  routing guards = exactly two bool arms (L1409); nested loops only in the
  inner-exits-via-ret shape (L1504); no general-expression stages (L1302); no
  infinite loops (L1501, the E1 tension); ≤1 surface return site in effectful fns
  (L1307).
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

139 tests: 18 golden Mermaid snaps (8 examples incl. zip_demo + vector_add,
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
`flow-interp/tests/acceptance.rs`. Implementation survived 2 adversarial code reviews + a
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
guards + general nested loops need an flow-ir ADR (I4 token-fork widening, per-arm
cond composition); OQ2 E4 general-expression-stage semantics; OQ8 fn-body tails as
return values (W11 reading — implemented, one-line swap if vetoed).
