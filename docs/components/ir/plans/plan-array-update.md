# Plan: array element update (ADR-0021) — `Update` op + `c[i] <- x` sugar, pipeline-wide

Written: 2026-07-18 · Session 13 · Status: **active** (executes before/with P5 so backend-llvm emits `Update` from birth)
Authority: ADR-0021 > `notes/array-update-design.md` (Option A, ratified) > ir/DESIGN §5.1 conventions > lower/DESIGN §8 (rebind machinery LD4/LD23, builtin-emission precedent LD26) > interp/DESIGN (oracle arms) > rewrite/DESIGN §1.2 plan laws + R1.

## Categorical model of the change (Dat + Trn delta)

- `Dat`: no new object — `Array{T,n}` is closed under a new morphism. Surface `BindStmt` gains a **partial morphism** `index? : BindStmt → Expr` (consolidation §3: an extension, not a parallel statement kind).
- `Trn`: one new op `Update : (Array{T,n} × I × T) → Array{T,n}` (pure, may-trap `IndexOob`). Lower gains one desugar `emit_index_assign = mut_rebind ∘ Update ∘ (cur, i, x)` — composition of existing arrows plus the new op; no new binding semantics.
- Composition rules added (rewrite layer 3): `index_i ∘ update_i = π_x` · `index_j ∘ update_i = index_j ∘ π_a` (i≠j, both const in-bounds) · `update_i ∘ update_i = update_i` (outer wins). **Implemented this increment: the first only** (alias-channel-expressible); the other two need the `reoperand` plan channel — headroom (ADR-0021 §3). Trap class: joins `Index`/`Div` may-trap set.
- Coherence: same-`Loc` `Dat`+`Alg` throughout (degenerate physical pair); no placement/transmission change. §4.5 all-PASS expected to stand.

## Work packages (sequenced TDD; U1 lands the op *everywhere the Operation enum is matched exhaustively* so the workspace compiles at every step; each WP green + fmt/clippy-clean before the next)

