# Next Session

Written: 2026-06-14 · end of Session 07 · by: Claude (Opus 4.8 — direct + dynamic-workflow review; Sonnet verifiers; Fable 5 unavailable)

## Where things stand (≤5 lines)

**`flow-interp` is fully DESIGNED and review-hardened, but NOT yet implemented.** `interp/DESIGN.md` (the oracle: the `RValue` value domain, the fueled `eval` `Trn`, an **incident-SCC loop driver**, token-as-Writer, `Outcome = Done | Diverged | Trapped`) leads with its ADR-0014 categorical-model section and survived a 6-dimension adversarial review (22 confirmed findings, **6 blockers** — headline: the loop driver originally read the exit payload from an **out-of-SCC route object** that was unwritten when read → would have miscompiled every loop example; fixed). **ADR-0015 (print/println split) is decided + implemented**: `print` raw, `println` appends `\n`, one IR op `Print { newline }`; examples use `println` for line output and `print` for `pipeline`'s label, so `pipeline → "f(10) = 25\n"`. Workspace **366 green**.

## Test state: ALL GREEN

`cargo test --workspace`: 366 passed, 0 failed (flow-syntax 174 · flow-ir 92 · flow-lower 100). `cargo fmt --check` + `cargo clippy --workspace --all-targets` clean. All example/token/tree/countdown snapshots regenerated for `println` and hand-verified as Print→Println-only. Benches unchanged.

## Do next (ordered, smallest-first)

1. **Implement `flow-interp`** strictly to `interp/DESIGN.md` (authoritative, review-hardened). Create `crates/flow-interp` (dep: `flow-ir`; dev-deps `flow-syntax` + `flow-lower` for the acceptance pipeline). Modules per §12: `value.rs` (`RValue`/`Outcome`/`TrapKind`/`render`), `eval.rs` (env + topo walk + product assembly; the §2 **incident-SCC skip rule**), `loops.rs` (the §4 `run_loop` — `body_order`/`back_route`/`exits` derived per §4; the **55 contract** = merge-view exit). TDD: write the six acceptance goldens first (`abs "7\n"`, `sum_to_n "55\n"`, `pipeline "f(10) = 25\n"`, `fanout "36\n12\n"`, `fir "5.375\n"`, `sepia "4080\n"`), then make them pass.
2. **Then** the contract/edge tests (§11): `sum_to_n(10)==55` / `fir==5.375` / `sepia fold==4080` **by execution**; fueled divergence (constant-true guard, §11.3); traps (hand-built div-0 / index-oob, §11.4); token-through-loop reusing **lower's committed `countdown` golden-h fixture** → `"5\n4\n3\n2\n1\n0\n"` (§11.5); determinism (§11.6).
3. **`flow-check` design** can follow/interleave: lower/DESIGN §12 ledger of owed checks + interp §9 assumptions = exactly what check owes (exclusivity, E2 seq-context, full typing/E3).
4. `flow dump-ir --mermaid` as a real CLI when `cli` gets its increment — the `dump_ir` *example* already exists (`cargo run -p flow-lower --example dump_ir -- <file>`).

## Open questions for Sapir

- **interp/DESIGN §14:** (a) **IN6 float ÷0** reads as IEEE (no trap), but ADR-0013 says "division by zero traps" unqualified — needs a one-line ADR-0013 amendment (integer-trap / float-IEEE) before it is normative across backends; (b) **multi-merge / multi-back-or-exit loop SCCs** are out of M1 (the driver errors on them); (c) **countdown shape** is informational — interp reuses lower's print-before-guard golden-h fixture (→ 6 lines), while user-guide §3.5 is a different guard-first shape.
- **Carried over:** lower §16 OQ1–OQ8; ADR-0013 review (now load-bearing under both lower AND interp); cross-builder id nonce (ir STATUS).

## Gotchas / warnings (things that will waste the next session's time)

- **The interp loop driver is the miscompile-prone part** — implement §4 exactly: the per-iteration body is morphisms **incident** to the SCC (`source∈SCC ∨ target∈SCC`), **not** "both endpoints in-SCC" (that drops fir's `coeffs[k]` invariant feed AND the out-of-SCC exit route); reset in-SCC/route buffers per iteration; attribute `exits(m)` by route-feeder-in-SCC, not by `in_edges(m)`. The review caught all of this — re-read §4 before coding.
- **`print`/`println` go through `is_print_builtin` (ADR-0015, LD25)** — `print` was special-cased in 9 effect/typing/emit sites; the helper exists because `println` silently regressed one (countdown lost its token-in-`U`). Don't re-special-case `"print"` anywhere; use the predicate.
- **Don't re-litigate the ledgers:** syntax W1–W25, ir D1–D10 + I-ledger, lower **LD1–LD25**, interp **IN1–IN8**.
- **interp must read the merge-state semantics from ir/DESIGN §7 (D7)**, not invent: LoopBack fires on true / LoopExit on false; exit payload = merge view (sum_to_n exits 55); I4 fork = back+exit.
- **Snapshot discipline** (insta, not cargo-insta): accept by `mv x.snap.new x.snap` AFTER reading; wrong-but-stable is the failure mode. Golden tests read `examples/*.flow` live — `git status examples/` before trusting (clean at this commit).
- The countdown/effectful-call goldens live in `crates/flow-lower/tests/golden.rs` as inline sources (not `examples/`) — DESIGN §9 regressions, don't promote.
- `verilator`/`nvcc` not installed; `clang` is. LC-1 (`?` in fanout) still parked.
- **ADR-0014 dev-flow rule in force:** every `DESIGN.md` leads with `## Categorical model (Dat + Trn)`; reconcile checks FRAMEWORK §8 and flips the `docs/architecture/INDEX.md` row in the same change.

## Commands (build/test/bench invocations that currently work)

```sh
cargo test --workspace                                               # green — 174 syntax + 92 ir + 100 lower (366)
cargo run -p flow-lower --example dump_ir -- examples/sum_to_n.flow  # file → Category-IR Mermaid (NEW, S07)
cargo run -p flow-ir --example dump_demo                             # hand-built IR shapes → Mermaid
cargo bench -p flow-lower --bench lower_scale                        # criterion lower bench
cargo run -p flow-cli                                                # still the not-implemented stub, exits 1
```
