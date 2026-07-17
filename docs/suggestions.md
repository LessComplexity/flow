# System suggestions (category-theory derived)

> Roll-up of every `components/<name>/suggestions.md`, highest payoff first (ADR-0017).
> Each row cites the FRAMEWORK rule it applies; detail lives in the linked component
> file. Applying one = a normal plan → implement cycle; applied rows move to the
> session log (see changelog below).

## Open / parked

| # | Component | Rule (§) | Change | Status | Detail |
| --- | --- | --- | --- | --- | --- |
| 1 | backends (all three) | §4.4/§7.4 strategy 2-category | Shared `Backend` contract + `TargetText` type, fixed by ADR before the first backend | **Parked until P5 design** — the audit itself scopes it "owned by the backend ADR"; writing it now is spec-without-implementation (the project's named top risk) | [backend-llvm](components/backend-llvm/suggestions.md) |
| 2 | lower | §5 one source of truth | Generic `dfs_cycles` util for `detect_cycles`/`find_cycle` | **Deferred (YAGNI watch)** — two call sites with genuinely different on-back-edge behavior; a third graph-cycle check triggers it | [lower](components/lower/suggestions.md) |
| 3 | interp | §5 one source of truth | Confine the numeric-width dispatch to one seam | **Refuted on vet (S09), deferred** — premise overstated: only `num_lt`/`num_le` share the 5-way shape; `arith` already seams via `int_arith!`/`float_arith!` (different bodies), `as_int` is integer-only. Revisit only if a width is ever added post-M1 (IN7) | [interp](components/interp/suggestions.md) |
| 4 | interp | §5 deduce-don't-store (perf) | Thread `in_scc`/`topo_order` from `eval_fn` into `run_loop`/`derive_plan` | **Deferred** — perf store without profile evidence (HANDOFF §7.2 step 6); matches the S08 optional-hardening item | [interp](components/interp/suggestions.md) |
| 5 | cli | §5 define each boundary once | One declared `Diagnostic` target, one renderer | **Soft** — revisit when `flow-cli` is built (Session-06 audit) | [cli](components/cli/suggestions.md) |
| 6 | lower | §5 one source of truth | Route `emit_fanout`'s return-position no-value case through `ChainCtx::RetValue` (as `seq`/L1611 does, S11) instead of the generic L1306 fall-through | **Open (small)** — found by the ADR-0019 WP2 review; pre-existing, deliberately left in-scope-minimal | [lower](components/lower/suggestions.md) |

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
