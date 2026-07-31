# Plan S45 — the move panel, deduced: delete the flag, keep the win

Status: **PLAN, written before the code** (FRAMEWORK §6.1); reconciled in §10 afterwards.
Machines: Apple M4 Pro (10P+4E; L1D 128 KB, 128 B lines, page 16 KB, L2 16 MB shared by **5**
P-cores) and an Intel i9-14900F (P-core L1D 48 KB/12-way/64 B lines, page 4 KB, **2 MB private L2
per P-core**, 36 MB L3). Worktree `agent-a5e299a1ffeefd714` off `1877b73`.
Governing records: `docs/performance/s44-conflict-not-capacity.md`,
`benches/results-s44/l1-micro-panel.md`, `benches/results-s44/i9-ladder.md`.
Results land in `benches/results-s45/deduced-move-panel.md` **the moment they are taken**.

## 0. The defect

S44 shipped `--move-panel=W:B`: two numbers typed by a human, applied globally to every eligible
map. `W` is program geometry; `B` is a machine constant. Both are exactly what this compiler's
premise says must not be typed — geometry comes from the graph, machine constants come from a
target profile. **Every input the rung needs is either a graph fact or a detectable machine fact.**

Four things have to be deduced, and they live in three different places:

| what | where it must live | why there |
| --- | --- | --- |
| `W`, the 2-D width, and the extent | **`mapal-ir`** — a fold-less move-site record | it is a property of the program. ADR-0032: the emitter must not re-derive graph analysis |
| line size, sets, ways, page | **`TargetProfile`** — detected | ADR-0032: `mapal-ir` never learns a machine fact |
| **whether to fire** | **the emitter**, at compile time | it is a *join* of the two, and joins belong where both are in scope |
| `B` | **the emitter**, derived | same |

## 1. The site — a graph fact (`mapal-ir`)

`tile_site` **hard-requires a fold** in the map body (`let fold_id = fold_id?`), and a transpose is
a pure permutation with no reduction, so no record carries its width. This adds the **fold-less arm
of the same family** — S44 §6 scoped it, this builds it.

```rust
pub struct MoveSite {          // GEOMETRY ONLY. No cache size, no line size, no threshold.
    pub width: u64,            // C, the divisor of the (t÷C, t%C) decomposition
    pub rows:  u64,            // extent / C
    pub cq:    u64,            // address coefficient on t÷C
    pub cr:    u64,            // address coefficient on t%C  <- the walk stride, in ELEMENTS
    pub elem:  Ty,             // the read array's element type
    pub len:   u64,            // the read array's length, in elements
}
pub struct MovePlan { pub sites: SecondaryMap<MorphismId, MoveSite> }
pub fn move_plan(&self, f: FuncId) -> MovePlan       // peer of `tile_plan`
```

Recognition, every clause reusing machinery that already exists:

1. `Operation::Map { body, captures ≥ 1 }`, mapped array an iota of size `n` (`tile_iota_size`),
   target array size `n`.
2. **No `Fold` anywhere in the body** — that is what makes this the arm `tile_site` cannot take,
   and it keeps the two recognizers disjoint by construction.
3. A `Div`/`Mod` pair on the mapped element by the same literal `C` — **`tile_split`, unchanged**,
   the identical call `tile_site` makes. `n % C == 0`, `rows = n/C ≥ 2`.
4. The body's output is a single `Index(array, addr)`; `array` is a map capture (`slot < captures`).
5. `addr` is affine in the derived axes: `base + cq·(t÷C) + cr·(t%C)` — a 25-line walker over
   Add / Mul-by-literal / literal, reusing `TileAffine::{add, scale}`.
6. `tile_trap_free(body, …, None)` — the permutation reorders visits, so a body that can trap could
   trap at a different element. Free for the shapes we run: `transpose_1024` emits with no
   `mapal_trap` call at all, i.e. the `Index` is already `bounds_proof`-proven.

