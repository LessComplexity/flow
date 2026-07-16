# Session 09 (part 2) — 2026-07-16 — CT-suggestion triage + apply

Immutable handoff log (ADR-0017). Follows [2026-07-16-init-category-architect.md](2026-07-16-init-category-architect.md)
in the same working session. Orchestrator: Claude (Fable 5); implementation: 4 Opus
agents + Opus adversarial reviewers via dynamic workflow; every diff then re-reviewed
line-by-line by the orchestrator (per Sapir's directive).

## Verdicts (8 suggestions from docs/suggestions.md)

**Applied (3):**

1. **lower** — `resolve_ty` / `TypeTable::resolve` duplicated `TyKind ⇀ Ty` skeleton
   consolidated into private `tys.rs:resolve_tykind`; single declared seam
   `resolve_named: (name, span, diags) → Option<Ty>` (build-time closure:
   cyclic/by_name/`resolve_struct`; post-build closure: `map.get`). L-codes, spans,
   messages, snapshots byte-identical; public API unchanged. ~70 duplicated lines gone.
2. **ir** — §5.1 typing-table golden oracle: in-module `#[cfg(test)]`
   `validate.rs::typing_table_golden::edge_type_ok_matches_design_5_1`. 85 rows
   hand-transcribed from DESIGN §5.1 across all ops, asserted against `edge_type_ok`.
   Test-only, builder-free fixture — two-realization independence preserved (DESIGN
   §11). **All rows agree: no DESIGN§5.1 ↔ validate drift exists today.** Scope note in
   module docstring: typing judgment only; graph-shape "extra conditions" (I5/I6/I8,
   token freedom) live in their own passes.
3. **syntax** — `LineIndex` dropped its owned whole-source `String`; now
   `LineIndex<'a>` borrowing the caller's `&str`. `line_col` body untouched
   (char-aware columns pinned by existing multibyte test). Mechanical lifetime
   updates: `tests/support/mod.rs` (TreeWriter already held `&'a str` beside the
   redundant copy), `flow-interp/examples/run.rs`, `flow-lower/examples/dump_ir.rs`.

**Refuted on vet (1):** interp numeric-width seam — the implementer STOPPED per the
vet-first instruction: only `num_lt`/`num_le` share the flat 5-way shape; `arith` is a
3-int+2-float split with different bodies (already seamed via `int_arith!`/
`float_arith!`); `as_int` is integer-only. A single four-op seam is not cleanly
achievable, and interp's own suggestions.md Detail had already adjudicated defer
(FRAMEWORK §5 "three similar lines beat a premature abstraction"; scalar set frozen for
M1, IN7). No change made. Roll-up row corrected to reflect this.

**Parked (4):** backend `TargetText` ADR (owned by P5 design time), lower `dfs_cycles`
util (YAGNI watch, third call site triggers), interp `derive_plan` threading (perf
store without profile evidence; S08 optional-hardening item), cli single-Diagnostic
contract (component unbuilt). Reasons recorded in docs/suggestions.md.

## Orchestrator review findings (post-agent, line-by-line)

- lower diff: seam equivalence verified arm-by-arm (closure = original Named branch
  verbatim; `n.span` threading correct; depth/L1208/L1000 moved not modified). Clean.
- syntax diff: no callers missed (workspace grep); `lib.rs` re-export unaffected. Clean.
- ir test module: `edge_type_ok` confirmed to read only `m.op`/`m.target` — the
  fixture's throwaway-minted `MorphismId` is safe; comment added saying why. Two
  coverage pins added by orchestrator: **"Eq str rejected"** (Str is printable but NOT
  comparable — A ∈ N ∪ {Bool} only) and **"Index u8 idx ok"** (I = any integer scalar,
  unsigned included). 83 → 85 rows.

## Test state: ALL GREEN

`cargo test --workspace`: **394 passed, 0 failed** (174 syntax + **93 ir** + 100 lower
+ 27 interp). `cargo fmt --check` clean; `cargo clippy --workspace --all-targets` clean.
No existing test modified; no asserted value changed.

## Docs reconciled in the same change

Component IMPLEMENTATION/suggestions/STATUS for lower, ir, syntax (agents; verified);
ir STATUS also had **pre-existing test-count drift corrected** (claimed 46/16/13,
actual was 48/18/14 before this session — later sessions had added tests without
updating STATUS). Roll-ups: docs/suggestions.md (applied → changelog, statuses),
docs/STATUS.md (ir 93, session-log row 09).

## Open items

Unchanged from part 1: flow-check next; ADR-0016 ratification; pre-existing
uncommitted work decision (spec Mermaid lint diff, nvim, VISION.md, generics examples,
stray `2` file). Nothing committed yet — Session 09's full change set awaits commit.

## Resume / inspect commands

```sh
cargo test --workspace                                    # 394 green
cargo test -p flow-ir typing_table_golden                 # the new §5.1 oracle
git diff crates/ HANDOFF.md docs/architecture/INDEX.md docs/components/ir/STATUS.md docs/STATUS.md
git status --short                                        # untracked docs tree + pre-existing leftovers
```
