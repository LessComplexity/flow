# Next Session

Written: 2026-06-11 · end of Session 01 · by: Claude (Fable 5 orchestrator + Opus 4.8 workflow agents)

## Where things stand (≤5 lines)

M0 bootstrap is complete: Cargo workspace skeleton (10 crates, no deps, `cargo test` green), E1–E5 patched into the spec with markers, `docs/spec/ERRATA.md`, ADR-0001…0007, the docs/ status system instantiated, and six Flow-Core examples written as the acceptance surface. Every artifact was independently review-verified. No code beyond stubs exists yet.

## Test state: ALL GREEN

`cargo test --workspace` passes (zero tests — empty workspace compiles cleanly).

## Do next (ordered, smallest-first)

1. Read user-guide §3 (as patched: E4 statement rule, E5 `type` keyword) + ADR-0005, and ALSO read ADR-0008 — it imposes error-recovery + structured-diagnostics constraints on the parser design — then write `docs/components/syntax/DESIGN.md` for the lexer increment (token set, `SourceLoc` spans, error/recovery strategy, what the tests will assert).
2. Implement the lexer in `flow-syntax`: full Flow-Core token set incl. guard arrows (`-true->`, `-false->`, `-_->`, integer-literal guards), keywords (`type`, with `category` reserved-and-rejected per ADR-0006), spans from day one.
3. Golden tests: token streams for all six `examples/*.flow` (first external dev-dependency: `insta`).
4. If time remains: recursive-descent parser skeleton — flows are statements (ADR-0005).

## Open questions for Sapir

Both prior questions are RESOLVED: the E5 `category` → `type` rename is final (no veto, veto window closed 2026-06-11, ADR-0006); `flow-language-design.docx` has been added to `docs/spec/` (historical authority only; never edited).

## Gotchas / warnings (things that will waste the next session's time)

- `verilator` and `nvcc` are **not installed** on this machine; `clang` is. Backend-cuda/verilog differential tests must skip-with-reason (HANDOFF §5 item 5) — already noted in those components' STATUS.
- user-guide §7.3 shows `?` inside parallel fanout — interacts with E2 but is Core+1 territory; recorded as ERRATA **LC-1**, deferred to the Core+1 error-handling ADR. Do not resolve ad hoc.
- category-ir §2.6's monad table now has a divergence-monad row (E1 cross-reference fix); ADR-0002's spec-impact lists it.
- The examples are syntax-checked against the patched user-guide by review only — the parser is the real arbiter. Expect small example fixes during P1; record them (ERRATA or example header), never silently drift.
- Float print formatting is unpinned (sepia expects `4080` from an f32 total). Decide formatting when building the interpreter and record it in `interp`'s DESIGN.md.
- An `editors/nvim/` syntax plugin exists; update it when surface syntax changes (it is a tooling artifact, not normative).

## Commands (build/test/bench invocations that currently work)

```sh
cargo test --workspace   # green, zero tests
cargo run -p flow-cli    # prints not-implemented stub, exits 1
```
