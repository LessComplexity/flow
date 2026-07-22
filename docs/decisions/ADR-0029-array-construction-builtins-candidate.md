# ADR-0029: Array-construction builtins — `iota`, `fill`, numeric widening (candidate, accepted-for-implementation)

Date: 2026-07-22 · Status: **accepted-for-implementation — STAGE 1 SHIPPED · STAGE 2 SHIPPED S21 (2026-07-22: CUDA iota/fill kernel realization — the 5th `Unsupported` cell discharged; `widen` per the Amendment below — `Operation::Widen` + the `widen_i64`/`widen_f32`/`widen_f64` builtin family, whole pipeline incl. testgen; procedural-v2 bench generators — sources 3.8 MB → 72 KB, oracle pins −275/3748 + 1815/6944 reproduced). Residual: the llvm N≥256 clang -O2 gate exposed a distinct emitter defect (first-class aggregate staging copies — S21 WP3b, in progress), NOT a source-size issue — the module is 21 KB.** (S20 optimization-marathon mandate: "for the adr suggestions no need for my approval, let the best option take the case for now"): flow-ir `Operation::Iota`/`Fill` + validation (`NonStaticCount`/`IotaCountMismatch`), the interp oracle arms, and the llvm emission (loop-skeleton store loops, compile-and-run parity `differential_iota_fill`) all landed with tests green; the CUDA realization is `EmitError::Unsupported`-stubbed pending stage 2 (kernel shapes + arena membership); flow-lower/syntax surface and `widen` are the next stages. Deviations from D1/D2 as written: (i) `iota`'s count rides as a `Constant` source object (graph-property static-n, replay-uniform); (ii) `fill`'s count is the internal 2-tuple's slot-1 `Constant` (the `zip`/`update` pattern — `n` also derivable at replay); (iii) `widen` deferred to the lower stage (it is a typing/emit concern, no dedicated IR op planned — to be decided there and recorded). Implementation wave queued behind the current benchmark sweep + W3 (last-use) — this is a cross-pipeline language change (syntax → lower → interp → both backends), executed model-first like ADR-0018.
Motivation: measured on the S20 marathon sweep (RTX 4090 box, clang 18): the matmul generators embed a/b as N² literals → `matmul256_cap.ll` is **23 MB** of literal stores; a single `clang -O2` build needs **27 GB RSS and ~1 h**; two parallel builds were **OOM-killed**; sources are unreviewable (1.5 MB); N≥512 llvm legs are infeasible. backend-llvm BL1 (alloca soup / literal-store modules) + suggestion #3 (array-fill primitive) name the same wall.

## Context

Flow builds arrays today in exactly two ways: **literals** (`[1.0, 2.0, …]` — materialized as host static data + one upload, BC11) and **bulk ops** (`map`/`zip`/`enumerate`/… over an existing array). There is no way to build a big array *from nothing*: no fill/repeat, no index sequence, no numeric conversion. Consequences, all measured this session:

- Benchmarks must embed data as N² literals ⟹ module size, compile time, and review-ability all blow up ~quadratically.
- The natural procedural shape — `iota(nn) -> map { t -> ((t*7+s) % 101) - 50 }` — is unwritable: `iota` doesn't exist, and the integer formula can't become a `f64`/`f32` element (no conversion).
- The baselines (naive-cuda, rust_naive, cpp_naive, chapel) all build their matrices procedurally from the index formula — the *like-for-like* Flow program is the procedural one.

ADR-0018 (`zip`/`enumerate` as pure collection builtins) is the precedent for adding exactly this class of op; ADR-0023 (dynamic sizes) is orthogonal — every size here stays static.

## Decision (recommended option; questions resolved with defaults)

**D1 — `iota(n)` builtin (pure collection op).** Produces `[T; n]` of consecutive integers `0..n-1` (element type defaults `i32`; `iota(n): [i64; n]`-style annotation where needed). Realization: one new `Operation::Iota` (sibling of `Enumerate`) — interp computes it directly; CUDA emits a trivial `out[i] = i` kernel (arena-member buffer); llvm a store loop that clang vectorizes. Trap-free by construction (like `Zip`/`Enumerate` — no trap param).

**D2 — `fill(x, n)` builtin.** Produces `[T; n]` with every element `x`. Same realization shape (`out[i] = x`). Together D1+D2 cover constant arrays (`fill`), index sequences (`iota`), and — composed with the existing `map` — every procedural construction the benchmarks need.

