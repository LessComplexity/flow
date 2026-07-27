# plan-s38 — trap order is source order (approach A, ratified)

Status: **SHIPPED (S38, 2026-07-27) — approach A landed; A′ priced and REFUTED.**
Built once in S37, measured, and reverted; landed in S38 with the perf re-run the plan asked for.
Component: `ir` (`topo_order`) · fallout in `backend-llvm`, `backend-cuda`, `rewrite`.
Origin: the S37 `open_inline` counterexample, pinned at `85b2243`.

**What landed** — `crates/mapal-ir/src/algo.rs`: the `topo_order` ready worklist became a
`BinaryHeap<Reverse<(loc.start, loc.end, insertion index)>>` instead of a FIFO `Vec`; and
`crates/mapal-rewrite/tests/testgen/mod.rs`: `const L = SourceLoc{0,0}` at 122 sites became a
per-`build` monotonic counter (4b). Gate **981 passed / 0 failed**, differential **37/37 in 403.93 s**
at `-O0`/`-O2`, `cargo fmt` clean.

**§7 step 1 answered: A′ costs MORE than A, not less.** Measured over 14 examples / 36 functions /
778 raw lowered objects: **62.2% of objects (484) change position** under the source-position key,
only **9 of 36 functions (25%) are already in source order**, and all nine are 3–9 objects; 902
pairwise inversions. The deviation is systematic, with three causes visible in the dump: an object's
`loc` is the **operator token**, not the sub-expression extent (in `(x > 0)` the `>` is `483..484`
and its operand `0` is `485..486`, and lowering creates the constant first), guard/loop result
objects carry a span that opens before their branch bodies (`abs`'s Phi is `488..543`, created last,
sorting before the `* -1` at `532`), and Parameter/Return both carry the whole function span. So A′ —
"make lowering create objects in source order" — means changing both the recursion order *and* what
`loc` each object carries, across all of lowering, churning every golden in the tree rather than the
19 A churns. **Do not re-price this.**

**The bug is wider than the plan described, and the wider form is the better demonstration.** §1 is
about trap *kind*; the same root cause also let a trap **swallow output written before it in source
order**, because `mapal_par_run_pinned` executes its task body synchronously on the host thread
(`mapal-rt/src/lib.rs:1080`) and the old order hoisted those runs ahead of the prints. Same program,
both exit 101:

```
PRE  (insertion-order tie-break):  mapal trap: div_zero
POST (source-position tie-break):  111 / 222 / mapal trap: div_zero
```

Invisible to the 1,280-run sweep by construction: `expect_native` maps `Outcome::Trapped(_)` to
`(None, 101)` and the stdout assert is guarded by `if let Some(want)`, so trapping runs compare only
the exit code. Now pinned by `backends/llvm/tests/differential.rs::
differential_trap_preserves_preceding_output`, verified as a real negative control (PRE fails it).
That test must pin its expectation as a **literal**: `interp::run` derives `output` from the
IoToken's accumulated log and only on `Done` (`mapal-interp/src/lib.rs:55`), so interpreted output is
a *value* that dies with the aborted computation while compiled output is a side effect that
survives — the two I/O models diverge exactly on the trap path, and `expect_native`'s `None` is
forced rather than lazy.

**Golden churn: 38 snapshots, all adjudicated** (35 + 25 subagent verifier/refuter pairs across three
rounds). 37 ordering-only; one real behaviour change (`example_calc`, both backends) signed off by
Sapir as the intended fix. The observable-effect axis is closed by proof, not inspection: the ordered
sequence of print/trap/run_pinned/par_check calls is unchanged in 34, and in 3
(`capture_one_kernel_matmul`, `example_vector_add`, `example_zip_demo`) a PRINT crosses a
`mapal_par_check` that is **provably a no-op** there — those modules contain zero `mapal_par_trap`
call sites, and the only production writer of `run.trap` is `mapal_par_trap` (init 0 at
`mapal-rt/src/lib.rs:586`, CAS at `:1000-1006`; the accesses at `:1271`/`:1278` are inside
`#[cfg(test)] mod tests`, which starts at `:1111`), so `check_trap`'s `if trap != 0` can never fire.

