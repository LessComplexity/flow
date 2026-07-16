# Next Session

Written: 2026-07-16 · end of Session 09 · by: Claude (Fable 5 orchestrator; Opus workflow agents; category-architect skill)

## Where things stand (≤5 lines)

**Docs tree extended (ADR-0017); CT-suggestion backlog triaged (3 applied, 1 refuted, 4 parked); `zip`/`enumerate` are now Core builtins (ADR-0018), oracle-defined and example-golden.** The category-architect tree is live: per-component `IMPLEMENTATION.md` (model → `file:symbol`, adversarially verified) + `suggestions.md` + `plans/reviews/general/`; top-level `architecture-map.md` (coherence checklist: all six §4.5 laws PASS), `IMPLEMENTATION.md`, `suggestions.md`, immutable `docs/sessions/` logs. Suggestions: 3 applied (lower `resolve_tykind` consolidation, ir §5.1 golden-oracle test, syntax `LineIndex<'a>` borrow), 1 refuted on vet (interp width seam — premise overstated), 4 parked with reasons (`docs/suggestions.md`). The ir oracle found **zero** DESIGN§5.1 ↔ `edge_type_ok` drift.

## Test state: ALL GREEN

`cargo test --workspace`: **423 passed, 0 failed** (178 syntax · 101 ir · 112 lower · 32 interp). fmt + clippy clean. No benches run this session (no perf-relevant change).

## Do next (ordered, smallest-first)

1. ~~Commit Session 09~~ done (docs tree, cleanups, leftovers, ADR-0018 zip/enumerate).
2. **`flow-check` design + implement** (unchanged from S08; the owed ledger is interp/DESIGN §9 + lower/DESIGN §12: Return exclusivity, E2 seq-context effect legality, full typing / E3 lifetime scope). Write `components/check/DESIGN.md` leading with `## Categorical model (Dat + Trn)`, flip its INDEX row, fill its stub `IMPLEMENTATION.md` rows in the same change (ADR-0017).
3. **Then P4 rewrites** — unblocked, oracle exists.
4. Session bookends now: `/category-architect start` to resume, `end` to close (writes next-session.md + an immutable sessions/ log).

## Open questions for Sapir

- ~~Pre-existing uncommitted work~~ resolved by Sapir: everything committed, stray `2` deleted. `examples/vector.flow` remains the one out-of-Core sketch (generics — waits on design-note candidates ②/③).
- **Carried:** RATIFY ADR-0016 (guard-first loops); IN6 float ÷0 ADR-0013 amendment; lower §16 OQ1–OQ8; ADR-0013 review; backend `TargetText` ADR (parked as suggestions #1, owned by P5 design).

## Gotchas / warnings (things that will waste the next session's time)

- **All S08 gotchas still stand** (guard-first loop driver ADR-0016; `body_order` degenerate-guard clause; interp deps; IEEE float compares; fueled loops; ledger no-relitigate rule). Read `sessions/` logs newest-first — next-session.md is the pointer, the logs are the record.
- `docs/components/<c>/IMPLEMENTATION.md` State columns are now ground truth for STATUS — a code change without its row update fails the reconcile gate (HANDOFF §7.2 step 7, ADR-0017).
- `LineIndex` is now `LineIndex<'a>` (borrows source) — new call sites must keep the source alive; don't "fix" it back to owned.
- `resolve_tykind` is the single `TyKind ⇀ Ty` skeleton; never re-fork the Named branch into the callers.
- The ir §5.1 golden oracle (`typing_table_golden`) must stay test-only and builder-free — sharing it into production paths collapses the independence property it exists to protect.

## Commands (build/test/bench invocations that currently work)

```sh
cargo test --workspace                                               # green — 423 (178+101+112+32)
cargo run -p flow-interp --example run -- examples/zip_demo.flow     # live zip/enumerate showcase
cargo test -p flow-ir typing_table_golden                            # §5.1 oracle
cargo run -p flow-lower --example dump_ir -- examples/fir.flow       # file → Category-IR Mermaid
cargo bench -p flow-interp --bench interp_scale                      # criterion interp bench
cargo bench -p flow-lower  --bench lower_scale                       # criterion lower bench
```