| WP | Crate | Work | Tests (min) |
|---|---|---|---|
| **U1 ir + mechanical arms** | `flow-ir` (+arms in `flow-interp`, `flow-rewrite`) | `Operation::Update` (`graph.rs`); builder constructor `update(arr, idx, val)` with seal-time shape checks mirroring `index` (`IrError` twins); validate arm + **§5.1 typing-table row** (`(Array{T,n}, I, T) → Array{T,n}`, I = Index's integer-scalar set); proptest generator arm; Mermaid label. **Plus the exhaustive-match arms downstream (S13 review blocker — omitting them breaks the build):** interp eval arm (clone array, bounds-check `i<0 ∨ i≥n` ⇒ `Trapped(IndexOob)`, write slot — u8 index zero-extends like `Index`'s `as_int`); rewrite `replay.rs::emit_op` Update arm (mirror Index: `slot_feeders` → `fb.update(...)`); `Update` added to `reads_packed_source` in **both** `replay.rs` and `graph_rewrites.rs` (3-tuple internal pack, like Index/Zip); `is_pure` deliberately left without Update (conservatively may-trap-kept — matches ADR §3) | typing-table golden row(s); validate accept/reject (wrong arity, non-array, elem-ty mismatch, index-ty); builder twins; topo neutrality; **identity-replay round-trip over an Update-bearing graph**; interp value contracts (hit, OOB both sides, u8 index ≥128 in-bounds) |
| **U2 syntax** | `flow-syntax` | `BindStmt.index: Option<Expr>`. Parser: **`looks_like_bind` already classifies `c[i] <- x` as a bind** (bracket-depth scan hits `BackArrow` at depth 0 — S13 review; no fork restructuring, no chain-path routing). Extend `parse_bind_stmt`: optional `'[' expr ']'` after the name, before the `:`/`<-` branch; reject `mut` + index, type-ann + index, nested `c[i][j]` (new P-codes, numbered in DESIGN patch); recovery per existing scheme | parse goldens (`c[t] <- v;`, in-loop form); rejection matrix (nested `c[i][j]`, `mut c[i] <-`, `c[i]: T <-`); existing goldens untouched (`c[0] -> println;` chain form unaffected) |
| **U3 lower** | `flow-lower` | **Three explicit wiring points (S13 review — inheritance is NOT automatic; `StmtKind::Bind` today always `bind_new`s a fresh shadow):** (a) emit: `index.is_some()` ⇒ resolve name as rebindable (non-mut/unbound diagnostics fire), emit `Update(cur, i, x)`, then the real `rebind()` — never `bind_new`; (b) `collect_assigns_stmt`: indexed Bind records its target name (array joins the loop carried set); (c) `scan_stmt`/PhiArmScan: indexed Bind records its target (L1408 fires in Phi arms). New L-codes only where no existing one fits (non-array target; elem-ty mismatch if width-unification doesn't cover) | golden IR straight-line + **loop-carried (`mut c` array, `c[t] <- v` in loop body — carried through the merge, the ADR's motivating shape)**; negative: lone `c[i] <- x` loop is not spuriously L1502-rejected; L1408 fires for `c[i] <- x` in a Phi arm; L-code rejection rows; token untouched (pure) |
| **U4 interp contracts** | `flow-interp` | (arm landed in U1) end-to-end contracts through the full pipeline | **matmul4 rewritten loop-driven** (single flattened loop, `c[t] <- cell(...)` — the ADR's motivating program) + fanout branches independently updating the same source array (value semantics) |
| **U5 rewrite** | `flow-rewrite` | equation **L-a only** (`index_i ∘ update_i` const in-bounds → alias to written value; L-b/L-c are headroom — no operand-rewrite channel exists, ADR §3); **testgen**: arrays + `Update` chains in both modes (trap_free: indices literal-in-bounds by construction; default: sometimes OOB) + **lift the one-loop-per-fn cap** (S12 P0 shape must be generable — llvm review F1) | L-a micros (hit; OOB-not-folded; non-const-not-folded); R1 property battery green with Update + multi-loop in the pool (load-bearing); idempotence/determinism unchanged |
| **U6 docs** | — | ir/lower/interp/rewrite/syntax DESIGN + IMPLEMENTATION + STATUS rows; capability-matrix row `array update (c[i] <- x)`; rewrite suggestions gains the `reoperand` channel headroom row (L-b/L-c) | reconcile gate (FRAMEWORK §8 line in each) |

Then P5 (backend-llvm per its DESIGN, which gains the `Update` op-table row — see DESIGN §2 patch): flow-rt → emitter → differential harness picks up Update automatically via examples + testgen.

## DoD

`cargo test --workspace` green with: U1–U5 rows above, the loop-driven matmul4 contract, R1 battery green over an Update-bearing testgen pool, all DESIGNs reconciled. Then P5 DoD per backend-llvm DESIGN §6 (differentials over the Update-bearing pool included).

## Risks / review focus

1. **Rebind-path inheritance is the whole safety argument** (U3): if the desugar bypasses any rebind rule (Phi-arm L1408, loop snapshot LD8, carried-set collection), it silently legalizes what raw mutation was banned for. Reviewers: attack with `c[i] <- x` inside Phi arms, fanout branches, seq blocks, nested loops.
2. **Trap-order under rewrite**: `Update` is may-trap; DCE/fold must not drop or reorder an observable OOB (R1 ⊥-identification covers trap *identity*, not trap *existence*). The OOB-not-folded micros + R1 battery are the instrument.
3. **testgen index generation**: trap_free mode must provably stay in-bounds (else the differential suite flakes); default mode should *sometimes* go OOB (exit-101 path coverage).
4. **Parser fork (corrected S13 review)**: bind-vs-chain is decided *before* expression parsing by `looks_like_bind`'s bounded lookahead, which already returns true for `c[i] <- x` (bracket-depth-aware scan, `BackArrow` at depth 0). U2 extends `parse_bind_stmt` — it must NOT route the LHS through chain parsing (that path would misdiagnose). Chain statements (`c[0] -> println;`) are untouched by construction.

## As-built (S13)

U1–U6 all landed; workspace 558 green, fmt+clippy clean. Deltas from plan:

| # | Plan said | As built |
|---|---|---|
| U1 | ir op + mechanical downstream arms | Landed — `Operation::Update` (`flow-ir/src/graph.rs:116`), builder + validate + typing-golden row + Mermaid; **absorbed the interp eval arm and the rewrite `replay.rs::emit_op` + `reads_packed_source` arms** (exhaustive-match build blocker, as flagged) |
| U2 | `BindStmt.index` + parser extension | Landed on the `looks_like_bind` path; no chain-path fork |
| U3 | 3-point lower wiring (emit / carried-set / Phi-arm scan) | Landed — indexed Bind emits `Update(cur,i,x)` then real `rebind()`; loop-carried array shape golden |
| U4 | interp contracts | Landed — `matmul4` rewritten single-flattened-loop `c[t] <- cell(...)`; fanout value-semantics contract |
| U5 | rewrite L-a + testgen | **L-a only** per ADR §3 (`equations.rs:111`; L-b/L-c need the `reoperand` channel — headroom); testgen grew arrays + `Update` chains and **lifted the one-loop cap to `MAX_LOOPS = 2`** (`tests/testgen/mod.rs:181`) |
| U6 | docs reconcile | Landed (this pass + component STATUS/DESIGN rows) |

Landed **alongside in P5b, not in this plan**: `loop_plan` (BL7) extracted into `flow-ir` (`algo.rs:345`, exported `LoopPlan`) as the one canonical-loop predicate — interp + backend-llvm + rewrite all consume it, replacing their S12 private copies. Then P5 (backend-llvm) picked up `Update` automatically via the emitter op-table row + differential examples/testgen (`differential_matmul_loop_driven_update`).
