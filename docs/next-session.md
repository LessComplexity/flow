# Next Session

Written: 2026-06-12 · end of Session 04 · by: Claude (Fable 5 orchestrator + Opus 4.8 workflow agents)

## Where things stand (≤5 lines)

**flow-ir is complete and tested** (P2 first half). **ADR-0013 + ERRATA LC-4** resolve the spec's internal dataflow conflict (Pair-metadata/`rhs_const` vs the adjacency-only analyses) in favor of **edges-only**: per-slot `Pair{slot,arity}`, constants-as-objects, loops as inline SCC-visible cycles (no materialized `Trace`), IO as a **linear world token** with three pinned laws (signature synthesis `main : IoToken → IoToken`; token-sink I4b; token-in⇒token-out for loops). The sealed builder + an independent `validate()` enforce the I-ledger (`ir/DESIGN.md` §9); design got 3 adversarial reviews (26 findings applied), implementation got 2 reviews + a soundness attack (3 real breaches found and fixed with regressions: Str-param seal/validate gap, I5 route-vs-state SCC hole, u64→u32 arity truncation). `lower/DESIGN.md` §0.1 now holds **5 pinned lowering obligations** the IR golden tests already encode. The interpreter still does not exist.

## Test state: ALL GREEN

`cargo test --workspace`: 32 targets all ok, 0 failed. flow-ir 87 tests (46 unit rejection-matrix · 16 builder_rejections · 13 golden Mermaid, every snap hand-verified + linted · 4 proptests incl. headline "seal Ok ⇒ validate empty" @256 · 8 algos incl. 100k-chain J1 test). flow-syntax 174 unchanged. `cargo fmt --check` + `clippy -p flow-ir --all-targets` clean. Bench `ir_scale` recorded (chain 100k: build+seal 65ms / dump 69ms / sccs 7.9ms — near-linear).

## Do next (ordered, smallest-first)

1. **P2 second half: `flow-lower` design.** Write the real `docs/components/lower/DESIGN.md`: parse tree (`flow-syntax::ast`, syntax DESIGN §15) → `flow-ir` builder calls per category-ir §4 + the §0 obligations extract (re-verify, it's non-binding) + **§0.1's five pinned rules** (token synthesis; canonical ret-write `Dest::Ret` vs `output()`; negative-literal folding; right-folded value-guard Phi chains; loop exit reads merge-state view — `sum_to_n(10)` exits **55**). Key open design work: symbol table / `mut`-SSA discipline (each update = fresh object; back edge routes to merge), guard-arm classification (Phi §4.4 vs Trace routing §4.5 — an arm reaching `-> loop;` is routing, never Phi), loop-state packing (multiple `mut` vars → tuple U), Hole substitution (piped value = leftmost left operand), L-code diagnostics for lower-stage rejections.
2. **Implement `flow-lower`** with golden IR dumps for all six examples (the 13 ir goldens show the expected shapes — e.g. `golden_mermaid` (d′) is sum_to_n's loop; `cargo run -p flow-ir --example dump_demo` prints the pipeline-`f` and sum_to_n shapes ready to paste into a Mermaid renderer) + lex→parse→lower round-trip tests. The interpreter (P3) is next after that; differential tests wait for it.
3. If time remains: start `check`/`interp` DESIGN reading (interp pins float print formatting + multi-ret-writer exclusivity — both parked for it).

## Open questions for Sapir

- **ADR-0013 review** (accepted autonomously, revisable): the IR realization decisions — esp. (a) IO as linear world-token threading (vs a looser effect ordering), (b) trap semantics for div/mod-by-zero + OOB `Index` until Core+1 coproducts, (c) `Operation::Trace` not materialized. All argued in the ADR + `ir/DESIGN.md`; nothing downstream is built on them yet, so now is the cheap moment to veto.
- **Cross-builder id mixing is UB with no defense** (DESIGN §10, pinned by test): slotmap keys collide across `IrBuilder` instances; a foreign `FuncId` can seal+validate clean against the wrong callee. Fine while flow-lower is the only constructor — say the word if you want the builder-nonce ADR now rather than later.
- Carried over: P0115 anonymous-block stages; W15 unary binding (flag only if you want different).

## Gotchas / warnings (things that will waste the next session's time)

- **Don't re-litigate the ledgers:** syntax W1–W25, ir **D1–D10** (`ir/DESIGN.md` §18) + the I-invariant ledger (§9). In particular: loops are ALWAYS two routes (even `B = U`); `LoopBack` fires on **true**, `LoopExit` on **false**; exit reads the **merge-state view** (55, not 54/65); `Output` only for bare pre-existing `x -> ret`.
- **I5 checks the carried STATE's SCC membership, not the route's** — the route is always pulled into the SCC by its cond slot (review SND-1). If you touch `check_loops`, keep builder and validate copies in lock-step (they are deliberately independent code).
- **I9/I10 intake runs on synthesized tys too** (pack/binop/routes) — adding a builder primitive without the intake call reopens the Str-smuggling hole (review L2-04/F1).
- **Token rules are load-bearing for lower:** every print-bearing fn declares token-threaded (`main : IoToken → IoToken`), final token → Return; loop-carried tokens must exit via every `LoopExit` (`TokenNotEscaping` otherwise).
- **Snapshot discipline unchanged** (insta; read every .snap against the DESIGN — wrong-but-stable is the failure mode). Golden tests read `examples/*.flow` live — `git status examples/` before trusting (clean as of this commit).
- **J1 stands crate-wide:** all recursion in flow-ir is iterative/depth-guarded (Tarjan explicit stack, Ty walks bounded). New graph algorithms must follow; the 100k-chain test will catch you.
- A legal "two merges in one SCC" graph (Verilog-reject shape) cannot seed the inner loop from the outer merge (I5 rejects) — fuse via cross-feeding next-states from external seeds (see `tests/algos.rs` nested test).
- `verilator`/`nvcc` still not installed; `clang` is. LC-1 (`?` in fanout) still parked for the Core+1 error-handling ADR.

## Commands (build/test/bench invocations that currently work)

```sh
cargo test --workspace                          # green — 174 flow-syntax + 87 flow-ir + empty crates
cargo test -p flow-ir                           # full IR suite (<2s; proptests bounded)
cargo insta review                              # review pending snapshot changes (none pending)
cargo run -p flow-ir --example dump_demo        # hand-built pipeline + sum_to_n IR → Mermaid on stdout (see expected IR shapes)
cargo bench -p flow-ir --bench ir_scale         # criterion build/seal+dump+sccs bench (chains + grids)
cargo bench -p flow-syntax --bench lex_parse    # lex+parse bench (unchanged)
cargo run -p flow-cli                           # still the not-implemented stub, exits 1
```