`transpose_1024.mapal`'s `a[(t % 1024) * 1024 + t / 1024]` yields `width 1024, rows 1024, cq 1,
cr 1024, elem f32, len 2²⁰`.

## 2. The machine — detected facts (`TargetProfile`)

```rust
pub struct L1d { pub bytes: u64, pub line_bytes: u64, pub sets: u64 }
impl L1d { pub fn ways(&self) -> u64 { bytes / (line_bytes * sets) } }
pub l1d: Option<L1d>,     // None = this profile cannot answer, so the rung cannot fire
pub l2_cores: u64,        // how many PHYSICAL cores share `l2_bytes`
pub fn l2_per_core(&self) -> u64 { l2_bytes / l2_cores }
```

**Associativity is never recorded**: `ways = bytes / (line · sets)` is the *definition* of a
set-associative cache, so recording it could only make it wrong — the reason `f32_tiles` was
deleted. `sets` **is** recorded, because each host has a different best source and only the
detector knows which: **Linux reads `number_of_sets`** (the truth, exposed), **macOS derives
`page / line`** (nothing better is readable there).

That derivation is the VIPT no-alias bound, and it is stated as **a checked heuristic, not a law** —
it reads the *configured* page rather than the architectural minimum, and there are real parts it
gets wrong (a PIPT L1 larger than its page reach, e.g. Cortex-A53 32 KB 4-way: truth 128 sets, this
says 64; alias-handling VIPT designs, e.g. AMD K8 64 KB 2-way: truth 512, this says 64). Two things
keep it honest: it is used only where nothing better is readable, and where it is used it checks
out — M4 → 128 sets / 8-way (the brief's verified reading), i9 P → 64/12 and i9 E → 64/8, both
**matching `/sys/.../index0` exactly**. And the blast radius is bounded by construction:
`sets · ways ≡ bytes / line` however the split falls, so a mis-split can only mis-scale the gcd
collapse, never total capacity — a factor-two error moves `slots` by two, against a nearest
measured margin of pressure **21.3 vs 1**.

**`l2_bytes` is documented "per-core" and `apple-m` sets it to the 16 MB *shared* cluster L2.** That
ambiguity is a live defect the cost term would trip over, so `l2_cores` makes it explicit:
`hw.perflevel0.cpusperl2` = **5** on the M4 (⇒ 3.3554 MB per core), `shared_cpu_list` = `0-1` on the
i9 (⇒ 2 MB per core, against 36 MB of L3).

Detection: macOS `hw.cachelinesize` / `hw.pagesize` / `hw.perflevel0.l1dcachesize` /
`hw.perflevel0.cpusperl2`; Linux `/sys/devices/system/cpu/cpu0/cache/index0/{coherency_line_size,
size}`, `getconf PAGESIZE`, `index2/shared_cpu_list`. Named profiles stay the override and the
cross-compilation case — which is not optional here, because the i9 legs are **cross-compiled from
the Mac** and cannot read the i9's sysfs.

`GENERIC.l1d = None`, so the default profile cannot fire the rung and its 171-cell emission stays
byte-identical (rule 1). The rung fires under `--target=native` on the Mac and under a named
profile for the i9.

## 3. The decision — backend arithmetic

```text
S            = cr · sizeof(elem)                                   -- the walk's byte stride
sets_touched = line | S  ?  sets / gcd((S/line) mod sets, sets)  :  sets
slots        = sets_touched · ways
lines_live   = min(width, width · S / line)
pressure     = lines_live / slots
FIRE  iff   sets_touched < sets             -- CONFLICT, not capacity
      and   pressure > 1                     -- the sweep needs more lines than it can reach
      and   len · sizeof(elem) > l2_per_core  -- and a defeat is not free
