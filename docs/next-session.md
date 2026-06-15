# Next Session

Written: 2026-06-15 · end of Session 08 · by: Claude (Opus 4.8 1M — orchestrator + design author; dynamic workflow: 1 Opus implementer + 4 adversarial reviewers; Fable 5 unavailable)

## Where things stand (≤5 lines)

**M1 ORACLE ESTABLISHED — `flow-interp` is implemented and all six examples run correctly on CPU.** Implementing the loop driver exposed a **blocker in the (previously "review-hardened") interp/DESIGN §4**: the eager-both reading ("run the whole loop body, then test the guard") speculatively evaluated the continue-branch on the exit step, OOB-trapping `fir` at the exit state `k=4` (`coeffs[4]` on `[f32;4]`) instead of producing `5.375`. Resolved as a **semantics** decision via **ADR-0016 (guard-first loop evaluation)** — the continue-branch is the `inr(U)` arm of the Elgot step and is *not* evaluated on exit; the oracle holds the rule and all backends inherit it (differential-tested). DESIGN §4 rewritten guard-first (decide/exit cone → read guard → advance cone). Workspace **393 green** (366 prior + 27 interp), fmt+clippy clean, `interp_scale` bench recorded.

## Test state: ALL GREEN

`cargo test --workspace`: 393 passed, 0 failed (174 syntax · 92 ir · 100 lower · **27 interp** = 10 acceptance + 6 determinism + 3 divergence + 8 traps). `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` clean. `interp_scale` baseline: deep_loop 1k/10k = 323µs/3.20ms, large_map 1k/10k = 862µs/8.56ms (≈ linear).

## Do next (ordered, smallest-first)

1. **`flow-check` design + implement** (the next component; P3 finishes M1's checks). The interp `§9` assumptions are *exactly* check's owed ledger: (a) **Return exclusivity** (IR permits multiple full-value Return writers — ir I-RET; check guarantees exactly one fires), (b) **E2 seq-context effect legality** (no effects in parallel fanout; `print`/`println` only in sequential context), (c) **full typing / E3 lifetime scope**. Cross-ref: lower/DESIGN §12 ledger of owed checks + interp/DESIGN §9. Write `components/check/DESIGN.md` leading with its `## Categorical model (Dat + Trn)` (ADR-0014), flip its INDEX row to `modeled`.
2. **Then P4 rewrites** (`flow-rewrite`): layers 3–4 (const-fold, DCE, CSE) + layer-1 map-fusion; every pass property-tested *against the interp oracle* (random Core program × random inputs → interp-equal before/after). The oracle now exists — this is unblocked.
3. **`flow dump-ir --mermaid` as a real CLI** when `cli` gets its increment (the `dump_ir` example already exists).
4. Optional interp hardening (all minor, deferred under YAGNI; see interp/STATUS "known issues"): cache `derive_plan` per merge; assert the non-straddling-product invariant; the integer-overflow ADR (IN7).

## Open questions for Sapir

- **RATIFY ADR-0016 (guard-first loop evaluation).** Decided autonomously this session from your "pick the best place per the language's design" delegation, and implemented. It is a load-bearing **oracle semantics** decision that every backend must honor. Confirm: (a) the fix belongs in the oracle + ADR (not in `lower`); (b) `category-ir.md` (frozen Level A) needs no edit because E1/Elgot already implies it (ADR-0016 is the operational refinement) — so **no ERRATA entry**. If you'd rather the speculative-trap rule be enforced in `lower` (predicated ops) or pushed into `category-ir.md` text, that's a superseding ADR.
- **IN6 float ÷0** (IEEE, no trap) still wants a one-line ADR-0013 amendment (integer-trap / float-IEEE) before it is normative across backends. ADR-0016 *dissolves* the related `fir` worry (the OOB index is never semantically reached), but the bare float-÷0 question remains.
- **Carried over:** lower §16 OQ1–OQ8; ADR-0013 review (now load-bearing under ir + lower + interp); cross-builder id nonce (ir STATUS); the backend `TargetText` strategy-2-category ADR candidate (categorical-model §7).

## Gotchas / warnings (things that will waste the next session's time)

- **The loop driver is GUARD-FIRST now (ADR-0016), not eager-both.** `loops.rs` splits each iteration's `body_order` into a `decide_order` (backward-reachable from `exit_route` — builds the guard + exit payload + any exit-feeding print) and an `advance_order` (the next-state, where trapping ops like fir's `Index` live), evaluating advance **only when the guard continues**. Re-read DESIGN §4 before touching loops — this is the miscompile-prone part and the eager-both version is WRONG (traps fir).
- **`body_order` has a degenerate-guard clause** beyond pure SCC-incidence: it also includes any `Pair` edge whose target is the back/exit route object, so a **constant** guard/exit-value (e.g. the §11.3 constant-true divergence loop) still finalizes its route. No-op for the six examples.
- **interp depends on `flow-ir` + `slotmap` only** (env/buffers are `SecondaryMap<ObjectId,_>`; flow-ir does not re-export slotmap). `flow-syntax`/`flow-lower` are **dev-deps** (the `parse→lower→run` test pipeline). No `HashMap` anywhere (E2).
- **Float compares use native `<`/`<=`** (`num_lt`/`num_le`) — NOT `!Lt` for `Le`/`Ge` — so NaN ordering is IEEE (all false). The oracle is diffed against backends, so this must stay exactly IEEE (`nan_ordering_is_ieee` pins it).
- **Don't re-litigate the ledgers:** syntax W1–W25, ir D1–D10, lower LD1–LD25, **interp IN1–IN8 + ADR-0016**.
- **All loop tests are fueled (E1).** A hanging interp test is a protocol violation, not bad luck. `eval_call`/`run` take an explicit `budget`.
- The `countdown`/effectful-call goldens live in `crates/flow-lower/tests/golden.rs` as inline sources (interp reuses the `countdown` source string for token-through-loop); examples read live from `examples/`.
- **ADR-0014 dev-flow rule in force:** every `DESIGN.md` leads with `## Categorical model (Dat + Trn)`; the reconcile flips the `docs/architecture/INDEX.md` row in the same change (interp already `modeled`).

## Commands (build/test/bench invocations that currently work)

```sh
cargo test --workspace                                               # green — 393 (174 syntax + 92 ir + 100 lower + 27 interp)
cargo test -p flow-interp                                            # the oracle's 27 tests
cargo run -p flow-lower --example dump_ir -- examples/fir.flow       # file → Category-IR Mermaid
cargo bench -p flow-interp --bench interp_scale                      # criterion interp bench
cargo bench -p flow-lower  --bench lower_scale                       # criterion lower bench
cargo run -p flow-cli                                                # still the not-implemented stub, exits 1
```
