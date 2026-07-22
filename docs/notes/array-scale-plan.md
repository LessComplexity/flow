# Design note: the three array scale walls — options and sequencing

Written: 2026-07-18 · post-review critique-mitigation pass (plan.md item 7).
Status: **design plan, not an ADR**. Nothing here changes semantics; every option below is a backend emission-scheme or analysis change except where explicitly marked Core+1 (those rows need their own ADR and are Sapir's call — Core scope stays frozen). Ground truth: `crates/backends/llvm/src/func.rs`, `docs/components/backend-llvm/{STATUS,DESIGN}.md`, ADR-0021, `tests/perf_baseline.rs`.

## The three walls (as recorded)

All three are recorded in backend-llvm/STATUS "What does not / known issues" and DESIGN §4; this note converts the records into a plan.

| # | Wall | Recorded evidence |
|---|------|-------------------|
| W1 | **Stack ceiling** — whole arrays live in entry-block allocas | sepia at N = 262144 holds ~9 MB of `[Pixel; N]` allocas over the 8 MB default stack; the perf test works around it with `ulimit -s hard` (`perf_baseline.rs:run_big_stack`, lines 162–168) |
| W2 | **Literal-store explosion** — an array literal emits N `Pair` stores | at N = 262144 the module is ~1M lines and clang `-O2` needs >25 min CPU (observed S13); 65536 is still minutes; perf N is capped at 4096 (`perf_baseline.rs:173–179`) |
| W3 | **Naive-copy `Update`** — every element write memcpy's the whole array | ADR-0021 §4: "naive `Update` is O(n) copy per write in every backend until the in-place headroom lands"; k updates ⇒ O(k·n) memcpy traffic |

None of these is a correctness bug. They are the recorded cost of BL1 (slot/alloca scheme, mem2reg left to LLVM) and of ADR-0021's correctness-first choice. The walls become claims-blocking only when the project wants to say something about performance beyond "native ≫ interp on the sepia shape at N ≤ 4096".

## W1 — stack ceiling

**Root cause in the emission scheme.** Every materialized (non-constant, non-erased) object gets exactly one `alloca` in the function's entry block (`func.rs:196–215`), sized by its lowered type; arrays lower as whole aggregates (`Array{T,n}` → `[n x T]`, DESIGN §2). Entry-block allocas are never reused and mem2reg does not promote large aggregates, so the frame is the **sum of all whole-array aggregates in the function**, live or not. Sepia at N = 262144: `Pixel` = 3×f32 = 12 B ⇒ 3 MB per `[Pixel; N]`; the program carries three such aggregates (`image`, `mapped`, and the `(f32, [Pixel; N])` fold pair) ⇒ the recorded ~9 MB. The 8 MB default is a property of the host, not of the program — examples and testgen arrays are tiny and never approach it; the perf shape is the only place it bites.

### Option 1a — heap-lower arrays above a byte threshold

Arrays with `n · sizeof(elem)` above a fixed threshold lower to a pointer slot; storage comes from two new flow-rt externs (`flow_alloc(size) -> ptr` / `flow_free(ptr)`, thin over Rust's allocator). `Index`/`Update`/`Map`/`Fold` GEP against the heap pointer — the op table rows are unchanged in shape. Frees are scheduled at the last-use frontier (category-ir §9.4/§10 — this would be that machinery's first real consumer). Escape: a heap array returned from an internal fn transfers the free duty to its caller (the spec §10 "Con" — escape analysis is load-bearing); in Core today this is a caller-frees rule over internal by-value calls, no user-visible change.

- *For.* Lifts the ceiling entirely (frame holds pointers, not aggregates); the threshold (a deterministic function of the `Ty` — L2-safe) keeps small arrays in allocas, so golden snapshots and mem2reg behavior for examples/testgen are mostly untouched; composes with 3a (in-place `Update` writes into the heap buffer).
- *Against.* The first dynamic-allocation path in the runtime: needs an OOM story (pin: NULL ⇒ abort, matching Rust's own allocation-failure behavior — **not** a new `flow_trap` kind; the oracle has two trap classes and L1 says classes never cross). Adds a free-scheduling pass with exactly the subtlety §10 warns about (escape at return). flow-rt's surface grows beyond print/trap for the first time.
- *Note.* LLVM-backend-specific. Verilog has no heap (arrays are BRAM; ADR-0021 §4's one-write-port RAM story is unaffected), and the CUDA backend will need its own device-allocation answer at P6 — see sequencing.

### Option 1b — promote never-written arrays to private constant globals

An array whose object has no `Update` consumer anywhere in its use set is read-only; emit it as a `private constant` global (`.rodata`) and GEP-load from it directly — no stack slot at all. Under naive-copy `Update`, even the *source* of an update chain qualifies (the memcpy reads the global; the target is the mutable slot).

- *For.* Removes the biggest frames (large literals are exactly the read-mostly ones) without touching flow-rt or introducing allocation; zero runtime cost.
- *Against.* Only helps literal-originated arrays — `map`/`fold` results still need a home, so sepia's `mapped` array stays on the stack; partial fix. Interacts with 3a (an in-place `Update` must never target a constant global — see the differential-duty section).
- This option shares most of its detection machinery with W2's option 2a (both need to recognize a literal-fed array); they should be designed together.

### Option 1c — keep the alloca scheme, make the frame a compile-time number

Every type is statically known, so the frame size is computable at emit time. Report it (a CLI diagnostic when the frame exceeds a configurable budget) and/or have the CLI run `main` on a spawned thread with an explicit stack size — the Rust-idiomatic form of what `run_big_stack` does with `ulimit`.

- *For.* Cheap; converts a silent host-limit failure (SIGSEGV) into an honest compiler diagnostic; moves the existing workaround from the test into the product.
- *Against.* Lifts nothing — the ceiling moves from "crash" to "reported"; large-N programs still don't run on an unmodified host.

## W2 — literal-store explosion

**Root cause in the emission scheme.** Core has no array-fill node: `pack_array` mints N `Pair` morphisms, and the `Pair` arm (`func.rs:301–307`) emits a whole-aggregate load of the source (`load_whole`), a GEP (`component_ptr`), and a store per element — ~3–4 `.ll` lines per element, so the module is O(N) instructions (~1M lines at N = 262144, matching the recorded observation). clang's `-O2` time is superlinear in instruction count; the interpreter walks the same N `Pair`s but pays only linear time. This is why the perf cap is 4096 and why even the `ulimit`-raised 262144 build was only ever measured once. A load-bearing IR detail: `flow_ir::Value` has scalar variants only (`ty.rs:146–154` — no array/aggregate variant), and sepia's literal reuses *one* packed struct object N times, so "the elements are constant" is a property of a Pair cone feeding the array, not of any `Constant`-kind object.

### Option 2a — emit literal data as a private global constant (backend-local fold)

The emitter recognizes an array object whose Pair cone bottoms out in constants (folding through struct/tuple packing — the sepia case needs exactly one such fold level), emits the data once as a `private constant [n x T]` global, and materializes the slot with a single `llvm.memcpy` (or zero copies where 1b applies). Two emission forms: a **typed constant** (portable, still O(N) parse text but no instruction-level optimization work — clang parses constant data far faster than it optimizes 3N instructions) or a **byte blob** (`c"…"` string constant — one line, the fastest for clang, but it hard-codes host layout/endianness, which the current emitter deliberately avoids via the gep-null sizeof idiom; would need a host-target pin).

- *For.* Emitter-local — no IR change, no ADR; directly un-caps perf N; shares detection with 1b.
- *Against.* The constant-detection is a small fold pass with a real correctness edge (a Pair cone that *looks* constant but isn't must fall back to per-element stores); the magnitude of the clang-time win is expected large but unmeasured — the >25 min datapoint is the baseline to beat, and the win must be recorded, not assumed. Byte-blob form trades the emitter's layout-agnosticism for speed.

### Option 2b — an array-fill primitive (Core+1, needs an ADR)

`fill : (T × n) → [T; n]` at IR level (or ADR-0021's noted Option B `tabulate`, the general form, which needs closures and stays parked). Fixes the problem at the source: sepia's literal is a fill of gray-200, one node instead of N.

- *For.* O(1) IR and O(1) emission for fill-shaped programs; helps every backend at once.
- *Against.* A language change (frozen Core; Sapir's call), and only fill-shaped literals benefit — distinct-element literals still need 2a. Not a substitute, a complement.

### Option 2c — accept and document the cap

STATUS already records the 4096 cap and its reason; DESIGN §4 records the dropped 262144 row. Zero work, and the honest status quo until 2a or 2b lands. Recorded for completeness — this is the default if neither option is scheduled.

## W3 — naive-copy `Update`

**Root cause in the emission scheme.** `emit_update` (`func.rs:566–587`) emits the type-directed index guard, then `llvm.memcpy` of the *entire* source array into the target's fresh slot (size via the gep-null sizeof constant expression), then one dynamic GEP + element store. "Rebind mints a fresh object, slots never alias" is the emitter invariant the memcpy pays for. Per write the traffic is O(n); the op's motivating pattern — loop-carried `mut c` with `c[t] <- v` (matmul, `differential_matmul_loop_driven_update`) — performs k updates ⇒ O(k·n) traffic for an O(k) algorithm.

### Option 3a — last-use in-place `Update` (the recorded headroom)

When the source array's last use is the `Update` itself, skip the memcpy: store the element into the source's slot and alias the target object to it. ADR-0021 §4 names this headroom; `notes/array-update-design.md` (Option A) states the performance case; the analysis substrate already exists as spec machinery — the use set is `out_edges` and last uses are the §9.4/§10 frontier.

**Why the analysis is *sound and exact* in Core — and the Futhark comparison.** The gold standard here is Futhark (Henriksen et al., PLDI 2017), which makes in-place array updates safe in a functional GPU language through **uniqueness types**: because Futhark is higher-order, whether an array is aliased at a call boundary cannot always be decided locally, so uniqueness becomes a type-system property the programmer and compiler co-maintain. Flow Core needs none of that machinery, for three structural reasons: (1) **purity + one-definition** (ir I3, ADR-0013: every object has exactly one defining morphism, all dataflow is adjacency) — the use set is literally `out_edges`, no aliasing can hide off-graph; (2) **first-order with closed bodies** — `map`/`fold` bodies capture nothing (L11xx design), so uses cannot escape through a closure; (3) **whole-program, fixed `n` in the type** — no call-boundary uncertainty, no dynamic sizing. Uniqueness in Core is therefore a *graph property decidable by a walk*, not a type-system extension, and nothing becomes user-visible. This is E3's own machinery (ERRATA E3 → ADR-0004) applied inside its proven scope — the guarantee is scoped to the first-order core, and last-use in-place stays inside that scope. If Core+1 ever adds closures or higher-order arrays (the `tabulate` direction), this conclusion must be revisited, and Futhark's type-level answer becomes the model.

The two emission subtleties, honestly stated: (i) **loops** — the loop-carried case is the one that matters and it is sound *structurally*: in the guard-first CFG (ADR-0016/DESIGN §3) the decide cone runs before the branch every iteration and the advance cone (where the `Update` lives) is unreachable on the exit iteration, so no decide-cone read can observe a post-`Update` value; in-place = element store into the merge slot, and the exit route still copies the merge slot to the exit object's slot once (one O(n) memcpy per loop, not per update). (ii) **the aliasing invariant** — today's "slots never alias" becomes a deduced, analysis-gated fact; the analysis must also respect the strict `select`-Phi (BL2: both cones always execute, so a Phi operand is always consumed) and must treat global-promoted arrays (1b/2a) as **non-unique** — an in-place store into `.rodata` is a segfault, outside trap semantics (see differential duty). Recommendation: implement the analysis in `flow-ir` next to `loop_plan` (the BL7 one-source-of-truth precedent — interp, rewrite, and every backend consume the same predicate), consumed by the LLVM emitter as an emit option.

### Option 3b — copy-on-write with reference counts

Uniform heap arrays with a refcount; `Update` mutates when rc = 1, copies otherwise.

- *Against (considered and rejected for Core).* Buys nothing over 3a inside Core — the static analysis is *complete* here, so the runtime check pays per-array headers and rc traffic to answer a question the graph already answers. It also reintroduces exactly the runtime reclamation machinery the E3 memory model exists to avoid. Becomes relevant only if arrays later escape into higher-order or dynamic contexts where static last-use is no longer decidable — record it as the Core+1 fallback, not a Core option.

### Option 3c — rewrite-level `Update` collapsing (law L-c)

`update ∘ update` at equal const in-bounds slot collapses to the outer write (ADR-0021 §3). Reduces the *number* of updates, not the per-update cost; depends on the `reoperand` channel the S13 review recorded as missing from RewritePlan. Orthogonal micro-win, not a wall-lifter.

## Recommendation and sequencing

1. **W3 first — option 3a (last-use in-place `Update`), before the P6 CUDA backend.** It is the only wall that changes the asymptotics of an already-shipped op, and its motivating pattern (matmul) is already the load-bearing differential. Doing the analysis once in `flow-ir` before P6 means the CUDA backend inherits the unique/in-place fact as an input rather than re-deriving it; retrofitting an analysis consumer into two backends costs more than designing it into the second. Emit-side it lands as a flag (`on` by default, `off` kept for A/B differential duty).
2. **W2 second — option 2a (global-constant literal fold), before or during P6.** Emitter-local, no ADR, and it un-caps perf N. Design its constant-cone detection together with 1b's read-only promotion (same pass, two sinks). Take the typed-constant form first; the byte-blob form only with a host-target pin and a measured win.
3. **W1 last — option 1a (heap lowering), designed alongside P6's memory story.** The CUDA backend needs an allocation home for arrays regardless; CPU heap lowering and device allocation are siblings under one "arrays get an address-space-aware home" design, and the P6 design session is the right place to fix the flow-rt alloc/free surface so both backends share it. Implementation can wait until large-N CPU numbers become a claim the project wants to make — W2 must land first anyway (there is no point heap-allocating a frame whose program clang cannot compile). **1c is unsequenced** — a cheap honesty improvement (compile-time frame-size report) that can land in any session.

**What blocks any performance claim.** Today the only recorded numbers are sepia at N ∈ {16, 4096}, native time flat ~4.8 ms and process-spawn-dominated, interp ~80× slower at 4096 and diverging with N. Any claim beyond that requires, in order: W2 fixed (large N must compile at all), 3a (Update-bearing programs must not be quadratic), and the `-O2` differential row (recorded open in STATUS/DESIGN §8; being added in this same pass) — perf numbers are measured on `-O2`, so `-O2` must be inside the verified envelope. A harness that isolates compute from process spawn is also owed before quoting absolute native times.

## Differential-test duty (per option)

L1 is the law for every option: `Done` ⟺ exit 0 + stdout byte-equal to the oracle; `Trapped` ⟺ exit 101; classes never cross. L2 likewise: every analysis/threshold decision must be a pure function of the sealed IR (emit-twice stays byte-equal). Per option:

- **1a (heap lowering).** Oracle parity is on *values*, so expectations are unchanged — but coverage *improves*: a big-frame program that today can only run under `ulimit` becomes runnable in the plain harness (the sepia-at-262144 shape can join the differential instead of the perf-only file). OOM is pinned to abort, never `flow_trap` — no third trap class. flow-rt gains alloc/free unit tests; the render-parity table is untouched. Threshold decisions deterministic per L2.
- **1b/2a (constant globals).** Read-only data; index/update guards are runtime and unchanged. Golden `.ll` snapshots regenerate (DESIGN §4: snapshots are read before accepting). Byte-blob form carries a portability pin (host layout only) that the host-only differential cannot catch — that limitation is recorded, not papered over.
- **1b/2a × 3a interaction (the sharp edge).** The last-use analysis must treat global-promoted arrays as non-unique. Failure mode if it doesn't: in-place store into `.rodata` ⇒ SIGSEGV — a class-cross the differential *will* catch (expected `Done`/101, observed signal), but only if the pool contains `Update`-of-literal programs. testgen already generates `Update` (ADR-0021 §5); pin a case where the updated array is a large literal.
- **3a (last-use in-place).** A/B duty: the entire differential suite runs with the emit flag on **and** off — 10 examples raw+rewritten, ≥256 closed testgen cases × 2 modes, the trap cases, and `differential_matmul_loop_driven_update` as the load-bearing loop-carried case. Trap behavior is parity-safe by construction (the guard precedes both the memcpy and the in-place store — `func.rs:574`; on a trap the process exits 101 before any write lands, so naive and in-place are indistinguishable to the oracle) — `differential_traps_exit_101` runs under both modes to pin that claim. Honest cost: the sweep roughly doubles.
- **3c (rewrite L-c).** Inherits the existing rewritten-IR differential rows; no new duty beyond the missing-`reoperand` channel's own tests when that lands.

## What this does NOT commit to

- **No language or semantics change.** No `fill`/`tabulate` primitive (2b is marked Core+1 and needs its own ADR), no dynamic arrays `[T]`, no `Vec` (ADR-0021's non-goals stand), no raw mutation and no heap quartet (ADR-0013's exclusion stands). In-place `Update` is a backend deduction under value semantics, not a semantics change — the naive copy remains the reference semantics.
- **No GC, no refcounting in Core.** 3b is recorded as rejected-for-Core; the E3 memory model (scoped by ADR-0004 to the first-order core) is not reopened — heap lowering under 1a must stay inside that scope (fixed `n`, escape only to a known caller).
- **No milestone promise.** The sequencing above is a recommendation, not a schedule; P6's scope is unchanged by this note, and no option is committed to a session.
- **No new performance claim.** The perf cap stays at N = 4096 until W2 lands, the `ulimit` workaround stays until W1 lands, and the sepia numbers in STATUS remain the only recorded baseline.
- **No claim that uniqueness *types* are needed.** Futhark's type-level machinery is cited as the gold standard for the higher-order case; this note's claim is narrower — Core's first-order, closed-body, whole-program structure makes last-use a graph analysis. If closures or higher-order arrays ever land, this section is void and the question reopens.
- **No Verilog commitment.** W1/W2 are LLVM-emission concerns; the Verilog array story (BRAM, one write port, ADR-0021 §4) is designed at P7 and untouched here.
