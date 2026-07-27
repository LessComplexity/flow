# 2026-07-27 — S37: what `out[i]` is, and the array nobody read

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Opens the S37 block —
continues `2026-07-27-s36d-readme-editors-and-the-fma-question.md`.
Branch: **`s37-elem-plan`** (off `main` @ `c5f48c9`), 6 commits, **not pushed, not merged**.

Driven by Sapir: *"iota as an index law … can be a generic truth about the graph"* → *"zip is the
same logic as iota … it should be a generic notation of a graph fanout WITH A STRUCTURE"* →
*"both can be answered by the backend and not our end"* → *"query not rewrite — migrate tile_site to
the structure too"* → *"fix the empty task dispatch too … won't it be smarter to ensure such a task
never even compiles?"* → *"investigate the conv2d layout shift"* → *"include ms, and tell me what is
the baseline"* → *"fix the open_inline trap bug now"* → *"keep A for next sessions"*.

## 0. Continuation brief

Current state: **`elem_plan` ships — the compiler now knows what `out[i]` IS, and the biggest win of
the session came from deleting an array nobody read.** saxpy's timed window went **0.4769 → 0.0981 ms
at one thread** (4.86×) and **0.1860 → 0.0833 ms at the default width** (2.23×); matmul is flat at
every size, which is the control. Branch is 6 commits, gate green **except one pre-existing failure**
(`open_inline`, fails on `main` too, seed pinned).
Next step: **land approach A** — `docs/components/ir/plans/plan-s38-trap-order-is-source-order.md`.
It is built, measured and deliberately reverted; it needs its own session for 19 goldens + a perf
re-run. Price A′ first (§5.1 of that plan).
Resume command/check: `git log --oneline -6 && cargo test -q -p mapal-rewrite --release --test inline`

## 1. The idea, and whose it was

Sapir reframed S36c's "iota is an array, not an index law" from an emitter special case into a graph
law: *a stage carries a structure; composing stages composes structures.* Writing the element laws
out with their targets proves it is one notion, not five —

| stage | `out[i] =` | reads at `i` | carries a body? |
| --- | --- | ---: | --- |
| `Iota` | `i` | 0 | no |
| `Fill` | `x` | 0 | no |
| `Zip` | `(a[i], b[i])` | 2 | no |
| `Enumerate` | `(i, a[i])` | 1 | no |
| `Map{f}` | `f(a[i])` | 1 | **yes** |

Three things the repo already said, found while grounding it:

- **ADR-0018 calls `Zip` "the canonical iso `Aⁿ × Bⁿ ≅ (A×B)ⁿ`."** saxpy was materialising 8 MB to
  realise an isomorphism.
- **`Enumerate` needs no constructor**: it is `Pair(Index, ·)`, i.e. `enumerate a ≅ zip(iota n, a)`.
- **`Fill`'s count is already "type-carried … deduced, not stored"** — deduce-don't-store applied
  inside the op.

And the mechanical reason map fusion "fires on nothing": `analyze_map_fusion` bails on any producer
that is not a `Map` (`functor_laws.rs:98`), which is every one of the four bodyless ops.

## 2. What shipped

**`elem_plan`** (`93a0a7f`) — a deduced query, peer of `tile_plan`/`path_plan`. `ElemSrc` is
`Index` | `Broadcast` | `Load` (the cut) | `Pair` | `Apply`. Three guards: single in-edge, outside
every loop SCC, depth ≤ 16. Producers recognised by an **exact op-tag set**, not by "carries no
body" — trap-freedom is a documented guarantee of those four tags specifically.

Same commit: **`tile_iota_size` migrated off literal op-tag matching.** It asked
`op == Operation::Iota` — what the node is *tagged* — where it means to ask what the element *is*,
and `tile_site` calls it twice per site with a **silent** fallback to the scalar emitter. Landed with
a pin first (`tile_sites_pin.rs`, 15 sources), because losing a site is a 4.0× cliff
(`s25.md:46-48`) with no diagnostic. The pin surfaced a fact worth keeping: **raw = 0 everywhere —
no tile site is recognised before `rewrite`**, so the entire tiled path is downstream of
Inline/LiftLoops.

