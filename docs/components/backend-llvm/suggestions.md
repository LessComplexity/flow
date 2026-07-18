# backend-llvm — suggestions (category-theory derived)

> Improvements deduced by FRAMEWORK rules. Each cites its rule and names the concrete
> change. Not applied — a backlog for future work.

| # | Rule (§) | Smell found | Proposed change | Payoff |
| --- | --- | --- | --- | --- |
| 1 | §4.4 / §7.4 strategy 2-category | Three backend crates will realise one contract `CategoryIr → TargetText`; no shared contract is declared yet | Fix a shared `Backend` trait + `TargetText` type by ADR **before** the first backend is written (the firm candidate in [categorical-model.md §7.5](../../architecture/categorical-model.md)) | Adding a target = adjoining an object; never edits the core |
| 2 | DESIGN §2 `Update` row + BL1 | `Update` always `llvm.memcpy`s the whole source array into a fresh alloca before the element store — the naive-copy semantics ADR-0021 records as replaceable | In-place lowering via a last-use analysis: when the source array is dead after the `Update` (its only consumer is the rebind), skip the memcpy and store into the source slot | Lifts the O(n)-per-update copy **and** the alloca-slot stack ceiling (BL1) that caps the perf shape |
| 3 | DESIGN §4 perf + BL1 | Array literals are N `Pair` stores, so the perf module is ~1M lines at N = 262144 and clang `-O2` blows past 25 min CPU — top N capped at 65536 (as-built S13) | An array-fill / splat primitive in Core (or heap lowering of large aggregates) so a uniform image is O(1) IR, not O(N) | Restores the DESIGN N = 262144 perf point; shrinks emitted `.ll` for any large-array program |
| 4 | DESIGN §8 open question | Differentials run only at `-O0`; LLVM-level UB the emitter accidentally relies on would not surface | Add a `-O2` differential row over the same example + testgen pool (the harness already compiles at both opt levels for perf) | Catches miscompiles that only appear under optimization; cheap once `-O0` is green |
| 5 | DESIGN §2 Index (as-built S13) | `guard_index` emits a two-sided signed compare for every index type; the `slt 0` half is provably dead for a zero-extended u8 index | Drop the lower-bound compare on the u8 path (emit `uge` only, matching §2's original spec) once a golden pins the u8 guard shape | Two fewer instructions per u8 `Index`/`Update`; the `.ll` matches the spec text exactly |

## Detail
### 1. Backend strategy 2-category
Already derived and adversarially verified in the Session-06 audit — see
[categorical-model.md §7.5](../../architecture/categorical-model.md) item 1. Owned by a
future backend ADR; recorded here so the component increment starts from it.

### 2. In-place Update via last-use
ADR-0021 makes naive copy the semantics everywhere and names in-place elision via
last-use as recorded headroom; DESIGN §2's `Update` row repeats it. The copy is the
only reason `Update` needs a distinct target alloca, so eliding it on a dead source
also collapses the two allocas into one — the direct lever on the BL1 stack ceiling
that `perf_baseline.rs:run_big_stack` currently works around with `ulimit`.

### 3. Array-fill primitive / heap lowering
The perf baseline's own escape hatch (as-built S13). Both suggestion 2 and this one
attack BL1 from opposite ends — 2 shrinks per-op frame growth, 3 shrinks the initial
literal — and either alone restores the DESIGN N = 262144 point.

### 4. `-O2` differential row
DESIGN §8's first open question; the perf harness already produces `-O2` binaries, so
this is a harness wiring change, not new emission code.

### 5. Dead u8 lower-bound compare
Cosmetic/spec-fidelity: the two-sided guard is correct but `guard_index` (as-built S13)
diverges from §2's uge-only u8 text. A one-line branch on the index signedness restores
the spec shape; gate it behind a golden so the u8 guard `.ll` is pinned first.
