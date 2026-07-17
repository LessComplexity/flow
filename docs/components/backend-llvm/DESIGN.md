# Component: backend-llvm — DESIGN

Written: 2026-07-17 · Session 12 · Status of this doc: increment 1 (P5 → M2) — authoritative for `crates/flow-backend-llvm` (+ the shared `crates/flow-rt` runtime it introduces per ADR-0020)
Spec authority: **ADR-0020 (backend emission contract)** > category-ir.md §8.1 (`F_LLVM`) + §8.5 (piecewise correctness) > ADR-0013 / ir/DESIGN (§5.1 typing table, §7 loop quartet D7, §8 tokens) > **ADR-0016** (guard-first loops — mirrored in CFG) > interp/DESIGN (the oracle: wrapping integers, traps, IEEE, render) > rewrite/DESIGN R1 (the differential equality relation) > architecture.md §4.1. DoD: HANDOFF §8 P5.

## Categorical model (Dat + Trn)

**Firewall.** Level B: these are the compiler's own types. The emitted `.ll` text *represents* the image of `F_LLVM : Flow-Cat → LLVM-Cat` (Level A, category-ir §8.1); this section models only the emitter.

**Physical pair.** Emission itself is one in-process pass — degenerate, `Dat` + `Alg`. The physical pair becomes real only around the **emitted artifact**: the external toolchain (`clang`) and the running native process are genuine `Loc`s, and the differential harness's `stdout`/exit-code capture is the `Trm` carrying the observable back. The H↔D `Trm` with real content arrives in backend-cuda; here the physical story is confined to the harness boundary.

### Why (one paragraph)

Three payoffs. (1) **§8.5 piecewise correctness is the code shape**: `emit` walks `topo_order` translating one morphism at a time into instructions over per-object stack slots — the functor applied edge-by-edge; global correctness is then exactly the differential law `run_native ∘ emit ≈ run` (C-interp-2), discharged on examples + testgen programs, not argued. (2) **ADR-0016 becomes control flow**: the interp driver's decide/advance split maps 1:1 onto the loop CFG (header computes the decide cone, the conditional branch separates advance from exit) — the guard-first semantics is preserved *structurally*, not re-derived. (3) **The one-runtime seam (ADR-0020)** deletes the two classic differential-killers — float formatting and trap UB — by construction: every print and trap calls `flow-rt`, whose text/exit behavior *is* the oracle's.

### Core category (Dat)