**Emitter consumption** (`891b56c`) — `emit_map`/`emit_fold` build the element from the law.

**`Apply` legality, declined here** (same commit) — `Map` as a producer behind a three-conjunct gate
(trap-free via `tile_trap_free`, loop-free, and an explicit `Print`/`TimeMs` check, because
`tile_trap_free`'s catch-all arm would admit them). **The CPU backend then refuses it, on
measurement**: enabling it put two extra calls inside saxpy's timed loop regenerating `x[i]` and
`y0[i]` from the index, for 0.72×. That is Table B working as designed — mapal-ir says legal, the
backend says not profitable, and a bandwidth-bound target should answer differently.

**Buffer elision** (`d111213`) — the win. Once every consumer rebuilds the element, the array is
write-only. saxpy's `zip` task wrote 8 MB per run that nothing read, **inside the `time` bracket**.
`%Frame` also lost the 4 MB iota: 12 MB total. Follows the `elided_updates` precedent; dropping the
frame *field* is the part DCE cannot do, because `%Frame` is one object shared across tasks.

**Empty-task registration** (same commit) — an elided producer's task registers `kind=0, n=1` instead
of `Split{n}`. Left as `Split`, the pool sliced a million-element range across every core to run a
zero-instruction function.

**Differential batching** (`6168863`) — unrelated to the above, and the second-biggest result.

## 3. The differential was 409 s because of macOS code signing

The sweep's cost was never the compile and never the cross product. Measured, 20 modules:

| | wall | CPU |
| --- | ---: | ---: |
| link only, 20× parallel | 0.26 s | 2.8 s (clang scales ~11×) |
| re-exec already-run binaries | 0.02 s | — |
| **first exec of 20 FRESH binaries** | **7.41 s** | **0.27 s** |

The work is code-signature validation in a system daemon *outside the process tree* — which is why
`user+sys` looked idle while the suite crawled, and why the existing fan-out to
`available_parallelism()` made it **slower**. 1,280 first-execs ≈ the whole 409 s. Linux has no such
daemon and the same total is dominated by `ld` (CI: 1,391 s of 1,475 s).

Fix: merge 32 cases into one translation unit, select with `argv[1]`. Emitted modules turn out to be
unusually easy to concatenate — no `target` lines, no numbered `attributes`, no metadata. What
collides is what a module *defines*; prefixing those and emitting the 24 shared `declare`s once makes
them disjoint. **Sweep 408.6 s → 15.1 s (27×), coverage identical** — same 320 programs × {raw,
rewritten} × {`-O0`, `-O2`} = the same 1,280 comparisons, 979 workspace tests either way.

## 4. Numbers

Baseline **`main` @ `c5f48c9`**; metric is each program's own printed `iter ms=` (the `time`-bracketed
window in its `.mapal` source); M4 Pro, `clang -O2 -march=native`, emitted `--rewrite`,
**conformance face — zero FMA, verified: 0 `contract` flags, 0 `fmla`/`fmadd`**; runs alternate
between the two binaries in one pass; ratio = baseline ÷ new.

### One thread

| shape | runs | baseline ms | new ms | ratio |
| --- | ---: | ---: | ---: | ---: |
| **saxpy_1048576** | 21 | **0.4769** | **0.0981** | **4.86×** |
| transpose_1024 | 21 | 0.8718 | 0.8187 | 1.06× |
| conv2d_1024 | 51 | 0.3247 | 0.3095 | 1.05× |
| gather_1048576 | 21 | 0.5398 | 0.5400 | 1.00× |
| fir_1048576 | 15 | 2.0608 | 2.0710 | 1.00× |
| reduce_1048576 | 21 | 0.5022 | 0.4993 | 1.01× |
| matmul1024_cap_f32 | 21 | 32.29 | 32.56 | 0.99× |

### Parallel (`MAPAL_PAR` unset = shipped default, 14 threads)

