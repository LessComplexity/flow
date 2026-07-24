# System suggestions (category-theory derived)

> Roll-up of every `components/<name>/suggestions.md`, highest payoff first (ADR-0017).
> Each row cites the FRAMEWORK rule it applies; detail lives in the linked component
> file. Applying one = a normal plan → implement cycle; applied rows move to the
> session log (see changelog below).

## Open / parked

| # | Component | Rule (§) | Change | Status | Detail |
| --- | --- | --- | --- | --- | --- |
| 1 | backends (all three) | §4.4/§7.4 strategy 2-category | Shared `Backend` contract + `TargetText` type, fixed by ADR before the first backend | **DONE (S12): ADR-0020** — contract-by-convention `emit(&CategoryIr) -> Result<String, EmitError>` + shared `flow-rt` runtime; flagged for Sapir review | [ADR-0020](decisions/ADR-0020-backend-emission-contract.md) |
| 2 | lower | §5 one source of truth | Generic `dfs_cycles` util for `detect_cycles`/`find_cycle` | **Deferred (YAGNI watch)** — two call sites with genuinely different on-back-edge behavior; a third graph-cycle check triggers it | [lower](components/lower/suggestions.md) |
| 3 | interp | §5 one source of truth | Confine the numeric-width dispatch to one seam | **Refuted on vet (S09), deferred** — premise overstated: only `num_lt`/`num_le` share the 5-way shape; `arith` already seams via `int_arith!`/`float_arith!` (different bodies), `as_int` is integer-only. Revisit only if a width is ever added post-M1 (IN7) | [interp](components/interp/suggestions.md) |
| 4 | interp | §5 deduce-don't-store (perf) | Thread `in_scc`/`topo_order` from `eval_fn` into `run_loop`/`derive_plan` | **Deferred** — perf store without profile evidence (HANDOFF §7.2 step 6); matches the S08 optional-hardening item | [interp](components/interp/suggestions.md) |
| 5 | cli | §5 define each boundary once | One declared `Diagnostic` target, one renderer | **Soft** — revisit when `flow-cli` is built (Session-06 audit) | [cli](components/cli/suggestions.md) |
| 6 | lower | §5 one source of truth | Route `emit_fanout`'s return-position no-value case through `ChainCtx::RetValue` (as `seq`/L1611 does, S11) instead of the generic L1306 fall-through | **Open (small)** — found by the ADR-0019 WP2 review; pre-existing, deliberately left in-scope-minimal | [lower](components/lower/suggestions.md) |
| 7 | rewrite | §5 / §9.2 / §4.3 | P4 headroom: precise DCE, constant dedup via replay channel, layer-2 naturality pass, generic-SCC replay | **Open (post-P4, S12)** — all strictly R1-safe extensions of shipped conservative choices | [rewrite](components/rewrite/suggestions.md) |
| 8 | backend-cuda | §5 deduce-don't-store / §4.5 Law 1 | Remaining headroom: region-v2 emission (#0 — the top item), by-value device products (#1), batched trap checks (#3), scalar forwarding (#4 — subsumed by regions), grid-stride/geometry (#5), pinned memory (#6), CDP (#7), nvcc `-O3` row (#8), SoA/tiling (#9), parallel `Fold` (#10 — oracle ADR), NVRTC (#11), arena v1.1 (#18a), invariant hoisting (#16 — WP-D, next), minimal-emission residue (handle aliases · product-Inline braced literals · print token-product local) | **Open (S22)** — S22 discharged: **#15 wrap/unwrap chains (minimal emission WP-B/WP-C — `flow_ir::emission_plan` driving DevEmit+FnEmit; d_fn3 = one return expression)**; S20/S21 discharged: captures, #17 (+S21 17b noted), #12, #13, #14/14b, #18 v1.0, #19a, #2 | [backend-cuda](components/backend-cuda/suggestions.md) |
| 9 | backend-llvm | §5 deduce-don't-store | Remaining headroom: heap lowering (the last BL1 face — the 8 MB stack ceiling on the sepia-class alloca shape), dead enumerate-elem elimination, nested products-in-products by-ref (recorded limitation) | **Open (S21)** — S21 discharged: #3 array-fill primitive (ADR-0029 `iota`/`fill`/`widen` — N≥512 benches unblocked) and **WP3b first-class aggregate-move elimination** (pointer-only staging + `llvm.memcpy`; matmul256 clang -O2 OOM → 0.08 s/57 MB). S20 discharged: #2 Update elision, #6 by-ref captures, #7 fn attrs, #8 by-ref call args | [backend-llvm](components/backend-llvm/suggestions.md) |
| 10 | ir + backends | §5 one-source-of-truth / §4.4 strategy | **`block_plan` — the backend-generic blocking schedule query (Sapir direction, S27 close, asked twice: "shouldn't it be backend generic?").** `tile_plan` already carries the backend-generic *legality* (bit-exact interleaving, affine reads, row-invariance); the blocking *schedule* — which dims split, panel walk order, residence/reuse structure — is today hand-rolled inside the llvm emitter (jt-outer panel residence, TI/TJ, k-unroll). That schedule is the same *shape* every backend instantiates against its own tier (llvm: L2/registers · cuda: smem/mma fragments · verilog: PE-array dims/BRAM); only the sizes are `Loc` facts. Extract `flow_ir::block_plan` (schedule tree over the tile record, tier sizes as parameters) **when cuda consumes `tile_plan`** — the second consumer is the consolidation trigger (rule of three; extracting before it is premature abstraction §5); llvm's emitter then re-derives its nest from the query. Done-when: llvm + cuda both consume `block_plan`; the emitters hold only per-`Loc` constants + instruction selection | **Open (S27)** — direction committed; gated on cuda `tile_plan` consumption (S28 agenda 3) | [backend-llvm](components/backend-llvm/suggestions.md) |

## Applied (changelog)

Session 09 (2026-07-16 — see [the session log](sessions/2026-07-16-apply-suggestions.md)):

- **lower** — `resolve_ty`/`TypeTable::resolve` duplicate `TyKind ⇀ Ty` trees consolidated
  into `tys.rs:resolve_tykind`; the two callers pass only their `resolve_named` seam (§5).
- **ir** — §5.1 typing-table golden oracle: `validate.rs::typing_table_golden::`
  `edge_type_ok_matches_design_5_1`, 85 rows transcribed from DESIGN §5.1, test-only
  (two-realization independence preserved). All rows agree — no doc/code drift found.
- **syntax** — `LineIndex` owned whole-source `String` copy → `LineIndex<'a>` borrow (§5
  deduce-don't-store); `line_col` values unchanged.

## System-wide reductions

None open. The Session-06 reduction audit ([categorical-model.md §7](architecture/categorical-model.md))
ran the §3 procedure over twelve clusters: consolidations already executed were ratified,
and the three tempting cross-component merges (two `SourceLoc`s, `IrError`/`IrViolation`,
surface-vs-IR `Ty`) are **justified twos** — do not collapse them.