**Perf (§7 step 5): the pre-registration is refuted — emission order IS performance-relevant.**
i9-14900F, governor `performance`, 8 P-cores, 3 passes × 101 alternating runs, PRE = `main@d3ca82c`,
values byte-identical on all 7 shapes every pass. Using medians (saxpy's and gather's *minima* are
unusable — saxpy's min swung 0.40–0.63 ms between passes while its median held to 4 significant
figures): **saxpy 1t +5.2% / +5.3% / +5.3%** (three passes, the third a byte-identical rebuild, so it
is also the noise-floor control), **conv2d 1t −4.6% / −2.8% / −8.2%** (faster) with **par +4.1% /
+7.2% / +3.0%** (slower), **mm1024 1t +2.6% / +2.6% conformance but +0.15% on the FMA face** — the
regression is face-dependent, which is why the FMA leg was worth running. fir, reduce, transpose and
gather flat inside the ±1% noise floor. **Mechanism NOT isolated**, and deliberately not chased
(Sapir): both a `%Frame` member-order change (member order derives from graph object order, which
`replay.rs:1029` derives from `topo_order`) and a task-interleaving/locality change are consistent
with the data, and the latter fits conv2d's 1t-faster/par-slower sign flip better. Vector-instruction
counts are byte-identical pre/post (ymm/zmm 199/199 saxpy, 295/295 mm1024, 583/583 conv2d), so this
is scheduling, not degraded codegen — S36c's refuted `%Frame` alias-barrier claim stays refuted.
Incidental finding: `--contract` is a **no-op on 4 of 7 ladder shapes** (saxpy, reduce, transpose,
gather emit byte-identical IR in both faces) because contraction flags are applied only in tile
kernels and those four are not tile sites.

---

## 1. The bug

A randomised proptest draw found `PassId::Inline` changing what a program does:

```
Trapped(IndexOob)  !≈  Trapped(DivZero)
```

Shrunk program: `Call(helper)`, `Iota`, `Index{arr, idx:178}` (out of bounds), `FoldArr` whose body
divides by zero. Reproduced exactly, with the two walks side by side:

```
BEFORE:  Call, Iota, Index, Fold      → Index traps first → IndexOob
AFTER:   Iota, Fold, Index            → Fold  traps first → DivZero
```

