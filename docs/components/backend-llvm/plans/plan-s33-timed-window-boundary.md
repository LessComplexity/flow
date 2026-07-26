# plan-s33 — the timed window must not contain a materialisation

Status: **SHIPPED** (S33) — see §7 as-built. Component: `mapal-rt` (backend-llvm's runtime seam).
Driven by: `docs/performance/conv2d-per-core-gap.md` §7 — the diagnosis that closed the
S31/S32 conv2d gap as a measurement boundary, and Sapir's call to standardise on a
compute-only window by pre-faulting Mapal's output.

## 1. Categorical model (FRAMEWORK §2 + §4)

### Why model it

The defect is not in any algorithm; it is a **placement** fact, and only the `Loc`/`Trm`
half of the framework can state it. An arena block returned by `mapal_rt_alloc` is a
*virtual* range: the datum has no `DataLoc` in physical RAM until something touches it.
The kernel's first store is therefore doing two things at once — executing a `Trn` **and**
triggering the transmission that materialises its own output. `() -> time` brackets the
first; it cannot help also bracketing the second.

### Objects

| Object | Semantics |
| --- | --- |
| `Block` | one arena allocation — `(address, layout)`, as tracked in `ARENA` |
| `Bytes × Align` | the emitter's request |
| `VirtLoc` | a reserved address range; no physical page behind it |
| `RamLoc` | physical residence — the page the core actually writes |

### Morphisms

| Morphism | Signature | Partiality | Semantics |
| --- | --- | --- | --- |
| `alloc` | `Bytes × Align → Block` | Total | reserves the range, records it in `ARENA` |
| `reside` | `Block → Block` | **Total (new)** | forces physical residence — pays the fault-in |
| `dl_virt` | `Block → VirtLoc` | Total | where the range is reserved |
| `dl_ram?` | `Block → RamLoc` | **Partial today**, Total after | where the datum actually lives |
| `free_all` | `Block* → ()` | Total | unchanged |

`dl_ram?` being **partial** is the whole bug, stated exactly: today a `Block` may have no
RAM placement at the moment a transformation is placed against it.

### The transmission

The page fault is a `Trm`: `c_from = kernel zero-page source`, `c_to = RamLoc`,
`carries = Block`. It is real, it costs ~0.10 ms warm / 0.30 ms cold per 4 MB, and it
currently fires **inside** the region attributed to `Trn` execution.

### Coherence

This is **§4.5 Law 1, placement honesty**, in its literal §7.6 systems form: *no
transformation reads or writes data not present at its location.* `task7` is placed at a
core and writes `y`; `y` has no `RamLoc` until `task7` itself faults it in. The law is not
violated in the sense of producing a wrong answer — the kernel delivers the page
synchronously — but the transmission is **undeclared and unattributed**, which is exactly
what Law 1 exists to surface.

### Composition rule (the invariant this change installs)

> **CR-1.** `entry_alloc = reside ∘ alloc`.
> Every `Block` handed to emitted code has `dl_ram` total on return.
>
> **CR-2.** Therefore no `time`-bracketed region contains a materialisation `Trm`.
> A timed window measures `Trn` only.

CR-2 is the property the shapes benches have always claimed and never had, and it is what
makes them comparable to `shapes_baseline.cpp`, whose `std::vector<float> out(n)`
value-initializes — i.e. performs `reside` — before `run_iters`.

## 2. The change

One place. `mapal_main` emits **exactly one** arena call for the whole program:

```
%frame = call ptr @mapal_rt_alloc(i64 16810248, i64 8)   ; conv2d_1024, 16.8 MB
```

`entry_alloc` pushes into `self.allocas` (`func.rs:804`), so it lands in the entry-block
prologue ahead of every task — including both `time` reads. Making `mapal_rt_alloc` itself
`reside` therefore satisfies CR-1 and CR-2 for every heap-lowered program at once, with
**zero emitter change and zero mapal-ir change** (ADR-0032 clean: the runtime learns nothing
about the graph, the graph learns nothing about the machine).

In `crates/mapal-rt/src/lib.rs:mapal_rt_alloc`, after the null check:

```rust
// CR-1 (plan-s33): hand back a RESIDENT block. Fresh large allocations are
// mmap'd lazily, so the first store into them faults — and with the emitter
// allocating in the prologue but the tasks storing later, that fault lands
// inside whatever `() -> time` region happens to write first. One touch per
// page moves it here, where it belongs.
// ponytail: one byte per 4 KiB, not a memset — the fault's own zeroing IS the
// initialization, so memset would zero every page twice (15 ms on a 64 MB
// matmul frame). Ceiling: assumes >= 4 KiB pages; a 16 KiB-page host just
// touches 4x more often than it needs to, which is harmless.
let mut off = 0usize;
while off < layout.size() {
    unsafe { ptr.add(off).write_volatile(0) };
    off += 4096;
}
```

`write_volatile` so LLVM cannot delete the loop as dead stores.

Contract note for the doc comment above `mapal_rt_alloc`: the block is still documented as
uninitialized, and stays honest — every large block comes from a fresh zero-filled mapping,
so writing `0` observes and changes nothing. Consumers still write before they read.

### Rejected: pre-touch only the arrays the timed region writes

Sapir asked whether to touch the whole arena or just the variables the kernel is about to
use. **Whole arena.** The targeted form needs to know which arrays sit downstream of a
`time` read, which is graph knowledge, so the emitter would have to reason about the timed
region and emit selective pre-touch calls. That couples allocation to *measurement
instrumentation* — tuning the runtime to flatter the benchmark, which is the S30b framing
failure again.

CR-1 is deliberately stated with no mention of timers, benchmarks, or kernels: *an arena
block is resident when handed out.* The measurement fix is then a **consequence** of a
property that holds for every program, not the goal. (The one form of selectivity worth
revisiting is per-lane, and only for NUMA — see §5.)

### Rejected: memset, i.e. copying what `std::vector` does

The baseline's zero-fill is the wasteful version of `reside`. The kernel **already** zeroes
every page while faulting it (mandatory, or you would read another process's data), so a
memset writes zeros over zeros — 2× the memory traffic, ~15 ms of pure waste on a 64 MB
matmul frame. One byte per 4 KiB triggers the identical faults and lets the kernel's own
zeroing be the initialization. We match the baseline's *boundary*, not its method.

## 3. What this does and does not claim

- It does **not** make any program faster. The fault cost is paid either way; this decides
  *when*, and therefore what the clock sees. Total process time is unchanged (the one-byte
  touch is ~4k stores per 16 MB).
- It **does** make `benches/shapes/*.mapal` measure the same thing the baselines measure.
- Nothing goes in the README off the back of it until the re-measured rows exist.

## 4. Acceptance

| # | Check | Done when |
| --- | --- | --- |
| 1 | Page faults attributable to the conv map | `perf stat -e page-faults` differenced (full − genonly) drops **3 → 0** on the i9 |
| 2 | Mapal's window becomes compute-only | flow `iter ms` ≈ its `ref-cycles` time (0.150 ms), i.e. **0.258 → ~0.16** warm; the residual over ref-cycles is under 10% |
| 3 | conv2d verdict on a matched boundary | flow beats `cppb` 1t (0.1944) rather than losing to it |
| 4 | No value change anywhere | `cargo test --workspace --release` 72 suites green; llvm goldens byte-identical (this is runtime-only — **no `.ll` should move at all**) |
| 5 | Cold/warm spread narrows | flow cold/warm ratio falls from 1.27 toward cpp's 1.08, since the cold-fault cost leaves the window |
| 6 | Unit check | extend `arena_alloc_is_usable_and_freed`: a block ≥ 1 MB reads back as zero at page boundaries and is still writable/distinct/freed |

Check 4 is the one that makes this cheap to revert: if any `.ll` moves, the change leaked
out of the runtime seam.

## 5. Risks

| Risk | Handling |
| --- | --- |
| **NUMA first-touch** — on a multi-socket host, the thread that first touches a page decides which node's RAM backs it. Pre-touching the whole arena from the main thread puts **every** page on one node, where today a worker's own first touch would have distributed them | **Real, and unmeasured.** Irrelevant on the i9 and M4 (single socket), live on a dual-socket EPYC — i.e. exactly the vast.ai class of box. Ship it single-socket; before any multi-socket run, either measure a parallel leg A/B or make `reside` lane-aware (each worker touches the slice it will own, which is the NUMA-correct form of CR-1 anyway). Record as an open item, not a blocker |
| A program allocates a big block it never fully writes — now pays residence it used to skip | Accept. The emitter only heap-lowers arrays ≥ 256 KB that a map/fold fills. Recorded as the ceiling in the `ponytail:` comment |
| Pre-touch defeats a future `MAP_POPULATE`/huge-page path | None needed; `reside` is exactly the morphism such a path would implement more cheaply. CR-1 is stated on the morphism, not the mechanism |
| Frame is one 16.8 MB block, so `reside` touches arrays the timed region never uses | Correct and intended — CR-2 wants *every* materialisation out of *every* window |
| Read as a speedup | It is not one. Same pages, same kernel zeroing, same total bytes — moved earlier and ordered. Nothing about it belongs in the README |

## 7. As-built (S33)

Shipped as `reside` in `crates/mapal-rt/src/lib.rs`, called from `mapal_rt_alloc`. 5 lines of
loop plus its rationale. **No emitter change, no mapal-ir change, no `.ll` moved.**

### Acceptance results

| # | Check | Result |
| --- | --- | --- |
| 1 | Map-attributable page faults 3 → 0 | **CRITERION WITHDRAWN — it was ill-formed.** See below |
| 2 | Window becomes compute-only | ✅ i9 warm **0.2586 → 0.1440 ms**, against 0.150 ms predicted independently by `ref-cycles`. M4 **0.395–0.426 → 0.2111** |
| 3 | conv2d beats `cppb` 1t on a matched boundary | ✅ i9 **1.44×** ahead (0.1440 vs 0.2072); M4 **1.24×** ahead (0.2111 vs 0.2616) |
| 4 | No value change; gate green; no `.ll` moves | ✅ **72 suites, 0 failed**; `fmt` clean; `git diff` touches only `mapal-rt/src/lib.rs`; `golden_ll` suites pass unchanged; conv2d stdout `576/-96` byte-identical PRE vs POST |
| 5 | Cold/warm spread narrows toward cpp's 1.08 | ✅ **1.15 → 1.01** — flat, i.e. *better* than cpp |
| 6 | Unit check | ✅ `mapal-rt` 21/21, incl. the extended `arena_alloc_is_usable_and_freed` |
| + | `reside` must not add net cost | ✅ total process wall **1.485 → 1.498 ms** (+0.9%, the ~4k stores) |

### Why check 1 was withdrawn

It asked for `page-faults(full) − page-faults(genonly)` to fall 3 → 0. That difference does
not measure what the check wanted. It measures the faults caused by `y` **existing in the
frame at all** — and `reside` does not remove a single fault, it relocates *when* each one
fires. Worse, post-change the `genonly` control pre-faults its own frame too, and that frame
is 4 MB smaller (no `y` field), so the difference stays ~2–4 either way. Measured PRE 1,
POST 4, with the absolute counts drifting 243/244/247 run to run: pure noise around a
quantity that was never sensitive to the change.

**The observable that does capture it is check 2** — the window collapsing onto the
`ref-cycles` figure — and it did, to within 4%. A per-window fault count would need
instrumentation inside the timed region, which is not worth building for a property check 2
already pins. *Recording this because designing an acceptance criterion that cannot move is
the same class of error as the measurement boundary it was meant to verify.*

### Deviation: the staticlib is not rebuilt by `cargo test`

`cargo test --workspace --release` refreshes the rlib but leaves
`target/release/libmapal_rt.a` stale. The first M4 measurement therefore linked the **old**
runtime and read 0.396 ms — an apparent total non-effect. `cargo build -p mapal-rt --release`
is required before any hand-linked leg. `shapes_ab.sh` and `tile_ab.sh` already do this;
ad-hoc links must too. **A stale staticlib presents exactly as "the change did nothing."**

### Scope of the re-measurement debt this creates

The artifact scales as *output size ÷ kernel length*, so it is not uniform. M4, 1t,
min-of-30, recorded figure → post-fix figure:

| bench | recorded | post-fix | correction |
| --- | ---: | ---: | ---: |
| conv2d_1024 | 0.395–0.426 | **0.2111** | 1.87–2.02× |
| conv2d_512 | 0.083 | **0.052** | 1.60× |
| fir_65536 | 0.2156 | **0.104** | 2.07× |

Every one of these **understated Mapal**, so the corrections all move in Mapal's favor — which
is precisely why they must not be published until re-measured properly through the harness
rather than read off this table. matmul at large N is expected to be near-immune (its kernel
is ~20 ms against the same 4 MB output, so the boundary is well under 1%) but is **unverified**.

## 8. Not in scope

- Re-measuring and republishing the conv2d rows in `README.md` / `docs/performance/`.
  That follows once checks 1–3 pass, and is its own step.
- The M4 leg. The fix is architecture-independent by construction (it is a boundary, not a
  kernel), but check 1 needs `perf`, so the i9 is the gate and the M4 is a wall-clock
  confirmation only.