| Object | Kind | Role |
|---|---|---|
| `TargetText` | `𝕊` | one emitted `.ll` translation unit (ADR-0020: the String **is** the artifact) |
| `EmitError` | sum ⊕ | `Unsupported { feature: 𝕊, loc }` (capability ✋) ⊕ `Internal(𝕊)` — renderer-free (C3) |
| `Module` | product | skeleton under assembly: `flow-rt` extern declarations, private `Str` globals, fn bodies, the `@main` wrapper |
| `FnCtx` | product | per-function emission state: `slot : ObjectId ⇀ 𝕊` (the object's alloca name; partial — token-erased objects have none), block name counter, current block |

### Passes (Trn)

| Pass | `t_from → t_to` | Notes |
|---|---|---|
| `emit` | `&CategoryIr → TargetText ⊕ EmitError` | ADR-0020 signature; deterministic (L2) |
| `emit_fn` | `(CategoryIr × FuncId) → 𝕊` | one LLVM internal function per `FuncDef` (incl. Map/Fold bodies) |
| `emit_morphism` | per `topo_order` step | §2 op table; the piecewise functor application |
| `emit_loop` | per canonical quartet | §3; ADR-0016 CFG |

### Composition rules

- **L1 — oracle parity (ADR-0020 §3).** For every program the harness runs: `Done` ⟺ exit 0 ∧ stdout byte-equal to `RunResult.output`; `Trapped(_)` ⟺ exit 101 (⊥-identified per rewrite R1); classes never cross. Integer `Add/Sub/Mul/Neg` wrap (**no `nsw`** — the spec §8.1 `nsw` is illustrative; ADR-0020); `Div/Mod` zero-guard → `flow_trap(div_zero)`, and signed `MIN / -1` guard → result `MIN` (parity with `wrapping_div/rem`); `Index` bounds-guard → `flow_trap(index_oob)`; floats IEEE at width; all text via `flow-rt`.
- **L2 — determinism.** Byte-identical `.ll` for the same sealed IR; names derived from deterministic per-dump ordinals (`f{i}o{j}` scheme like Mermaid), never slotmap bits.
- **L3 — capability totality on Core.** Every §5.1 op emits; the ✋ row of the capability matrix stays empty for llvm **except** non-canonical loop shapes (multi-merge nested SCC) → `Unsupported { "nested loops" }` — same scope boundary as interp M1 and rewrite R6 (the whole toolchain is honest about the one degenerate shape; lifting it is one recorded increment across all three).
- **L4 — token erasure.** `IoToken` has no runtime representation; effect order is the token chain's topo order, already linear (ir I4). A token-bearing tuple materializes only its non-token components; `Print` emits a `flow_rt` call at its topo position; token-only objects get no slot.

### Bridges

| Bridge | Signature | Stored? | Semantics |
|---|---|---|---|
| IR intake | `&CategoryIr` | borrowed | read-only; consumes raw or rewritten IR identically (ADR-0020 §4 tests both) |
| `flow-rt` | extern C symbols | linked staticlib | the print/trap seam (ADR-0020 §2); built by cargo, linked by the harness/CLI at `clang` time |
| toolchain | `clang` subprocess | external | absent ⇒ differential tests skip-with-reason recorded in STATUS (HANDOFF §5.5); emission/golden tests never skip |
| testgen | `#[path]` include of `flow-rewrite/tests/testgen` | test-only | random programs for the differential sweep — **default mode** (traps allowed: llvm implements traps deterministically), plus the examples |

---

## 0. Scope of increment 1 (P5 → M2)

In: `crates/flow-rt` (ADR-0020 §2 — the shared runtime, workspace member); full Core emission per §2–§4; the `@main` wrapper; golden `.ll` snapshots; the compile-and-run differential harness (examples + testgen, raw + rewritten IR); the sepia-at-N perf baseline.

Out: nested-loop emission (L3 — `Unsupported`, one increment with interp/rewrite when needed); any LLVM optimization flags beyond `-O0` for differentials (a `-O2` differential row is a cheap later add); FFI/inkwell bindings (text emission per HANDOFF §5.5); WASM (spec §8.4, post-M5).

## 1. crates/flow-rt (per ADR-0020 §2 — implemented in this increment, owned by this DESIGN)

`staticlib` + `rlib`; `#[no_mangle] pub extern "C"`:

```
flow_print_i32(i32, bool)   flow_print_i64(i64, bool)   flow_print_u8(u8, bool)
flow_print_bool(bool, bool) flow_print_f32(f32, bool)   flow_print_f64(f64, bool)
flow_print_str(*const u8, usize, bool)                  // bool = newline
flow_trap(u32) -> !          // 0 = div_zero, 1 = index_oob; stderr "flow trap: …"; exit(101)
```

Bodies: `print!("{v}")` / `println!` — Rust shortest-round-trip `Display` = interp `render` **by definition** (both call the same formatter; interp value.rs `render` is the reference; a unit test in flow-rt pins a table of values incl. `4080.0 → "4080"`, `5.375`, `-0.0`, `NaN`, `inf` against `flow_interp`-rendered strings). Stdout flushed on every call (differential reads pipes).

## 2. Emission scheme — types, slots, ops

**Types.** `i32→i32, i64→i64, u8→i8` (unsigned ops), `f32→float, f64→double, bool→i1`, `Tuple/Struct→` literal `{…}` struct, `Array{T,n}→[n x T]`, `Unit→` erased (no slot), `IoToken→` erased (L4), `Str→` private unnamed global constant (only ever a `Print` operand). Token-bearing products erase their token components — a `(IoToken, i32)` object materializes as `i32` (component index remapping recorded in `FnCtx`).

**Slot scheme (mem2reg-friendly classic).** Every materialized object gets one `alloca` in the function's entry block; a morphism emission loads its operand slots, computes, stores its target slot. Products assemble in place: `Pair{slot k}` = GEP into the aggregate alloca + store (the staging buffer, made of memory); `Proj{k}` = GEP + load; `Index` = bounds-check then dynamic GEP + load. This makes every §5.1 row a local template and leaves optimization to LLVM (`-O0` for differentials; the perf baseline may also record `-O2`).

**Op table** (the functor's morphism map; source = the operand aggregate per §5.1):

| op | LLVM |
|---|---|
| `Add/Sub/Mul` int | `add`/`sub`/`mul` (no `nsw`/`nuw` — wraps, L1) |
| `Div/Mod` int signed | `icmp eq 0` → trap-block; `icmp eq -1 && lhs == MIN` → result `MIN` (skip div); else `sdiv`/`srem` |
| `Div/Mod` u8 | zero-guard → trap; `udiv`/`urem` |
| `Add..Mod, Neg` float | `fadd/fsub/fmul/fdiv/frem`, `fneg` |
| `Neg` int | `sub 0, x` (wraps) |
| `Eq/Neq/Lt/Gt/Le/Ge` int | `icmp eq/ne/slt/sgt/sle/sge` (`u…` for u8) |
| same, float | `fcmp oeq/une/olt/ogt/ole/oge` — matches Rust operator semantics on NaN (`==` false, `!=` **true**, orderings false) |
| `And/Or/Not` | `and`/`or` on i1, `xor i1 …, true` (strict — operands already computed, matching the oracle) |
| `Phi` | **`select`** — both branch values already in slots (strict Phi, no control flow; trap parity: branch cones always execute, exactly like the oracle) |
| `Pair/Proj` | GEP store / GEP load (token components erased per L4) |
| `Index` | `icmp` range guard (0 ≤ i < n, signed) → trap-block; GEP + load |
| `Zip/Enumerate` | counted mini-loop writing the target aggregate (pair / `(i32, elem)` per ADR-0018) |
| `Call(g)` | direct `call` to the internal fn (aggregates by value — internal ABI, private linkage) |
| `Map/Fold{body}` | counted loop calling the body fn per element / with the accumulator threaded |
| `Print{newline}` | call the ty-matched `flow_rt` extern at the edge's topo position (L4) |
| `Output` | load + store (the identity move) |

**Functions.** Every `FuncDef` → `define internal` with the lowered signature minus erased components; `main : IoToken → IoToken` → `define internal void @flow_main()`. The public wrapper: `define i32 @main() { call void @flow_main(); ret i32 0 }`. One shared `trap_bb` per function per kind, calling `flow_trap` (`noreturn`) — reached only from guards.

## 3. Loops — ADR-0016 as CFG (the canonical quartet only, L3)

Per merge (recognized exactly like the interp driver derives its plan — same attribution rules):

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
- `differential.rs`: for each of the 10 examples **and** testgen programs (default mode — traps allowed), on **raw and `rewrite()`d IR**: emit → write tempdir → `clang <prog>.ll <libflow_rt.a> -o prog` (`-O0`) → run → compare per L1 (`Done` ⇒ exit 0 + stdout byte-equal; `Trapped` ⇒ exit 101, stdout ignored). `clang` located via `which`/`CC`; absent ⇒ skip-with-reason recorded in STATUS (HANDOFF §5.5). `libflow_rt.a` built once per test-run via `cargo build -p flow-rt` (workspace target dir).
- `perf_baseline.rs` (ignored-by-default long run): sepia-shaped synthetic at N ∈ {16, 4096, 262144} (builder-generated map+fold over `[Pixel; N]`): wall-clock native (`-O0` and `-O2`) vs interp; numbers recorded in STATUS (HANDOFF §8 P5 "first perf baseline — sepia at N×N").

## 5. Module layout

```
crates/flow-rt/src/lib.rs            # §1 (new workspace member, ADR-0020)
crates/flow-backend-llvm/src/
  lib.rs        # emit, EmitError + curated pub use
  ty.rs         # Ty → LLVM type text, token-component erasure maps
  module.rs     # skeleton: externs, Str globals, main wrapper
  func.rs       # emit_fn: slots, topo walk, op table (§2)
  loops.rs      # §3 quartet CFG
crates/flow-backend-llvm/tests/
  golden_ll.rs  differential.rs  perf_baseline.rs
```

Deps: `flow-ir`; dev-deps: `flow-syntax`, `flow-lower`, `flow-interp`, `flow-rewrite` (+ its testgen via `#[path]`), `insta`, `tempfile`. No LLVM crates — text emission only (HANDOFF §5.5).

## 6. Test plan (what P5-green / M2 means)

1. flow-rt render-parity unit table (§1) — incl. `-0.0`, `NaN`, `inf`, `4080.0`, `5.375`.
2. Golden `.ll` per example + micro shapes (loop CFG, trap guards, select-Phi, token erasure visible as absent slots).
3. **Differential green on all 10 examples** (raw + rewritten) — the M2 line — plus testgen sweep (≥ 256 cases), trap cases asserting exit 101.
4. Determinism: emit twice → byte-equal.
5. `Unsupported` on the hand-built nested-loop graph (L3 pin).
6. Perf baseline recorded in STATUS (§4).

## 7. Decision ledger (BL1–BL6)

| id | decision | why |
|---|---|---|
| BL1 | Slot/alloca scheme, mem2reg left to LLVM | every op a local template; piecewise functor legible; `-O0` differential honesty |
| BL2 | `Phi` = `select`, strict | oracle evaluates both branch cones — control-flow Phi would *skip* a trapping untaken branch and break R1 parity |
| BL3 | Wrapping ints, no `nsw`; `MIN/-1` guarded to `MIN`; traps via `flow_trap`/exit 101 | ADR-0020 §3; LLVM `sdiv` UB on both hazards |
| BL4 | Token fully erased; effect order = topo order of the token chain | ir I4 linearity makes the chain total order; no runtime token value exists to carry |
| BL5 | Internal ABI: aggregates by value, `internal` linkage, `@main` wrapper | private to the translation unit; no FFI surface beyond flow-rt |
| BL6 | Nested loops `Unsupported` (with interp M1 + rewrite RW8 as one scope boundary) | one honest ceiling across the toolchain; lifted together or not at all |

## 8. Open questions (→ ADR candidates / later)

- `-O2` differential row (cheap, catches LLVM-level UB accidentally relied on) — add when differentials are green at `-O0`.
- `frem` vs Rust `%` parity for float `Mod` — pin with a differential case; if `frem` diverges, call `fmod` from flow-rt instead.
- Nested-loop emission increment (with interp + rewrite, BL6).
