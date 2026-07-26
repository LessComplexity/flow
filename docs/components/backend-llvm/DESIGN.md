# Component: backend-llvm — DESIGN

Written: 2026-07-17 · Session 12 · Status of this doc: increment 1 (P5 → M2) — authoritative for `crates/backends/llvm` (+ the shared `crates/mapal-rt` runtime it introduces per ADR-0020)
Spec authority: **ADR-0020 (backend emission contract)** > category-ir.md §8.1 (`F_LLVM`) + §8.5 (piecewise correctness) > ADR-0013 / ir/DESIGN (§5.1 typing table, §7 loop quartet D7, §8 tokens) > **ADR-0016** (guard-first loops — mirrored in CFG) > interp/DESIGN (the oracle: wrapping integers, traps, IEEE, render) > rewrite/DESIGN R1 (the differential equality relation) > architecture.md §4.1. DoD: HANDOFF §8 P5.

## Categorical model (Dat + Trn)

**Firewall.** Level B: these are the compiler's own types. The emitted `.ll` text *represents* the image of `F_LLVM : Mapal-Cat → LLVM-Cat` (Level A, category-ir §8.1); this section models only the emitter.

**Physical pair.** Emission itself is one in-process pass — degenerate, `Dat` + `Alg`. The physical pair becomes real only around the **emitted artifact**: the external toolchain (`clang`) and the running native process are genuine `Loc`s, and the differential harness's `stdout`/exit-code capture is the `Trm` carrying the observable back. The H↔D `Trm` with real content arrives in backend-cuda; here the physical story is confined to the harness boundary.

### Why (one paragraph)

Three payoffs. (1) **§8.5 piecewise correctness is the code shape**: `emit` walks `topo_order` translating one morphism at a time into instructions over per-object stack slots — the functor applied edge-by-edge; global correctness is then exactly the differential law `run_native ∘ emit ≈ run` (C-interp-2), discharged on examples + testgen programs, not argued. (2) **ADR-0016 becomes control flow**: the interp driver's decide/advance split maps 1:1 onto the loop CFG (header computes the decide cone, the conditional branch separates advance from exit) — the guard-first semantics is preserved *structurally*, not re-derived. (3) **The one-runtime seam (ADR-0020)** deletes the two classic differential-killers — float formatting and trap UB — by construction: every print and trap calls `mapal-rt`, whose text/exit behavior *is* the oracle's.

### Core category (Dat)

