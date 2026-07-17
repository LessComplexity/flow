# Plan — `seq` statement block (ADR-0019), cross-component increment

Authority: [ADR-0019](../../../decisions/ADR-0019-seq-statement-block.md) ·
Components touched: **syntax, lower, check** (+ spec LC-5, HANDOFF §4.1, examples,
global STATUS). **ir and interp are untouched** — seq has no IR footprint (ADR-0019
pin d): its ordering guarantee is the token thread that statement-order lowering
already produces.

## Categorical model of the change (Dat + Trn deltas)

Level B firewall (ADR-0014): parse-tree and pass deltas only; Level A semantics
unchanged (E2/ADR-0003 stand verbatim).

**`Dat` deltas (parse tree):**

| object | delta |
|---|---|
| `StageKind` | coproduct gains a summand: `SeqBlock(Block)` |
| `FanoutKind` | shrinks: `Plain \| Seq \| Void` → `Plain \| Void` (the `Seq` summand migrates to the new node) |
| `Block` | unchanged — reused as the seq body (statements + optional tail) |

**`Trn` deltas (morphisms):**

| morphism | delta |
|---|---|
| `parse_stage_body` (`KwSeq` arm) | retargets: `parse_fanout_block` → block production; emits `SeqBlock` |
| `parse_fanout_block` | non-`Chain` statements draw **P0117** instead of the silent `filter_map` drop (parser.rs:1940) |
| `emit_seq_block` (new, lower) | `(Block, seed) → subgraph`: statements via existing statement lowering **in the enclosing scope**, tail via `HeadlessSeed(seed)`; value = tail value; no-tail-but-continues → error |
| `emit_fanout` (lower) | dead `_kind` parameter deleted |
| effects walk (check) | `Fanout` node opens illegal-effect context unconditionally; `SeqBlock` arm recurses with sticky context (C-check-4 keyed on node kind, the natural reading) |

**Composition rules (pinned):**
1. `⟦seq { s₁; …; sₙ; t }⟧ = ⟦s₁⟧ ; … ; ⟦sₙ⟧ ; ⟦t⟧(seed)` — seq is the identity on
   lowering semantics; its content is grammatical (a block in stage position), not
   semantic. Effect order = token thread = statement order.
2. `effectful?(seq b) = ⋁ effectful?(stmts(b) ∪ tail(b))` — the composite rule that
   closes OQ-C1: an effectful seq in a `Plain` branch is an effectful morphism in a
   branch, T0201 by ADR-0003. CK5 upgrades pin → theorem.
3. Headless statements/tail seed from the seq input — today's bare-chain branch form
   parses unchanged and means the same (backward compatible by construction).
4. Bindings escape (enclosing scope) — the `fanout.flow` idiom
   (`… -> seq { -> f -> x; }  x -> g;`) holds for seq exactly as for fanout.

## Work packages (order: WP1 → {WP2 ∥ WP3} → WP4)

### WP1 — flow-syntax

1. AST: `StageKind::SeqBlock(Block)`; delete `FanoutKind::Seq` (compiler finds every
   match site — that is the point of the node split).
2. Parser: `KwSeq` arm parses a block (`parse_block_body(false)` — guard arms stay
   P0006 in seq bodies; read `parse_block` first and reuse whichever wrapper fits) →
   `SeqBlock`. Keyword/brace handling unchanged (ADR-0011/0012 gate untouched).