`Index` and `Fold` are **independent** — the dataflow graph imposes no order between them at all.
`topo_order` breaks that tie on **object insertion order** (`algo.rs`, "ties broken by the order
objects were discovered"). Inlining removes the `Call` object and adds the body's, insertion order
reshuffles, and the tie flips. The rewriter changed the program's observable behaviour, which is the
one thing it may never do (`eval ∘ rewrite = eval`).

Pre-existing on `main`; nothing in the S37 work caused it. The seed is committed so it cannot pass
on a lucky draw.

**Note the 1,280-run differential is structurally blind to this class.** `mapal_trap` exits 101
whatever the kind, so both programs are exit-101 with stdout ignored; only the interpreter-level
property suite can see a trap *kind* change. That makes the randomised property tests the sole guard
here, and is an argument for keeping their draws random rather than pinning them down.

## 2. Why the repo's stated invariant did not hold

R-ORDER, `backend-cuda/plans/plan-minimal-emission.md`:

> **R-ORDER (effect/trap order):** statement order of Named/guarded/effect points is today's topo
> order restricted to those points; inlining never migrates a trapping or effectful op across a
> statement boundary. **Oracle trap order preserved by construction.**

The construction is what fails. "Topo order restricted to trapping points" is only equal to
statement order while insertion order happens to match statement order — and rewriting is precisely
the thing that breaks that coincidence.

## 3. The decision

**Traps are observable, therefore trap order must be a function of the PROGRAM, not of the
schedule.** The dataflow graph does not order two independent trapping ops, so the tie must break on
something intrinsic. Source position is the only such thing available.

This is S29's clock-read fence generalised: there, "the dataflow graph orders pure work against a
clock read not at all", and source order was the tie-break with meaning. Same reasoning, wider scope.

### 3.1 Why A and not a separate selection key

The system has **three** places that must agree on which trap is reported:

| site | mechanism today |
| --- | --- |
| the oracle | first trap reached walking `topo_order` |
| LLVM sequential / host spine | `mapal_trap` fired at the site, in emission order |
| LLVM parallel | `record_trap` → `mapal_par_trap(topo, kind)`, runtime CAS-**min on topo** |

They agree today **because all three derive from `topo_order`**. S24's speculate-and-order protocol
is already record-and-select — keyed on the topo index.

So changing only the interpreter to select by source position (the "B" proposal, considered and
rejected in-session) is unsound: the oracle would report the source-minimum trap while the compiled
binary reports the topo-minimum one, and they diverge exactly when the two orders differ. Making B
sound means changing the key in every backend as well, which creates a standing new invariant —
"oracle key == backend key", maintained across llvm, cuda and verilog forever — and requires the
interpreter to evaluate past a trap on dummy values, importing S24's soundness argument into the
definition of correctness itself.

**A keeps one order in the system.** Make `topo_order` source-respecting and the oracle, emission
order, and the runtime's CAS key all inherit it. No second mechanism, nothing to keep in sync.

## 4. The change

**4a. `topo_order` breaks ties on source position.** The ready-worklist orders by
`(loc.start, loc.end, raw key id)` rather than discovery order; the raw id keeps it total so the walk
stays deterministic when a desugaring emits several morphisms from one span.

**4b. testgen stops stamping every statement position zero.** `const L = SourceLoc { start: 0, end: 0 }`
at all 122 sites made source order carry no information, so the tie-break degenerated straight back
to insertion order — this is why a first attempt at 4a appeared to do nothing. A monotonic counter
gives "position order == the order testgen emitted them", which is the statement order these programs
model. Not a weakening of the test: generating programs whose statements all claim position 0 and
then asserting a rewrite preserves their outcome asks for a guarantee the language does not make.

**Verified in S37: with 4a + 4b, the counterexample passes** (`BEFORE = AFTER = Trapped(IndexOob)`),
`inline` is 15/15 green including the pinned seed, and the **1,280-run differential is 36/36 green at
`-O0`/`-O2`** — every value byte-identical.

## 5. What it costs, measured

- **19 golden failures / 18 pending snapshots** across `mapal-rewrite`, `backend-llvm`,
  `backend-cuda`, plus one CUDA assertion.
- The CUDA assertion is benign but instructive: arena zone offsets move from
  `o2@0, o3@256, o4@512` to `o2@0, o4@256, o3@512`. Still disjoint, copies still correct — the test
  pins an *ordering assumption*, not a correctness property.
- **The reach is wider than the bug.** That CUDA test uses `emit_src` on a **raw, unrewritten**
  graph and its offsets still moved, which proves lowering does not create objects in source-position
  order. So un-rewritten programs get all of the churn and none of the benefit.
- Emission reordering across matmul/conv/fir is **unmeasured**. It must be measured before landing —
  interleaved, ≥50 runs on the sub-millisecond cells, matmul as the negative control.

### 5.1 A′ — the variant worth pricing first

Make **lowering** create objects in source order, so `loc` order equals today's order for raw graphs.
Then 4a moves only *rewritten* programs, which is exactly where the bug lives, and the golden churn
shrinks to the rewritten goldens. Unscoped: nobody has measured how far lowering deviates. Price it
before choosing between A and A′.

## 6. Obligations this creates — STILL OPEN after S38

**A third one was found in S38 and is listed below as (3).** None of the three are closed by the
landing; all three carry to S39.



1. **Inlining must stamp spliced morphisms with the call-site position.** A callee's morphisms carry
   the *callee's* locs, which can sort earlier than the call site, so a trap inside an inlined body
   can still move. The pinned counterexample has an **empty** helper and therefore does not exercise
   this — the class is not closed by 4a alone. Needs its own counterexample: a helper whose body can
   trap, inlined into a caller with an earlier-positioned trap.
2. **`SourceLoc` stops being debug metadata and becomes a semantic attribute.** Once trap order is
   defined by it, every rewrite owes it a discipline exactly as it owes value-preservation. Deserves
   an ADR; testgen's all-zeros was a symptom of nothing having said so.

## 7. Steps

1. **Price A′** (§5.1): how far does lowering deviate from source order on the example corpus? Cheap
   to answer — compare `loc` order against insertion order per function. Decides A vs A′.
2. Land 4a + 4b. Gate on: the pinned counterexample, `inline` green, 1,280-run differential green.
3. Review the ~19 goldens **one at a time**; for each, confirm the change is ordering/numbering and
   the node and edge multisets are unchanged.
4. Update the CUDA arena assertion with the reason, not just the new offsets.
5. **Measure**: full ladder plus matmul, interleaved, min and median, ≥50 runs on sub-ms cells,
   1t and par. Pre-register that nothing should move; a change means emission order is
   performance-relevant and that is its own finding.
6. Write the ADR for §6.2, and close §6.1 with its own counterexample and fix.