| Object | Kind | Role |
|---|---|---|
| `TargetText` | `𝕊` | one emitted `.ll` translation unit (ADR-0020: the String **is** the artifact) |
| `EmitError` | sum ⊕ | `Unsupported { feature: 𝕊, loc }` (capability ✋) ⊕ `Internal(𝕊)` — renderer-free (C3) |
| `Module` | product | skeleton under assembly: `mapal-rt` extern declarations, private `Str` globals, fn bodies, the `@main` wrapper |
| `FnCtx` | product | per-function emission state: `slot : ObjectId ⇀ 𝕊` (the object's alloca name; partial — token-erased objects have none), block name counter, current block |

### Passes (Trn)

| Pass | `t_from → t_to` | Notes |
|---|---|---|
| `emit` | `&CategoryIr → TargetText ⊕ EmitError` | ADR-0020 signature; deterministic (L2) |
| `emit_fn` | `(CategoryIr × FuncId) → 𝕊` | one LLVM internal function per `FuncDef` (incl. Map/Fold bodies) |
| `emit_morphism` | per `topo_order` step | §2 op table; the piecewise functor application |
| `emit_loop` | per canonical quartet | §3; ADR-0016 CFG |

### Tile ladder — strategy rungs at recorded sites (TrnLoc, §4.4; S25–S29)

A recognized `TileSite` (mapal-ir's record: geometry + per-read coefficients +
the S28 `ksplit` decomposition) carries ONE `t_from→t_to` contract — per-cell
op/operand/k-order exact (R1). Each rung is a parallel `TrnLoc` realisation of
that contract, selected by an **emitter-local predicate** cashing facts the
record already holds (the standing rung doctrine: rungs 2/3/B needed zero
mapal-ir change; A3 cashes the new `ksplit` morphism). Ladder direction and
per-backend width ownership: `docs/notes/tile-ladder-direction.md`.

| Rung | Gate (emitter-local) | Strategy (`TrnLoc`) | Placement / shape |
| --- | --- | --- | --- |
| 1 — SIMD lanes (S25) | site recognized | `emit_tiled_map` | TJ-wide lane micro-kernel, one acc vector, runtime-`tj` select per tile |
| 2 — TI register blocking (S26) | `site.rows > 1 && site.b.ci == 0` | `emit_tiled_map_blocked` | `acc [TI·TJ x elem]` (TI=4); ONE b load per (k, lane) shared across the TI rows — b is the invariant read |
| 3 — packing (S27) | `packing_site` = rung-2 gate ∧ `b.ksplit.is_none()` | `emit_pack_copy` + packed reads | b repacked j-tile-major (a `DataLoc` sibling); per-width TJ, k ×2-unroll + prefetch |
| B — 1-D window (S28) | `window1d_site` = `rows == 1 && b.ck == 1 && b.ksplit.is_none()` | `emit_tiled_map_blocked_1d` (+ `emit_tile_window_block`/`emit_tile_window_step`) | **the rung-2 DUAL** — TI blocks over the LANE axis; ONE scalar `a` load per k shared across the TI subrows (a invariant, b slides: matmul shares b across rows, FIR shares a across lane-blocks); `acc [TI·TJ x elem]`; k ×2-unrolled iff `K % 2 == 0`; full blocks unmasked; the `[lo,hi)` window needs no `[0,C)` clip (`rows == 1` collapses the row loop); remainder = the TI=1 `emit_tile_j_split` discipline (constant-TJ main + one runtime-`tj` tile). Non-window 1-D sites keep the rung-1 nest byte-for-byte (the negative control) |
| A3 — conv micro-kernel (S28) | `conv_site` = `a.ksplit.is_none() && b.ksplit.is_some() && b.ck == 0 && a.clane == 0 && b.clane == 1` | `emit_tiled_map_conv` (+ `emit_tile_conv_tile`, `ConvTileCtx`) | **the k-split decomposition constant-folded** — per (row, j-tile) the `(kq, kr)` tap nest fully unrolls (`kq in 0..K/div` outer, `kr in 0..div` inner IS k-ascending); per tap a constant-index `a` load and a `b` vector load at `b_row + (cq·kq + cr·kr) + j0 + lane`, the tap offset compile-time — div/mod vanish from the emission (zero `sdiv`/`srem`); rung-1 row idiom (slice row range + signed per-row jw clip), constant-TJ main + one runtime-`tj` remainder; TI=1 (row blocking a recorded ceiling) |
| KC — k-panel split (S29) · **BUILT, TESTED, DEFAULT OFF** | `EmitOpts::kc_nest` (opt-in) ∧ `packing_site` ∧ `site.k > TILE_KC` | `emit_tile_packed_kc` (replacing `emit_tile_packed_j_outer`; + `emit_tile_kc_{i_regions, boundary_row, boundary_tile, j_split, apack, trio, acc_lane, a_values}`) | the OpenBLAS **(jc, kc, ic)** order: j-blocks of `NC` lanes (`tile_nc_for` = TJ×32) outer → k-panels of `TILE_KC` = 128 (peeled `kc == 0`, then a loop whose last panel is runtime-short) → the rung-2 head/interior/tail i regions innermost; a's rows packed contiguous per (i-block, kc) into a `[TI·KC x elem]` 64-aligned scratch. **acc stays ONE j-tile wide** (`[TI·TJ x elem]`, the same as the j-outer nest): with `kc` outside `ib`, other i-blocks run between two panels of the same block, so nothing survives in scratch — **partial sums park in `out`**, spilled at every panel end and reloaded at the next (the peeled panel seeds instead). Value-preserving and per-cell k still ascending ⇒ bit-exact vs the j-outer nest (R1, differential-enforced). **Honest status:** the lever was sized on A-re-read traffic alone (C/TJ → C/NC) and priced no counterweight; measured on M4 Pro it is a **3× LOSS at 1024 f32**, so it ships default-OFF — a pure performance tailor in ADR-0032's sense, kept because it was designed against box-scale traffic (4096 on zen3) where it is still unmeasured. Numbers and the reasoning: `docs/performance/matmul/s29.md` §1; the open leg + the parking-free alternative: suggestions #16 |

Composition rules for the S28 branches (plan-s28 numbering kept):

3. A `ksplit.is_some()` site takes the conv branch or the untiled fallback —
   NEVER the affine tile path (the affine emission ignores `ksplit` and would
   compute wrong addresses; the `emit_map` site filter retargets accordingly,
   and `packing_site` refuses k-split sites — the panel layout has no encoding
   for `(k÷div, k%div)`).
4. Every new branch keeps the per-cell fold chain **k-ascending** — the R1
   bit-exactness invariant.
5. Never mask dead lanes/subrows: constant-TJ main tiles, runtime-`tj` only on
   remainder tiles; par split-range clipping unchanged.

The KC rung adds two of its own (plan-s29 numbering kept); rules 4 and 5 above
hold for it unchanged:

1. Only packed sites with `site.k > TILE_KC` take the jb/kc nest — shallower K,
   unpacked (`--no-pack`) sites, rung 1, window1d and conv are byte-for-byte
   unchanged (the negative controls), and the whole rung is behind an opt-in
   flag, so the default emission is byte-identical to S28's.
3. acc discipline: seeded in the peeled `kc == 0` panel, reloaded from `out` in
   every later panel, and stored back at EVERY panel end — j-remainder tiles
   inside a jb block included. (Amended from the plan's "stored at the last kc"
   when the nest order fixed it: the (jc, kc, ic) order cannot keep a partial
   sum resident, which is also why acc is one j-tile wide and not `TI×NC`.)

### Composition rules

- **L1 — oracle parity (ADR-0020 §3).** For every program the harness runs: `Done` ⟺ exit 0 ∧ stdout byte-equal to `RunResult.output`; `Trapped(_)` ⟺ exit 101 (⊥-identified per rewrite R1); classes never cross. Integer `Add/Sub/Mul/Neg` wrap (**no `nsw`** — the spec §8.1 `nsw` is illustrative; ADR-0020); `Div/Mod` zero-guard → `mapal_trap(div_zero)`, and signed `MIN / -1` guard → **Div result `MIN`, Mod result `0`** (parity with `wrapping_div` and `wrapping_rem` respectively — `x.wrapping_rem(-1)` is always 0; S13 review blocker); `Index`/`Update` bounds-guard → `mapal_trap(index_oob)` with **type-directed index extension** (u8 `zext`+`ult`, signed `sext`+two-sided — §2); floats IEEE at width; all text via `mapal-rt`.
- **L2 — determinism.** Byte-identical `.ll` for the same sealed IR; names derived from deterministic per-dump ordinals (`f{i}o{j}` scheme like Mermaid), never slotmap bits.
- **L3 — capability totality on Core.** Every §5.1 op emits; the ✋ row of the capability matrix stays empty for llvm **except** non-canonical loop shapes (multi-merge nested SCC) → `Unsupported { "nested loops" }` — same scope boundary as interp M1 and rewrite R6 (the whole toolchain is honest about the one degenerate shape; lifting it is one recorded increment across all three).
- **L4 — token erasure.** `IoToken` has no runtime representation; effect order is the token chain's topo order, already linear (ir I4). A token-bearing tuple materializes only its non-token components; `Print` emits a `mapal_rt` call at its topo position; token-only objects get no slot.

### Bridges

| Bridge | Signature | Stored? | Semantics |
|---|---|---|---|
| IR intake | `&CategoryIr` | borrowed | read-only; consumes raw or rewritten IR identically (ADR-0020 §4 tests both) |
| `mapal-rt` | extern C symbols | linked staticlib | the print/trap seam (ADR-0020 §2); built by cargo, linked by the harness/CLI at `clang` time |
| toolchain | `clang` subprocess | external | absent ⇒ differential tests skip-with-reason recorded in STATUS (HANDOFF §5.5); emission/golden tests never skip |
| testgen | `#[path]` include of `mapal-rewrite/tests/testgen` | test-only | random programs for the differential sweep — **default mode** (traps allowed: llvm implements traps deterministically), plus the examples |

---

## 0. Scope of increment 1 (P5 → M2)

In: `crates/mapal-rt` (ADR-0020 §2 — the shared runtime, workspace member); full Core emission per §2–§4 **including `Update` (ADR-0021, S13)**; the `@main` wrapper; golden `.ll` snapshots; the compile-and-run differential harness (examples + testgen incl. Update-bearing programs, raw + rewritten IR); the sepia-at-N perf baseline.

Out: nested-loop emission (L3 — `Unsupported`, one increment with interp/rewrite when needed); any LLVM optimization flags beyond `-O0` for differentials (a `-O2` differential row is a cheap later add); FFI/inkwell bindings (text emission per HANDOFF §5.5); WASM (spec §8.4, post-M5).

## 1. crates/mapal-rt (per ADR-0020 §2 — implemented in this increment, owned by this DESIGN)

`staticlib` + `rlib`; `#[no_mangle] pub extern "C"`:

```
mapal_print_i32(i32, bool)   mapal_print_i64(i64, bool)   mapal_print_u8(u8, bool)
mapal_print_bool(bool, bool) mapal_print_f32(f32, bool)   mapal_print_f64(f64, bool)
mapal_print_str(*const u8, usize, bool)                  // bool = newline
mapal_trap(u32) -> !          // 0 = div_zero, 1 = index_oob; stderr "flow trap: …"; exit(101)

// S29 additions
mapal_time_ms() -> f64        // the `time` builtin's clock read: ms since one process-lifetime
                             // monotonic epoch (`OnceLock<Instant>`), so two reads are
                             // non-decreasing and their difference is real elapsed ms
mapal_rt_alloc(i64 bytes, i64 align) -> *mut u8   // the heap-lowering arena (BL9); uninitialised,
mapal_rt_free_all()                               // like the `alloca` it replaces
```

Bodies: `print!("{v}")` / `println!` — Rust shortest-round-trip `Display` = interp `render` **by definition** (both call the same formatter; interp value.rs `render` is the reference; a unit test in mapal-rt pins a table of values incl. `4080.0 → "4080"`, `5.375`, `-0.0`, `NaN`, `inf` against `mapal_interp`-rendered strings). Stdout flushed on every call (differential reads pipes).

**ABI attributes (S13 review major):** rustc lowers `extern "C"` `u8`/`bool` params as `i8 zeroext`/`i1 zeroext` (AAPCS64 requires the caller to zero the high bits; SysV tolerates it). The emitter must carry `zeroext` on **every i8/i1-typed parameter** in the `declare` lines *and* every call site — that includes the trailing newline `i1` on all seven print externs and both params of `mapal_print_u8`/`mapal_print_bool`. A missing `zeroext` prints garbage for u8 > 127 on arm64 (sepia's Pixel channels) and the mapal-rt unit table cannot catch it — only the differential would. **(as-built S13)** In a *call argument* the attribute goes **after** the type (`call void @mapal_print_u8(i8 zeroext %v, i1 zeroext true)`); the attr-before-type form is invalid LLVM — this was the zeroext bug the u8 differential caught and fixed (`func.rs:emit_print`, `module.rs:print_call`). The `declare` lines keep the same after-type placement (`module.rs:RT_DECLS`).

## 2. Emission scheme — types, slots, ops

**Types.** `i32→i32, i64→i64, u8→i8` (unsigned ops), `f32→float, f64→double, bool→i1`, `Tuple/Struct→` literal `{…}` struct, `Array{T,n}→[n x T]`, `Unit→` erased (no slot), `IoToken→` erased (L4), `Str→` private unnamed global constant (only ever a `Print` operand — lower rejects strings-as-data, so Str never occurs inside aggregates).

**Erased representation rule (S13 review — position-agnostic; token may be packed first or last).** A product's *residual* is its component list minus `IoToken`/`Unit` components. Residual arity ≥ 2 ⇒ `{…}` struct, `Pair`/`Proj` = GEP store/load through the **remapped** index (skip erased components when counting). Residual arity 1 ⇒ the object materializes as the **bare** component type; `Pair` into it / `Proj` out of it = plain store/load (no GEP — a `{i32}` GEP against a bare `i32` slot is invalid LLVM). Residual arity 0 ⇒ no slot at all (token-only objects). The component→erased-index remap is **derived on demand from `object(id).ty`** (deduce-don't-store); `FnCtx` holds only the slot map.

**Slot scheme (mem2reg-friendly classic).** Every materialized object gets one `alloca` in the function's entry block; a morphism emission loads its operand slots, computes, stores its target slot. Products assemble in place: `Pair{slot k}` = GEP into the aggregate alloca + store (the staging buffer, made of memory); `Proj{k}` = GEP + load; `Index` = bounds-check then dynamic GEP + load. This makes every §5.1 row a local template and leaves optimization to LLVM (`-O0` for differentials; the perf baseline may also record `-O2`).

**(S29) Heap lowering — one placement swap under the slot scheme (BL9).** In the **entry** function only, an entry-block block of at least `HEAP_MIN_BYTES` (256 KB) is emitted as `call ptr @mapal_rt_alloc(bytes, align)` instead of `alloca`, released by a single `mapal_rt_free_all()` immediately before that function's `ret`. Everything above is unchanged: an `alloca` result and an arena block are both a `ptr`, so every `getelementptr`/load/store consumer is byte-identical, and the swap is invisible below the one seam (`func.rs:entry_alloc`). This is a `DataLoc` move, not a semantic one — the same `Dat` placed in the arena instead of the frame. The parallel flavor's block is the whole `%Frame` (`build_frame_layout` already packs every array into it, so the frame IS what blows the stack); the sequential flavor's are the per-object slots and the packed panel. Sizing is the emitter's own LLVM `StructLayout` walk over the closed emitted-type grammar (`func.rs:llt_bytes`), checked against `ptrtoint (ptr getelementptr (%Frame, ptr null, i32 1) to i64)`. The single teardown point sits past `mapal_par_finish` and past the return value's load, so "free after every reader" holds in both flavors; and because only the entry fn registers blocks, "free everything" is exactly "free mine".

**Op table** (the functor's morphism map; source = the operand aggregate per §5.1):

| op | LLVM |
|---|---|
| `Add/Sub/Mul` int | `add`/`sub`/`mul` (no `nsw`/`nuw` — wraps, L1) |
| `Div` int signed | `icmp eq 0` → trap-block; `icmp eq -1 && lhs == MIN` → result `MIN` (skip `sdiv`); else `sdiv` |
| `Mod` int signed | `icmp eq 0` → trap-block; `icmp eq -1 && lhs == MIN` → result `0` (skip `srem` — `wrapping_rem(MIN,-1) = 0`, NOT `MIN`; S13 review blocker); else `srem` |
| `Div/Mod` u8 | zero-guard → trap; `udiv`/`urem` |
| `Add..Mod, Neg` float | `fadd/fsub/fmul/fdiv/frem`, `fneg` |
| `Neg` int | `sub 0, x` (wraps) |
| `Eq/Neq/Lt/Gt/Le/Ge` int | `icmp eq/ne/slt/sgt/sle/sge` (`u…` for u8) |
| same, float | `fcmp oeq/une/olt/ogt/ole/oge` — matches Rust operator semantics on NaN (`==` false, `!=` **true**, orderings false) |
| `And/Or/Not` | `and`/`or` on i1, `xor i1 …, true` (strict — operands already computed, matching the oracle) |
| `Phi` | **`select`** — both branch values already in slots (strict Phi, no control flow; trap parity: branch cones always execute, exactly like the oracle) |
| `Pair/Proj` | GEP store / GEP load (token components erased per L4) |
| `Index` | **type-directed** range guard (S13 review — the oracle's `as_int` zero-extends u8): i32/i64 index ⇒ `sext` to i64, `icmp slt i, 0` ∨ `icmp sge i, n` → trap-block; u8 index ⇒ `zext` to i64, `icmp uge i, n` only (no lower bound — u8 ≥ 128 at i8 must NOT read as negative); then GEP + load |
| `Update` (ADR-0021) | same guard as `Index` (same trap, same extension rule); `llvm.memcpy` source array slot → target slot (distinct allocas — rebind mints a fresh object, slots never alias); dynamic GEP + store the element. Naive copy is the semantics; in-place elision via last-use is recorded headroom, not this increment |
| `Zip/Enumerate` | counted mini-loop writing the target aggregate (pair / `(i32, elem)` per ADR-0018) |
| `Call(g)` | direct `call` to the internal fn (aggregates by value — internal ABI, private linkage) |
| `Map/Fold{body}` | counted loop calling the body fn per element / with the accumulator threaded |
| `Print{newline}` | call the ty-matched `mapal_rt` extern at the edge's topo position (L4) |
| `TimeMs` **(S29, plan-time-builtin)** | `call double @mapal_time_ms()` + store. The source `IoToken` erases and the `(IoToken, f64)` target's residual has arity 1, so it materializes as the **bare** `double` (the §2 erased-representation rule) — the call result IS the target's value, no pair is built. Emission position in the block IS the ordering the token models (L4). The op is effectful: it is never rewritten, and a fn containing it stays attribute-free through the existing token rule |
| `Output` | load + store (the identity move) |

**Functions.** Every `FuncDef` → `define internal` with the lowered signature minus erased components; `main : IoToken → IoToken` → `define internal void @mapal_main()`. The public wrapper: `define i32 @main() { call void @mapal_main(); ret i32 0 }`. **(as-built S13)** Trap blocks are emitted **per-site inline**, not as one shared `trap_bb` per function per kind: `func.rs:trap_if` mints a fresh `trap`/`cont` label pair at each guard, the trap block calling `mapal_trap` + `unreachable`. Two extra blocks per guard, no shared join — simpler emission, `-O0` cleans it up; a shared block is unclaimed headroom, not a correctness point.

**(as-built S13) Index guard.** `func.rs:guard_index` emits the **two-sided signed compare on the zext'd i64** (`icmp slt i64 %i, 0` ∨ `icmp sge i64 %i, n`) for **every** index type, not the uge-only form §2's `Index` row specifies for u8. It is semantically identical: `load_index` already `zext`s a u8 (and `sext`s a signed index) to i64, and a zero-extended value is provably ≥ 0, so `slt 0` is dead-false for u8 and the two-sided check reduces to the same trap set as `uge`. One guard template covers both extension paths; the dead u8 lower-bound compare is recorded cleanup, not a bug.

## 3. Loops — ADR-0016 as CFG (the canonical quartet only, L3)

Per merge — recognized by the **same predicate the interp driver and rewrite use, extracted into `mapal-ir` as the one source of truth** (S13 review, BL7): exit/body attribution is *route-feeder-in-this-merge's-SCC, never per-fn union or reachability* — the rule whose two independent copies both regressed in S12. This increment exports the predicate from `mapal-ir` (e.g. a `loop_plan(f, merge)`-shaped accessor over `loop_structure`) and the emitter consumes it; migrating interp's `derive_plan` and rewrite's `is_canonical`/`exit_of` onto the export is attempted in the same change if mechanical (their S12 pins prove equivalence), else recorded as a suggestions row:

```
entry:   … store init → %merge.slot; br %header
header:  emit decide cone (cond + exit-route feeders, incl. exit-feeding Print)
         %c = load cond; br i1 %c, label %advance, label %exit
advance: emit advance cone (next-state); store next → %merge.slot; br %header
exit:    copy exit-route payload slot → exit object's slot; br %after
```

The decide cone runs **every** iteration including the exit one (countdown prints `0` — parity with interp §4); the advance cone is unreachable on the exit iteration (guard-first, ADR-0016); route objects are allocas assembled by their Pair stores inside the respective cones. Loop-invariant operands are already stored before `%header` — guaranteed by ir §13's **LoopEnter deferral** (the S12 topo fix; this DESIGN's emission order is the same `topo_order` walk, so the guarantee transfers verbatim). Multi-merge SCC ⇒ `EmitError::Unsupported` (L3).

## 4. Differential harness + perf baseline (tests/)

- `golden_ll.rs`: insta snapshots of emitted `.ll` for micro programs + the 10 in-Core examples (byte-stable, L2; snapshots read before accepting).
- `differential.rs`: for each of the 10 examples **and** *closed-mode* testgen programs (default mode — traps allowed; **open mode `i32 → i32` is excluded** — it has no native `@main` analog, its oracle observable is `eval_call`'s return with random args; S13 review), on **raw and `rewrite()`d IR**: emit → write tempdir → `clang <prog>.ll <libmapal_rt.a> -o prog` (`-O0`) → run **with a timeout** (belt-and-braces against a non-terminating generated loop; timeout ⇒ test failure naming the program) → compare per L1 (`Done` ⇒ exit 0 + stdout byte-equal; `Trapped` ⇒ exit 101, stdout ignored). Closed entry shapes and their wrappers (BL8): `main : IoToken → IoToken` ⇒ the §2 void wrapper, compared against `run()`; `main : Unit → i32` ⇒ `define i32 @main() { %r = call i32 @mapal_main(); call void @mapal_print_i32(i32 %r, i1 zeroext true); ret i32 0 }` — the result is *printed through mapal-rt* so the differential observes the value, not just a constant exit code. `clang` located via `which`/`CC`; absent ⇒ skip-with-reason recorded in STATUS (HANDOFF §5.5). `libmapal_rt.a` built once per test-run via `cargo build -p mapal-rt` (workspace target dir; serialize the build behind a `OnceLock` so parallel test binaries don't race).
- `perf_baseline.rs` (ignored-by-default long run): sepia-shaped synthetic (builder-generated map+fold over `[Pixel; N]`): wall-clock native (`-O0` and `-O2`) vs interp; numbers recorded in STATUS (HANDOFF §8 P5 "first perf baseline — sepia at N×N"). **(as-built S13)** Top N is **65536**, not 262144: the array is a literal of N `Pair` stores (Core has no array-fill), so N = 262144 is a ~1M-line module clang `-O2` needs >25 min CPU on (observed S13). 65536 times the same map+fold shape; an array-fill primitive / heap lowering (recorded headroom) restores 262144. The alloca-slot scheme also puts the whole `[Pixel; N]` in the frame, so the runner raises the stack via `ulimit -s hard` (`run_big_stack`) — the perf shape is the only place the 8 MB default is exceeded.

## 5. Module layout

```
crates/mapal-rt/src/lib.rs            # §1 (new workspace member, ADR-0020)
crates/backends/llvm/src/
  lib.rs        # emit, EmitError + curated pub use
  ty.rs         # Ty → LLVM type text, token-component erasure maps
  module.rs     # skeleton: externs, Str globals, main wrapper
  func.rs       # emit_fn: slots, topo walk, op table (§2)
  loops.rs      # §3 quartet CFG
crates/backends/llvm/tests/
  golden_ll.rs  differential.rs  perf_baseline.rs
```

Deps: `mapal-ir`; dev-deps: `mapal-syntax`, `mapal-lower`, `mapal-interp`, `mapal-rewrite` (+ its testgen via `#[path]`), `insta`, `tempfile`. No LLVM crates — text emission only (HANDOFF §5.5).

## 6. Test plan (what P5-green / M2 means)

1. mapal-rt render-parity unit table (§1) — incl. `-0.0`, `NaN`, `inf`, `4080.0`, `5.375`.
2. Golden `.ll` per example + micro shapes (loop CFG, trap guards, select-Phi, token erasure visible as absent slots, `Update` memcpy+store, **two sequential loops in one fn** — the S12 P0 shape, uncovered by the 10 examples; S13 review).
3. **Differential green on all 10 examples** (raw + rewritten) — the M2 line — plus closed-mode testgen sweep (≥ 256 cases; pool includes Update ops and multi-loop fns per plan-array-update U5), trap cases asserting exit 101, and a **two-sequential-loops compile-and-run differential**.
4. Determinism: emit twice → byte-equal.
5. `Unsupported` on the hand-built nested-loop graph (L3 pin).
6. Perf baseline recorded in STATUS (§4).

## 7. Decision ledger (BL1–BL9)

| id | decision | why |
|---|---|---|
| BL1 | Slot/alloca scheme, mem2reg left to LLVM | every op a local template; piecewise functor legible; `-O0` differential honesty |
| BL2 | `Phi` = `select`, strict | oracle evaluates both branch cones — control-flow Phi would *skip* a trapping untaken branch and break R1 parity |
| BL3 | Wrapping ints, no `nsw`; `MIN/-1` guarded to `MIN`; traps via `mapal_trap`/exit 101 | ADR-0020 §3; LLVM `sdiv` UB on both hazards |
| BL4 | Token fully erased; effect order = topo order of the token chain | ir I4 linearity makes the chain total order; no runtime token value exists to carry |
| BL5 | Internal ABI: aggregates by value, `internal` linkage, `@main` wrapper | private to the translation unit; no FFI surface beyond mapal-rt |
| BL6 | Nested loops `Unsupported` (with interp M1 + rewrite RW8 as one scope boundary) | one honest ceiling across the toolchain; lifted together or not at all |
| BL7 | Loop exit/body attribution predicate exported from `mapal-ir`, consumed here (interp/rewrite migrated if mechanical) | the per-merge-SCC rule regressed twice in S12 precisely because it lived in two hand-maintained copies; a third copy is how the next P0 ships (one-source-of-truth, FRAMEWORK §5) |
| BL8 | Differential sweep is closed-mode only; `Unit → i32` entry gets a result-printing wrapper; native runs time-boxed | open-mode has no native observable; a constant exit-0 wrapper would compare nothing; a hung native run must fail loudly, not hang the suite (S13 review) |
| BL9 **(S29)** | Heap lowering is **entry-function only**, threshold-gated at 256 KB, with ONE free-everything teardown before that fn's `ret` — not per-allocation lifetimes | the stack ceiling is a target fact (macOS caps the main thread at 64 MB hard), so the threshold is a backend emission constant beside `TILE_I`/`TILE_KC`, not a language one. Entry-only is what makes the arena sound without a free analysis: the entry prologue allocates a handful of times and never in a loop, whereas a Named fn or a Map/Fold body runs an unbounded number of times and would grow the arena without a last-use point the emitter does not compute. Consequence, recorded: a `matmul2048` with its kernel in a named fn still hits the stack wall; the all-in-`main` capture form does not. Upgrade path: `mapal_rt_free(ptr)` + `LastUsePlan`-driven free points |

## 8. Open questions (→ ADR candidates / later)

- `-O2` differential row (cheap, catches LLVM-level UB accidentally relied on) — add when differentials are green at `-O0`.
- `frem` vs Rust `%` parity for float `Mod` — pin with a differential case; if `frem` diverges, call `fmod` from mapal-rt instead.
- Nested-loop emission increment (with interp + rewrite, BL6).