3. **P0117**: in `parse_fanout_block`, each dropped non-`Chain` statement draws
   `P0117` ("only chains are fanout branches; `x <- e`/`loop` statements do not
   belong in a fanout block") at the statement's span — replace the silent
   `filter_map` (parser.rs:1940). Diagnostic index + DESIGN catalogue row.
4. Tests: statement-form seq (headed chains, headless chains, rebind, loop, tail);
   empty `seq { }`; old bare-chain form (compat pin 3); guard arm in seq → P0006;
   rebind in a *Plain* fanout block → P0117; goldens updated where seq appears.
5. Docs same change: syntax/DESIGN.md §14.4 (stage classification; seq row moves out
   of the fanout section) + §15 shapes + diagnostic catalogue (P0117);
   IMPLEMENTATION.md rows (`SeqBlock`, P0117 site); STATUS.md counts.

### WP2 — flow-lower (after WP1 compiles)

1. Read `emit_block`/`BodyCtx` (emit.rs:553) and the guard-arm-block path
   (emit.rs:1985) first; reuse the fitting one for `emit_seq_block` — do not write a
   third statement-lowering loop.
2. `StageKind::SeqBlock` arm in the stage dispatch (next to Fanout, emit.rs:860):
   requires a wire (headless-chain error precedent), lowers statements in order **in
   the enclosing scope** (no child scope — pin 4), tail via
   `ChainCtx::HeadlessSeed(wire)`.
3. Value/continues logic mirrors `emit_fanout`: chain continues (or
   return-position) + no tail → error. Reuse `LCode::FanoutNoValue` if the message
   parameterizes cleanly; else next free code **L1611** (`SeqNoValue`). Decide
   reading the code, record in DESIGN either way.
4. `emit_fanout`: delete the `_kind` parameter.
5. Tests: golden IR for statement-form seq (two `println`s — assert the token thread
   orders them); seq mid-chain with tail (`data -> seq { … tail } -> f`); seq in
   return position with/without tail; rebind visible after seq (pin 4); effectful
   seq inside `Plain` fanout still rejected at lower's own effect walk (existing
   codes); empty seq.
6. Docs same change: lower/DESIGN.md (seq section rewritten off the fanout page;
   L-catalogue if L1611; morphism table), IMPLEMENTATION.md, STATUS.md.

### WP3 — flow-check (after WP1 compiles; parallel with WP2)

1. Effects walk (effects.rs:172): `Fanout` node opens the context unconditionally
   (`FanoutKind` is now `Plain | Void`; `Void` unreachable behind the parse-clean
   precondition — keep the existing note); new `SeqBlock` arm recurses statements +
   tail with **sticky** context (`in_fanout` carries through, exactly C-check-4).
2. Tests updated, same contracts: top-level `seq { print; print }` → clean;
   `seq { print }` inside a `Plain` branch → still T0201 (now by composition, not
   pin); *pure* seq inside a branch → clean (new — the loosening OQ-C1 asked about,
   free by construction); nested fanout/seq mixes unchanged.
3. Docs same change: check/DESIGN.md — C-check-4 rewritten node-kind-keyed; CK5
   rationale upgraded pin → theorem (cite ADR-0019); **§10 OQ-C1 struck** (answered);
   IMPLEMENTATION.md, STATUS.md.

### WP4 — spec, examples, roll-ups (after WP2 + WP3 green)

1. **LC-5** entry in `docs/spec/ERRATA.md` (ADR-0012/LC-3 precedent): user-guide §5.2
   prose + example patched to statement form:
   `data -> seq { "Step 1" -> log; "Step 2" -> log; "Step 3" -> log; }`.
   §8.6 (channels, full-language) untouched; §5.4 table unchanged.
2. `HANDOFF.md` §4.1 line 109 amended: `seq { … }` statement block for ordering.
3. Example: extend `examples/fanout.flow`'s effect epilogue **or** add a minimal
   `seq_demo.flow` (pure fanout → join → seq of ordered prints) — golden through
   parse→lower→interp (interp acceptance run proves the no-IR-delta claim end to
   end; expected output pinned in the header comment).
4. Global STATUS: ADR ledger row (ADR-0019); check row one-liner (OQ-C1 closed);
   capability matrix unchanged (seq already ✅ via interp).
5. `docs/next-session.md`: OQ-C1 struck from open questions; the S10
   `seq`-same-node-kind gotcha marked resolved by ADR-0019.

## Test matrix (minimum)