**D3 — Numeric widening conversion.** Explicit postfix `widen` on numeric expressions: `expr -> widen` lifts `i32→i64→f64` and `i32→f32`, `f32→f64` along the existing safe-widening lattice (wrapping/narrowing conversions stay rejected — the L-code matrix's literal-width discipline extended, not altered). No implicit conversion anywhere (the literal-width unification rules are untouched).

**D4 — Syntax: statement builtins, not operators.** `iota(65536) -> trange;` / `fill(0.0, 65536) -> seed;` / `((t * 7 + 13) % 101) - 50 -> widen -> a_elem;` — one lowering path each, all pure (E2-legal inside fanout bodies: no token, no captures beyond reads).

## Semantics notes

- **Oracle first:** interp gets the ops in the same change; the differential matrix (raw+rewritten, -O0/-O2, 640 cuda runs) covers them from day one. Determinism (L2): output text is a pure function of the graph.
- **Arenas:** `Iota`/`Fill` outputs are ordinary arena-zone members (static sizes).
- **Trap law:** both ops are total — they cannot trap, so they carry no trap param (the S20 `TrapCaps` analysis classifies them like `Zip`/`Enumerate`).
- **Not** dynamic arrays (ADR-0023): `n` is a static literal at lowering; the type stays `[T; n]`.

## Alternatives weighed

| Option | Verdict | Why |
| --- | --- | --- |
| Keep literals; optimize their emission (packed static data, faster clang path) | rejected | treats the compile-time symptom; the *source* stays quadratic and unreviewable; N≥512 still gated by module size |
| Generators emit per-element `Update` chains | rejected | Θ(n) ops in the graph — worse than literals everywhere |
| `repeat`/`range` as *syntax sugar* lowered to literals | rejected | identical module blowup one stage later |
| Implicit int→float coercion | rejected | Flow's width discipline is explicit-by-design (the L-code matrix); a single explicit `widen` keeps it |
| Wait for ADR-0023 dynamic arrays | rejected | orthogonal; sizes here are static — the gap exists *today* at every static size |

## Amendment — `widen` realization decided (S21, 2026-07-22)

Stage-1 deviation (iii) deferred the `widen` decision. Resolved under the S20/S21
delegated mandate:

- **IR: new `Operation::Widen` (33rd Core variant).** A representation-changing
  conversion cannot be a typing-only concern — every backend must emit a concrete
  cvt (`sext`/`sitofp`/`fpext` in llvm; C casts in cuda), so the op must exist in
  the graph. Unary, scalar, total, trap-free, pure (no token, fanout-legal;
  TrapCaps/FnAttrs class of `Zip`).
- **Lattice (validate-enforced):** `(i32→i64) · (i32→f64) · (i32→f32) · (f32→f64)`.
  Deviations from D3 as written: **`i64→f64` excluded** — not value-exact above
  2^53, and D3's "safe-widening" premise breaks; add by amendment if a use
  appears. `i32→f32` stays (D3 explicit; exact only within ±2^24 — the user opted
  into that by writing the explicit builtin; note recorded here, not a trap).
- **Surface: builtin family `widen_i64` / `widen_f32` / `widen_f64`** as bare
  pipeline stages riding the ADR-0018 name-resolution path (like `zip`/
  `enumerate`; collision → L1009). Deviation from D4's single postfix `widen`:
  lower's typing is forward-synthesis — a bare `widen` cannot infer its target,
  and `i32` has three lattice successors, so the single-name form is ambiguous
  by construction. Explicit names also match the width-explicit L-code
  discipline. Zero parser change (P0108 untouched — these are names, not call
  expressions). Illegal source/target pair → new L-code (L1614 class).
- **Rewrite:** replay arm mandatory (exhaustive match). Const-fold
  `Widen(Constant c) → Constant(widened c)` is oracle-exact and cheap — include;
  anything further is recorded headroom.
- **Testgen:** add Widen draws over the lattice so both differentials cover it.

## Consequences

- New wave: syntax (2 builtin forms + `widen`), lower, interp, flow-ir (`Operation::Iota`/`Fill` + validate + mermaid), both backends, testgen coverage, golden + differential rows, spec CHANGES/ERRATA application (the ADR-0018 process).
- The matmul generators then emit ~40-line sources for every N (incl. N=512/1024); the 23 MB module and the OOM-kill class disappear; the bench matrix gains the N≥512 legs honestly.
- bench harness: after the wave lands, regenerate artifacts (procedural v2) and re-sweep; the S20 v1 (literal) numbers stay recorded with their provenance.