| shape | runs | baseline ms | new ms | ratio (min) | ratio (median) |
| --- | ---: | ---: | ---: | ---: | ---: |
| **saxpy_1048576** | 51 | **0.1860** | **0.0833** | **2.23×** | **2.27×** |
| transpose_1024 | 51 | 0.2239 | 0.2244 | 1.00× | 1.08× |
| gather_1048576 | 51 | 0.1208 | 0.1290 | 0.94× | 0.97× |
| fir_1048576 | 51 | 0.2925 | 0.3019 | 0.97× | 0.99× |
| fir_65536 | 51 | 0.0510 | 0.0545 | 0.94× | 1.05× |
| conv2d_1024 | 51 | 0.0917 | 0.0901 | 1.02× | 1.04× |
| conv2d_512 | 51 | 0.0436 | 0.0345 | 1.26× | 0.98× |
| reduce_1048576 | 21 | 0.5310 | 0.5423 | 0.98× | 1.01× |
| matmul512_cap_f32 | 21 | 0.5353 | 0.5455 | 0.98× | 0.95× |
| matmul1024_cap_f32 | 21 | 3.6835 | 3.6543 | 1.01× | 1.01× |
| matmul2048_cap_f32 | 9 | 29.082 | 28.604 | 1.02× | 1.00× |
| matmul4096_cap_f32 | 5 | 235.65 | 234.15 | 1.01× | 1.00× |

**saxpy is the only mover.** Everything else is inside the noise band for these cells — gather's own
baseline spans 0.1208 min to 0.1658 median, a 37% spread within one binary. **matmul is flat at every
size from 512 to 4096**, confirmed independently by emission: no tiled kernel changed in any file.

**Parallel gain (2.23×) is smaller than single-thread (4.86×), and that is expected** — the work
removed was a memory-bound store pass that parallelism was already spreading across cores. Deleting
work helps a serial run more.

### Emission reach

42 of 51 sources changed, total structural delta **291 lines**, every one the same shape: `-4 +2` per
generation leg (GEP+load → `trunc`), or the zip pattern. attn's alarming 3,343-line diff was
block-label renumbering cascading through one 2,500-line `mapal_main`; its real delta is `-6 +3`.
**No tiled kernel changed anywhere; no function added or removed in any file.**

## 5. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Stage composition as a generic structure, not per-op arms | **kept** (Sapir) | One law over `ElemSrc`; `Enumerate` needs no constructor at all, which is the proof it is one notion |
| `elem_plan` is a **query**, not a rewrite | **kept** (Sapir) | Recording that an array *could* be skipped leaves the elide-vs-materialise decision with the backend, and keeps S27 rung-3's deliberate packing expressible |
| No cost model in mapal-ir | **kept** (Sapir) | The backend has the graph and counts its own ops (ADR-0032) |
| Migrate `tile_site` to the structure | **kept** (Sapir) | Op-tag matching is the fragility the structure exists to remove |
| `Apply` inlined on CPU | **rejected on measurement** | 0.72× on saxpy — recompute loses to a load when the array is already materialised |
| Producer family = the four bodyless ops | **kept** | `Map` carries an arbitrary `FuncId`; it is a consumer unconditionally, a producer only behind a gate |
| Elide arrays with captured consumers | **deferred** | Following the `Pair` chain is more analysis than today's win justifies; fir/conv2d iotas keep their buffers |
| CUDA mirrors steps 2–3 | **rejected** (Sapir) | CUDA never got the six sessions of cache/register work; its version of this is smem staging + MMA, its own track |
| `%Frame` alias metadata (a standing P0 since S36c) | **refuted before implementation** | §6 |
| Trap order = source order, approach A | **ratified, deferred** | Built, measured, reverted; `plan-s38` |
| Trap order via a separate selection key (B) | **rejected as unsound** | §7 |
| Halve the differential cross product | **rejected — unnecessary** | The cost was code signing, not coverage; batching got 27× with the cross product intact |

## 6. A P0 refuted before it was built

S36c §3(a) claimed `%Frame` destroys alias analysis and costs 2.3× on saxpy. The plan was written,
then checked against emitted output. **It is false.** Three checks:

1. A hand-written control — three arrays as three fields of one struct, one `ptr` parameter —
   **vectorises with no metadata at all**. LLVM computes non-overlap from the constant field offsets.
