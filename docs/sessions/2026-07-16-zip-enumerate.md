# Session 09 (part 3) — 2026-07-16 — ADR-0018: `zip` + `enumerate` builtins

Immutable handoff log (ADR-0017). Follows the same-day init + apply-suggestions logs.
Orchestrator: Claude (Fable 5) — design, ADR, plan, final line-by-line review;
implementation: sequenced Opus workflow (ir → lower ∥ interp → examples), 3 adversarial
reviewers, 1 fixer.

## What happened

1. **Design discussion with Sapir** (dynamic sizes, size-generics, execution-graph
   deduction incl. merge sort) → standing note
   `docs/notes/2026-07-16-sizes-generics-execution-graphs.md` (non-binding; four-tier
   size ladder, zip-as-natural-transformation, capture-as-broadcast, sorting networks).
2. Sapir directed: implement recommendation ① now. **ADR-0018 accepted** (scope change:
   Core Collections = map, fold, **zip**, **enumerate**; `iota` dropped — deduced
   `map π₀ ∘ enumerate`). Plan: `docs/components/ir/plans/plan-zip-enumerate.md`
   (typing table, oracle denotation, WP1–WP4, test matrix). HANDOFF §4.1 patched.
3. Built and landed; workspace **423 green** (178 syntax + 101 ir + 112 lower +
   32 interp), fmt + clippy clean; both examples verified live end-to-end.

## Decisions inside the increment

- **ir:** `Operation::Zip` (source = internal 2-tuple, Pair-then-primitive exactly as
  `binop`) + `Operation::Enumerate` (direct edge, like `Neg`). Enumerate bound
  `n ≤ i32::MAX` as `IrError::EnumerateIndexOverflow` with an independent
  `IrViolation` twin re-derived in `check_edges` (F4/SND-3 precedent);
  `edge_type_ok` arms stay pure typing. 13 typing-table-golden rows added in the
  same change. Proptest generator emits both ops (seal⇒validate-empty covers them).
  An oversize-enumerate graph is builder-unconstructible, so the validate twin is
  tested by hand-corrupting a sealed graph (in-crate privilege).
- **lower:** `is_collection_builtin` predicate (mirrors `is_print_builtin`; gates emit
  dispatch + both bare-name lookahead sites); `zip`/`enumerate` reserved (L1009). Five
  L-codes **L1606–L1610** (zip non-tuple / non-array / size mismatch; enumerate
  non-array / overflow) — emission owns diagnostics, builder re-checks defensively
  (LD12). `emit_zip` projects the tuple wire and the builder re-pairs — redundant
  proj/re-pair kept for the single-source contract; noted in DESIGN §8.9 as a P4
  fusion candidate. **Root-cause fix beyond scope (kept, reviewed):** D1 typing now
  seeds headless fanout branches with the scrutinee wire (`chain_seeded`) — a
  pre-existing gap that would mistype any tuple-array fanned into a `map`; emit
  already threaded the real wire, so this only brings D1's advisory typing in step.
- **interp (oracle-normative):** `Zip` = elementwise `Tuple[a[i], b[i]]`;
  `Enumerate` = `Tuple[I32(i as i32), x]`. Pure, total, no fuel subtlety.
- **examples:** `zip_demo.flow` = builtin showcase (zip + enumerate, historical
  header); `vector_add.flow` = zip form. Golden through all three suites; live
  outputs `c[0]=100 · c[15]=115 · sum=1720 · e[k]=2k`.

## Review trail

Adversarial reviewers: WP1 pass, WP3+WP4 pass; **WP2 fail → fixed** (reconcile miss:
global STATUS not updated in the same change; minor: no reserved-name regression test —
both resolved by the fixer). Orchestrator line-by-line review of every diff afterwards:
validate arms faithful to the plan table, `chain_seeded` verified as advisory-only
(suite-wide green, no duplicate diagnostics), routing complete at all three sites,
golden rows include the positive cases. Orchestrator repaired two remaining STATUS
gaps (syntax 174→178; missing ADR-0017 ledger row).

## Test state: ALL GREEN

`cargo test --workspace`: **423 passed, 0 failed**. `cargo fmt --check` clean;
`cargo clippy --workspace --all-targets` 0 warnings.
Verified live: `cargo run -p flow-interp --example run -- examples/zip_demo.flow`.

## Open items

- Backends will need `Zip`/`Enumerate` arms when built (capability-matrix row added:
  interp ✅, rest planned). The P4 rewrite laws for both ops are recorded in ir
  DESIGN (Future): naturality ×2, `map π₁ ∘ enumerate = id`, `iota` deduction.
- Carried: flow-check next; ADR-0016 ratification; ADR-0013 review; lower §16 OQs;
  design-note candidates ②–⑤ (size-generics, capture-as-broadcast/window, `[T;≤N]`,
  Vec/Stream split).

## Resume / inspect commands

```sh
cargo test --workspace                                                # 423 green
cargo run -p flow-interp --example run -- examples/zip_demo.flow      # live zip/enumerate
cargo test -p flow-ir typing_table_golden                             # §5.1 oracle incl. new rows
cargo test -p flow-lower --test rejection                             # L1606–L1610
```