| Layer | Positive | Negative |
|---|---|---|
| syntax | stmt-form seq; bare-chain compat; empty seq; rebind/loop inside; tail | clean guard in seq → P0004; spaced/pattern arm mixed → P0006; rebind/loop in `void` fanout → P0117 |
| lower | token-ordered goldens; tail value; binding escapes; mid-chain seq | no-tail-but-continues; effectful-in-fanout unchanged |
| check | top-level seq clean; pure seq in branch clean | effectful seq in branch T0201; nested mixes |
| interp | seq example golden output (acceptance; no code change) | — |

## Reconcile checklist (HANDOFF §7.2 step 7 + ADR-0017)

- [x] syntax/lower/check DESIGN morphism tables + catalogues updated with the code
- [x] IMPLEMENTATION.md rows per touched crate (State=built)
- [x] STATUS.md per crate + global STATUS (counts, ADR-0019 ledger row)
- [x] ERRATA LC-5 + user-guide §5.2 patched; HANDOFF §4.1 amended
- [x] OQ-C1 struck in check/DESIGN §10 and next-session.md
- [x] FRAMEWORK §8 sweep: `FanoutKind::Seq` fully gone (no vestigial arms; grep
      verified — remaining mentions are historical prose); no new parallel objects;
      diagram⇔table

## As-built (Session 11) — deltas from the plan text above

Built by an orchestrated workflow (3 Opus implementers + 8 adversarial reviewers +
3 fixers), orchestrator line-by-line review after. Workspace green: 484 tests
(192 syntax · 101 ir · 128 lower · 29 check · 34 interp), fmt + clippy clean.
Deviations from the letter of the plan, each verified:

1. **Guard-arm diagnostics (WP1 item 2/4).** The plan's "P0006 in seq bodies" was
   partially wrong about mechanism: under the ordinary block production a *clean*
   guard token routes to stray-guard **P0004**; spaced/pattern arm forms draw
   **P0005/P0106**, and **P0006** fires only when such arms mix with statements.
   Both paths are regression-tested (`seq_guard_arm_illegal_p0004`,
   `seq_arm_mixed_with_stmt_p0006`). ADR Decision paragraph annotated as-built.
2. **P0117 fires on `void` blocks only.** After seq migrates out,
   `parse_fanout_block`'s sole caller is the `void` stage; a rebind/loop in a
   *Plain* fanout reclassifies the block to P0115 StmtBlock upstream, so the
   matrix's "Plain fanout → P0117" case is mechanically unreachable. P0117 is
   pushed **directly** onto the diagnostics list (the `self.diag` cooldown
   collapsed multi-drop reporting to one — review blocker, fixed) and covers
   Bind/Loop drops; Error/arm items keep their own codes (no double-report).
3. **Void-block tail kept.** The block tail (final chain without `;`) was silently
   discarded in `void { … }` — pre-existing silent drop found by review; the tail
   is now a branch (`fanout_block_unterminated_final_branch_kept`).
4. **L1611 `SeqNoValue` chosen** over parameterizing L1305 (distinct condition,
   fanout-specific name). A new `ChainCtx::RetValue` context makes the
   return-position L1611 guarantee hold for **effectful** fns too (the
   effectful tail previously lowered under `Statement` and fell through to
   L1306). Valued case regression-tested.
5. **Sub-pass walker sweep (WP2, beyond plan scope but demanded by pins b/e).**
   The Phi-arm scanner (`scan_chain`/L1404-05-08), the loop carried-set collector
   (`collect_assigns_chain`), and the capture check (`capture_chain`) now descend
   `SeqBlock` — **and `Fanout` branches, fixing three pre-existing live
   miscompiles** (effectful fanout in a Phi arm hoisted unconditionally;
   loop-carried `mut` reassigned in a fanout branch dropped from the carried
   state; capture-in-fanout misreported L1101). Each has a named regression;
   the seq-wrapped `sum_to_n` variant is pinned to 55 via the interp oracle.
6. **Example: new `examples/seq_demo.flow`** (not a fanout.flow extension) —
   pure fanout → join → seq of ordered prints, golden through
   parse→lower→check→interp (`36\n12\n`); the lower golden shows the ordering is
   carried by the IoToken thread alone, no seq node (pin d proven end-to-end).