2. Every task in 7 shapes compiled alone, 61 tasks: **exactly one** reports `unsafe dependent memory
   operations`, and it is saxpy's `Zip` task, whose output nothing reads. The other four failures are
   `cannot identify array bounds` (transpose's permuted read), `call instruction` (gather), and two
   tiled kernels.
3. **saxpy's timed loop already vectorises.** There was nothing to unblock.

The 2.3× came from a synthetic probe reproducing a pattern the compiler does not emit — the same way
S36c's 3.1× iota probe did not transfer to transpose. Recorded at `b96a062`; the disjointness
derivation and the `ptr_resident`/`packed` exclusions are kept for the day a *live* loop hits this.

## 7. The trap bug: diagnosed, fixed, reverted

`open_inline` (pre-existing on `main`) caught `Inline` turning `Trapped(IndexOob)` into
`Trapped(DivZero)`. `Index` and `Fold` are independent — the graph orders them not at all — and
`topo_order` breaks that tie on **object insertion order**, which rewriting reshuffles.

Approach **A** (source-position tie-break + testgen emitting real positions) **works**: counterexample
passes, `inline` 15/15, differential 36/36. Reverted because it churns **19 goldens across three
crates** and reorders emission for programs that were *never rewritten* — a CUDA test on a raw graph
had its arena offsets move, proving lowering does not create objects in source order.

**Approach B — selecting by source position only in the interpreter — was proposed by me and then
withdrawn as unsound.** `record_trap` passes `task_site(m)`, the **topo index**, and the runtime
CAS-mins on it (`func.rs:1307`, `mapal-rt:1002`): the parallel backend is *already* record-and-select,
keyed on topo. Changing only the oracle makes it report the source-minimum trap while the binary
reports the topo-minimum one. A keeps one order in the system; B creates a second and a standing
"oracle key == backend key" invariant across three backends.

Full write-up and the A′ variant: `components/ir/plans/plan-s38-trap-order-is-source-order.md`.

## 8. Live handoff state

| Type | Handle | State | Inspect |
| --- | --- | --- | --- |
| branch | **`s37-elem-plan`** @ 7 commits | clean, **NOT pushed, NOT merged to main** | `git log --oneline -8` |
| gate | full workspace | green **except pre-existing `open_inline`** | `cargo test --workspace --release --no-fail-fast` |
| pinned seed | `crates/mapal-rewrite/tests/inline.proptest-regressions` | committed, fails until s38 lands | `cargo test -p mapal-rewrite --release --test inline` |
| perf box | `<perf-box>` i9-14900F | untouched this session | `ssh … uptime` |
| worktree | `…/scratchpad/pre` @ `1daddaa` | still stale since S33 | `git worktree list` |
| scratch | `/tmp/cmp_base`, `/tmp/cmp_new`, `/tmp/hd_*`, `/tmp/bs_*` | baseline vs HEAD emissions + binaries | delete freely |

## 9. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | Trap order = source order (approach A) | `plan-s38-trap-order-is-source-order.md` | Price A′ first, then land 4a+4b, review 19 goldens one at a time, re-measure | `open_inline` green; differential green; ladder unmoved |
| **P0** | Inlining must stamp spliced morphisms with the call-site position | plan-s38 §6.1 | Needs its own counterexample: a trapping helper inlined into a caller with an earlier trap | the new counterexample passes |
| P1 | Oracle clones captured arrays per fold step | §10 | `Rc` + copy-on-write in `RValue::Array`; plan first, **forbid repo writes in the agent prompt** | `differential_tiled_matmul_kc_c540` well under 374 s |
| P1 | `SourceLoc` is semantic, not debug metadata | plan-s38 §6.2 | ADR | ADR exists and names the rewrite obligation |
| P1 | Branch is unmerged and unpushed | §8 | Sapir's call: fast-forward `main` or PR | `main` contains the 7 commits |
| P2 | Elide arrays with **captured** consumers | plan-s37-stage-structure | Follow the `Pair` chain; fir/conv2d `ts`/`kr` are 4 MB each | their buffers gone, ladder unmoved |
| P2 | Captured map bodies round-trip the argument through one reused `alloca` | §10 | `body_call_arg` builds the product in scratch every element — serialises gather's loop | pass in registers, measure |
| P2 | CUDA: smem staging + MMA | plan-s37-stage-structure §9 step 5 | Its own plan; **not** a port of steps 2–3 | — |
| P2 | Republish the ladder tables with these numbers | `docs/performance/shape-ladder-v2.md` | saxpy's cells moved 4.86×/2.23× | tables carry the new numbers |
| P3 | Stale S33 worktree | §8 | `git worktree remove --force` | one worktree listed |

## 10. Method notes earned

1. **Interleave, or do not report.** Three wrong conclusions this session came from measuring all of
   A then all of B. The *same binary* read 0.5646 ms and 0.4731 ms twenty minutes apart. A "1.50×"
   became 1.00× under interleaving, and a "3% min / 8% median regression, reproduced twice" on conv2d
   evaporated entirely at 51 runs. The repo's own S33 rule already says "ratios inside one run".
2. **No claim about a sub-10% difference on a sub-millisecond cell without ≥50 alternating runs.**
3. **Always report absolute ms and name the baseline commit** (Sapir). A bare "1.05×" is
   unfalsifiable and hides baseline drift.
4. **Say which face you measured.** Everything here is conformance — verified, 0 `fmla`.
5. **A probe reproduces a pattern, not the compiler's output.** Two S36c probe numbers — 2.3% on
   `%Frame` and 3.1× on iota — did not survive contact with emitted code. Check the emitted artifact
   before building on a probe.
6. **Legality without profitability is half a change.** `Apply` shipped as a pessimisation because
   mapal-ir's half was implemented and the backend's half was skipped — the exact split the plan
   specified.
7. **A declined sub-law must degrade, not fail.** Refusing a nested `Apply` returned `None` and
   collapsed the enclosing `Pair` back to an array-of-structs read, silently undoing the zip win. The
   differential was green through it; only re-reading the vectorisation report caught it.
8. **Reviewing emission for correctness is not reviewing it for desirability.** An accepted snapshot
   had baked in the `Apply` pessimisation.
9. **Subagents write into the repo unless forbidden.** Five `zz_*.rs` probe files under
   `crates/*/tests/`, one of which did not compile, and an agent mid-way through migrating
   `RValue::Array` to `Rc` in the working tree. Forbid repo writes in the prompt; `git status` before
   any gate that follows agent work.
10. **`git status` before `git add -A`** — S36d's own note, broken here: an unrelated proptest
    regression file rode into a feature commit and had to be split back out.
11. **zsh does not word-split unquoted variables.** `for s in $SRCS` silently iterated once over a
    52-element list and produced a comparison of one file.

## 11. Docs reconciled

| Doc | Change |
| --- | --- |
| `components/ir/plans/plan-s37-stage-structure.md` | new — the design, Tables A/B, preconditions, staged plan |
| `components/rewrite/plans/plan-s37-stage-composition.md` | marked SUPERSEDED with the four things it got wrong |
| `components/backend-llvm/plans/plan-s37-frame-alias-scopes.md` | new, then marked SUPERSEDED BY MEASUREMENT with the three checks |
| `components/ir/plans/plan-s38-trap-order-is-source-order.md` | new — approach A ratified, deferred |
| `architecture/deduced-queries.md` | new section "Where queries sit" — the pipeline, the two arrow kinds, correctness conditions, who-decides-what |
| this log, `docs/STATUS.md`, `docs/next-session.md` | — |

## 12. Files changed

`crates/mapal-ir/src/algo.rs` (`ElemSrc`/`ElemPlan`/`elem_plan`/`body_is_classifiable`;
`tile_iota_size` migrated), `crates/mapal-ir/src/lib.rs`, `crates/mapal-ir/tests/algos.rs` (7 tests),
`crates/backends/llvm/src/func.rs` (`emit_elem`, `emit_map`/`emit_fold` consumption, `APPLY_INLINE`,
`mark_elided_arrays`, frame-field skip, empty-task registration),
`crates/backends/llvm/tests/tile_sites_pin.rs` (new),
`crates/backends/llvm/tests/differential.rs` (batching), `crates/backends/llvm/tests/golden_ll.rs`
(heap-frame literals + the reason), 10 llvm snapshots,
`crates/mapal-rewrite/tests/inline.proptest-regressions` (new).