```

**Three terms, none of them a fitted threshold, and each with its own measured witness.**

`pressure > 1` is the literal statement "the walk needs more lines than the reachable slots hold".
Its witness is side 128: a **real** set collapse (32 of 128 sets) that still measures 1.000×, which
is what rules out "a collapse is bad" and leaves only pressure.

`sets_touched < sets` is S44's headline — *conflict, not capacity* — as the gate it always was. A
walk that reaches every set is limited by capacity, and capacity is measured free on these parts
(S43: flat 32 KB → 8 MB). **This clause was added because the implementation's own test caught the
alternative firing on a measured loss**: side 1025 spreads over all 128 sets, scores pressure
**1.0009**, and S44 measured it running **2.12× faster unblocked than 1024** with blocking costing
0.623×. Without this term the deduction would have fired there — and a `pressure > 2` patch would
have been a constant fitted at exactly the boundary it needed to clear.

**The cost term, and why pressure alone cannot be the whole rule.** `pressure` predicts how OFTEN
the L1 is defeated, never what a defeat COSTS. i9 side 512 scores 21.3 and **loses 0.901×**
(replicated 0.907×). A defeat costs real money only when the array being re-swept does not fit the
**private** level below L1 — otherwise the miss is an L2 hit the out-of-order engine hides. The
read array is the right quantity, not the whole working set: the conflict is on `a`'s lines, while
the write stream is contiguous. That also keeps the discriminating case off a knife edge — 1 MB
against 2 MB rather than 2 MB against 2 MB.

**All six measured points, reproduced:**

| case | S/line | sets_touched | slots | lines_live | pressure | read | L2/core | verdict | measured |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| M4 1024 | 32 | 4 | 32 | 1024 | **32** | 4 MB | 3.36 MB | **FIRE** | 1.578× 1t / 1.993× thr |
| M4 2048 | 64 | 2 | 16 | 2048 | **128** | 16 MB | 3.36 MB | **FIRE** | 3.19× (probe) |
| M4 128 | 4 | 32 | 256 | 128 | **0.5** | 64 KB | 3.36 MB | decline (pressure) | 1.000× |
| i9 1024 | 64 | 1 | 12 | 1024 | **85.3** | 4 MB | 2 MB | **FIRE** | 2.646× 1t, disjoint |
| i9 2048 | 128 | 1 | 12 | 2048 | **170.7** | 16 MB | 2 MB | **FIRE** | 3.021× 1t, disjoint |
| **i9 512** | 32 | 2 | 24 | 512 | **21.3** | 1 MB | 2 MB | **decline (cost)** | **0.901× LOSS** |

## 4. `B` — the geometric mean of two measured costs

Two opposing multipliers, each one a measurement:

```text
traffic  T(B) = max(1, (line/sizeof) / B)   -- a block row shorter than a line refetches it
conflict C(B) = max(1, B / (slots/2))       -- read lines share the reachable sets with the writes
B = largest divisor of gcd(width, rows) <= sqrt((line/sizeof) * (slots/2))
```

Both are 1 inside `[floor, ceiling]`; that window is normally empty because the bounds pull apart,
and a product of two opposing multipliers is minimised at their geometric mean. The divisibility
clause is not rounding — the permutation needs the panel to tile the geometry both ways.

**The evidence for each bound.** Floor: B=8 on the i9 covers half a line and measures **15% slower
than B=16** at side 1024, 24% at 2048. Ceiling: B=`slots` on the M4 measures **29% (S44) / 34%
(S45) slower threaded** at side 1024 and **56%** at 2048.

| case | floor | ceiling | mean | **B** | measured optimum |
| --- | ---: | ---: | ---: | ---: | --- |
| M4 1024 | 32 | 16 | 22 | **16** | **16 — exact** |
| M4 2048 | 32 | 8 | 16 | **16** | **16 — exact** |
| i9 1024 / 2048 | 16 | 6 | 9 | **8** | 128 — 14% / 21% short |

**The i9 gap is not closable by any rule of this shape, and that is a measurement, not a
concession.** Counters on the box (side 1024, P-core): `off` and `B=128` take **1 053 802 vs
1 053 618** L1 misses — a 0.02% difference — and B=128 is **2.7x faster**; B=8 misses five times
less and is slower than both. dTLB misses are flat at ~290 across every arm and LLC misses are ~0.
So at that optimum the binding resource is **memory-level parallelism** (IPC 3.39 → 3.74 as misses
rise), which no quantity in `L1d` prices. Carried as a known gap with its size.

## 5. The flag: **demoted to an override, not deleted**

`EmitOpts::move_panel: MovePanel { Deduce (default) | Off | Force(w, b) }`.
`--move-panel=off` and `--move-panel=<W>:<B>`.

Deleting it outright would delete the ability to measure it: every A/B in this session needs an OFF
arm of the *same* binary, and the i9 B-sweep needs `Force`. `Deduce` is the default and the shipped
path; `Off`/`Force` exist for the harnesses and carry a doc comment saying so. **No machine constant
survives on the flag** — that was the defect, and `Force` is an experiment, not a configuration.

## 6. Placement (FRAMEWORK Dat/Trn/Loc/Trm)

* **Dat** `MoveSite` — a program object, in `mapal-ir`. **Dat** `L1d` — a machine object, in the
  backend profile. They never meet in either crate: only in the emitter, which has both in scope.
* **Trn** `move_plan : IR × FuncId → MovePlan` (deduced query, peer of `tile_plan`);
  `TargetProfile::move_block : MoveSite → Option<B>` (the join).
* **Loc** `func/bulk.rs::emit_map` — the one place the permutation is applied, unchanged from S44.
* **Trm** the emitted `.ll`.

`Sme::l1d_bytes` is left where it is and a test pins it equal to `l1d.bytes` wherever both exist.
Merging them is the tidier end state, but it routes `sme_kc` through a new `Option` and this session
has no business risking a working SME leg for a field rename. Recorded as the follow-up.

## 7. Gates, in order — values before timing, always

1. `cargo test --workspace --release` (expect **1037/0** before, ≥1037 after) and
   `cargo fmt --all --check`.
2. **Byte-identity**, `benches/emit_sweep_ab.sh`, 171 cells: `generic` before vs after → **0 diffs**
   (the default profile cannot reach the rung). `--target=native` before vs after → **exactly the
   transpose cells the arithmetic predicts move, and nothing else.** Every moved cell reported with
   its reason; every unmoved one too.
   The gate itself is verified to be able to fail (injected malformed source) **before** it is
   trusted — done, recorded in the results file §1.
3. Values: full stdout minus `iter ms=` byte-equal to OFF at every arm, every shape, both thread
   counts, on both machines. It is a permutation, so anything but equality is a bug.
4. Controls: the identity arm must overlap OFF; the saxpy null arm must not move.

## 8. Pre-registered predictions — declared before the timed runs

| # | prediction | what refutes it |
| --- | --- | --- |
| 1 | the rung fires at M4 1024/2048 and i9 1024/2048, declines at i9 512 and M4 128, **with no flag** | any cell whose deduced verdict contradicts the measured sign |
| 2 | deduced M4 B = 16, and it reproduces S44's 1.578× / 1.993× within noise | a deduced B that measures worse than the S44 hand-typed one |
| 3 | deduced i9 B = 8 gives ≈2.30× at 1 thread, ≈13% short of B=128 | B=8 measuring *better* than 128, or worse than ~2.0× |
| 4 | **M4 side 512 shows no pipeline win** (the cost term's discriminating prediction on this machine) | a disjoint win at any B — the cost term would be refuted on the M4 |
| 5 | `generic` emission is byte-identical; no non-transpose ladder shape moves | any diff outside the predicted cells |
| 6 | the threaded win still grows on the M4 and still shrinks on the i9 (S44 rule 24 + the i9 correction) | either direction reversing under the deduced B |

**Prediction 4 was tested first, before any of this code existed**, using the S44 flag — the one
prediction that could have killed the design. Result in the results file §3: every arm overlaps
OFF, min-to-min 1.02×. **Held.**

## 9. Work order

1. Machine facts on both boxes; gate verification; `--target=native` baseline. *(done)*
2. Predictive test of the cost term at M4 side 512 with the S44 flag. *(done — held)*
3. `mapal-ir`: `MoveSite`/`MovePlan`/`move_plan` + tests.
4. `TargetProfile`: `L1d`, `l2_cores`, detection, the i9 cross-compile profile + tests.
5. Emitter: `MovePanel` 3-state, `move_block` join, `emit_map` seam; update `tests/move_panel.rs`.
6. Full gate (§7).
7. M4 timed runs (1t and threaded, side 1024 + the ladder null shapes).
8. i9 cross-compile, ship, timed runs at 512/1024/2048, 1t and 32t.
9. Reconcile §8 line by line.

## 10. RECONCILIATION — scored against the measurements

Full tables in `benches/results-s45/deduced-move-panel.md`. Every §8 prediction, scored:

| # | prediction | outcome |
| --- | --- | --- |
| 1 | fires at M4 1024/2048 and i9 1024/2048, declines at i9 512 and M4 128, **no flag** | **HELD, every cell.** Read off the emitted text and confirmed by timing on both machines; the i9's 512 declined and every arm forced there **lost 0.85–0.90×** |
| 2 | deduced M4 B = 16, reproducing S44's 1.578× / 1.993× | **HELD.** B=16 derived; 1.548× at 1t and 2.350× threaded against S44's 1.578× / 1.993× |
| 3 | deduced i9 B = 8 ≈ 2.30× at 1 thread, ~13% short of B=128 | **HELD.** 2.226× disjoint; the gap is 14.2% at side 1024 (and 21.3% at 2048, larger than predicted) |
| 4 | **M4 side 2048: derived B=8 against the probe's 16** | **REFUTED as planned, then FIXED.** B=16 measured 15.7% better at 1t; the blocker pass replaced the ceiling-only rule with the geometric mean of the traffic and conflict bounds, which returns **16 at both M4 sides** and now picks the fastest arm at both thread counts (2.9786 ms 1t, 0.5535 ms threaded, both disjoint) |
| 5 | `generic` byte-identical; no non-transpose shape moves | **HELD.** 0 of 171 cells under `generic`; exactly 6 under `native`, all of them transpose 1024/2048 faces |
| 6 | the threaded win grows on the M4, shrinks on the i9 | **HELD.** M4 1.55× → 2.35×; i9 2.23× → 1.18× |
| — | **the cost term's decider** (M4 512, forced, run FIRST) | **HELD, and it selected between two hypotheses.** The rival — "a defeat costs iff the array overflows the *private* level below L1", which on the M4's shared L2 fires wherever pressure does — predicted a real win at M4 512. Measured: every arm overlaps OFF, min-to-min 1.02×, and every arm is *slower* threaded. The fair-share form ships **because it was measured**, not because it was preferred |

**One term was added after the plan, by the implementation's own test.** `move_block_reproduces_
both_machines` included side 1025, where S44 measured blocking at **0.623×** — and the planned
two-term rule **fired** there, at pressure 1.0009. The fix is §3's `sets_touched < sets`: conflict,
not capacity. A `pressure > 2` patch would have been a constant fitted at exactly the boundary it
needed to clear; the conflict clause is this session's own headline and carries its own witness.

**How P4 was fixed.** Removing the write term would have given B=16 at M4-2048 but B=32 at
M4-1024, which measures 34% worse threaded — the two sides disagree about `B = slots`. Keeping both
bounds and taking their geometric mean satisfies both sides at once, and needs no new constant.

**Two blockers were raised on review and both are closed.** (1) The i9 `fir` regression was
bisected to `vec_bytes` — `zen3` and `raptorlake` emit fir identically, so the cache facts were
never involved — and root-caused to `WINDOW_SUBROWS`, a hardcoded 4 whose own doc argued no
register budget applied. At `tile_j=32` that is 16 of AVX2's 16 registers. Deriving it from
`tile_i` (4 everywhere it was swept, 2 on a 16-register file) closes it: **1.8020 → 1.6892 ms,
level with `generic`**, byte-identical on every pre-S45 profile. (2) `B` above.

**Standing limitation, now measured rather than inferred:** the i9's optimum of **128** is bought
with memory-level parallelism, not cache residency — `off` and B=128 have identical L1 miss counts
and a 2.7x time difference. No L1-derived rule reaches it.
