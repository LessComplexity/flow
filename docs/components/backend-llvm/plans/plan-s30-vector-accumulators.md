# Plan — S30: tile accumulators as vector SSA values (kill the alloca)

**Status:** written pre-build, S30. Origin: the S29 KC diagnosis
(`docs/performance/matmul/s29.md` §1) — the KC nest is 3× slower not because of its
traversal (~3% of the gap) but because `clang` stops register-promoting the accumulator
alloca once other loops touch it: **92 `str q…,[sp]` in the hot task vs 0 in the
baseline**. Sapir: "do the phi rewrite for the tile accumulators."

## Why (one paragraph)

The emitter gives every object a stack slot and relies on LLVM's `mem2reg`/LICM to put
the hot ones back in registers. For the tile accumulator that promotion is the entire
performance of the kernel — and it is a *heuristic*, not a contract: it held for the
jt-outer nest and withdrew the moment the KC nest added park/reload loops over the same
slot. The fix is to stop asking. A value with no address has no aliasing question and
nothing to promote: emit the accumulator tile as SSA values carried by phi nodes, and
the register form becomes what we emit rather than what we hope for. This also drops our
dependence on the loop vectorizer, which is what currently turns the scalar lane loop
into `fmla.4s`.

## Categorical model

Nothing changes in `Dat`/`Trn`: the per-cell morphism chain is identical, k-ascending,
same operands, same rounding. This is purely a `DataLoc` change — the same accumulator
datum placed in the register file instead of the stack frame — and therefore R1-neutral
by construction, which the differential suite enforces.

| Item | Kind | Model |
| --- | --- | --- |
| `acc_r : <TJ x elem>` (one per subrow, `r < TI`) | `DataLoc` (register) | the live j-tile's accumulators as SSA values. `<TJ x elem>` is deliberately the FULL tile width, not the target's vector width: LLVM legalizes it to whatever the machine has (4× `q` on NEON, 2× `zmm`-class elsewhere), so the emitter stays target-independent — no `TILE_J`-vs-hardware-width coupling |
| the k loop's `phi` | `Trn` (the loop-carried edge, made explicit) | `%acc_r = phi [ %seed_r, %preheader ], [ %next_r, %latch ]` — the accumulation recurrence written in the value graph instead of in memory |
| lane unrolling | placement precondition | a phi needs one SSA value per accumulator, so the lane axis must be compile-time constant. True exactly on the constant-TJ **main** path; remainder/boundary tiles keep today's memory form |

**Composition rules.**
1. Per-cell arithmetic is unchanged: `fmul`/`fadd` (or their `contract` twins) over
   `<TJ x elem>` compute the same per-lane values in the same k order as the scalar
   chain — SIMD lanes are independent, so this is bit-exact, not merely close.
2. The new path applies **only** where the lane count is the compile-time constant
   `TILE_J`. Every remainder tile, boundary row, and runtime-`tj` tile keeps the existing
   emission byte-for-byte (the negative control).
3. Vector loads/stores carry an **explicit element alignment** (`align 4` / `align 8`),
   never the vector type's ABI alignment — `j0` offsets are arbitrary and the ABI
   alignment of `<16 x float>` is 64.
4. The accumulator never gets an address on this path: no `alloca`, no `getelementptr`,
   no `load`/`store` of accumulator state inside the k loop. Park/reload and the final
   store-out still touch `out` — those are real memory, once per panel, outside the k loop.
5. Both nests take it (jt-outer and KC), so the comparison in s29.md §1 is re-run on
   equal footing.

## Emission work items (backend-llvm only)

1. `emit_tile_trio_vec` beside `emit_tile_trio`: seed splat → k loop with `TI` vector
   phis → store-out. Gate: `main && bound == TILE_J`.
2. Vector operand helpers: `emit_tile_b_vector` (contiguous `<TJ x elem>` load from the
   packed panel or from `b` when `clane == 1`; splat when `clane == 0`) and an a-value
   splat (`insertelement` + `shufflevector` zeroinitializer).
3. The same for `emit_tile_kc_trio` — its reload/park stay scalar loops over `out`,
   but the k loop between them becomes phi-carried.
4. Keep the ×2 k-unroll: the body threads each accumulator through both steps.

## Tests

- differential: unchanged suite must stay green — byte-equal vs untiled and vs the interp
  oracle at -O0/-O2, both nests, f32 and f64, `MAPAL_PAR` splits. This is the R1 gate and
  it is the whole safety argument for rule 1.
- golden: the tiled goldens re-pin DELIBERATELY (the emission changes shape); assert
  structurally that the main path contains **no** accumulator `alloca`/`load`/`store` and
  does contain `phi <TJ x elem>`. The remainder/boundary goldens must NOT move.
- disasm gate (new, cheap): `str q…,[sp]` count in the tiled task is **0** for both nests.
  That is the actual thing this plan buys, and it is the assertion that would have caught
  the S29 regression at build time rather than at benchmark time.

## Measurement (done-when)

- `tile_ab.sh matmul1024_cap_f32`, MAPAL_PAR=1, min-of-3: the KC-off leg must not regress
  from 19.8 ms; the KC-on leg must close most of the 56.3 → 19.8 gap. If KC-on lands near
  KC-off, the S29 verdict flips from "the order is bad" to "the order was never measured"
  and the box leg becomes worth running.
- fir and conv2d shapes re-measured — they share the trio, so they inherit whatever this
  does.

## Ceilings (recorded, not built)

- Remainder/boundary tiles keep the memory form. They are a small share of the work at
  benchmark sizes and their lane count is genuinely runtime; masked vector ops would be
  the upgrade.
- The `<TJ x elem>` width is the tile width, not a tuned machine width. If a target
  legalizes it badly, the fix is a per-target `TILE_J`, which is already an emitter
  constant.
- Other objects keep the alloca discipline. This carve-out is justified by the tile
  kernel being the one place where the emitter both controls every access and knows the
  size at compile time.
