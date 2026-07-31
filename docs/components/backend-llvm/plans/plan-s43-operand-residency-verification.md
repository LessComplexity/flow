# plan-s43 — verify operand residency before building the cascade

Status: **DRAFT — awaiting Sapir's go**
Written: 2026-07-31 · S43 open · designed by Fable, to be built by Opus
Governs: `docs/performance/s42-sme-roofline.md` §5e · `docs/next-session.md` §1
Component: `backend-llvm` (SME rung). **No `mapal-ir` change. No `Dat`/`Trn` change.**

## 0. What this plan is, and what it deliberately is not

`next-session.md` §1 names the S43 P0 as the **hierarchical tile→cache mapping** (registers → L1
→ L2 → L3, one tile per level) and then says, in its own step 1, **do not write any of it until the
diagnosis is verified in the emitted kernel.** This plan is that step and only that step.

It builds **an instrument, not a feature.** Nothing here ships. The deliverable is a verdict on a
falsifiable statement (§5), plus — whichever way it falls — the per-stream, per-level attribution
that tells the cascade which rung to build first.

## 1. The categorical model — why this is a Coherence Law 1 measurement

FRAMEWORK §7.6: at the systems level, **Coherence Law 1 (placement honesty) becomes a literal
hardware detector** — "every transformation's inputs are materialised at, or delivered to, its
location." The SME rung today asserts a placement it has never checked.

| Atom | Today's model | What the measurement asks |
| --- | --- | --- |
| `Trn` | `sme_panel` — the `ti·tj` outer-product accumulation over k | unchanged; held **byte-constant** across every arm |
| `Loc` | *one* implicit operand location, sized by `Sme::panel_l1d_ratio` | the real chain: **ZA regfile → L1D (128 KB) → L2 slice (~3.2 MB) → DRAM** |
| `DataLoc` | one `DataLoc` per operand array | **which `Loc` each operand stream's `DataLoc` actually sits at** |
| `Trm` | not modelled | the tile swaps between levels — what the cascade would build |

S42 §6 already recorded the model defect: *"that is not one `DataLoc` but a chain of `DataLoc`s over
one `Dat`, each with its own extent, and the `Trm`s between them are the tile swaps. Today's single
`kc` collapses that chain to one link."* The instrument in §3 **forces** a chosen `DataLoc` onto a
chosen `Loc` while holding the `Trn` fixed — which is the only way to price a placement without
also changing the algorithm.

**This is why the `kc` sweep cannot answer the question.** `kc` is a confounded instrument: it buys
residency *and* pays `k/kc` extra output sweeps, because the kernel `Store`s its ZA tiles rather
than accumulating. The window mask buys residency and pays nothing.

## 2. The residency audit — derived, and therefore exactly what needs measuring

From the emitted nest (`func/sme.rs::emit_tiled_map_sme`) and kernel body (`module.rs::sme_panel`),
at N=4096, packed, `kc` off, ti=tj=2, t=16:

| stream | footprint | reuse distance | ⇒ must reside in |
| --- | ---: | ---: | --- |
| `ap` (A panel) | 512 KB | 1 MB (one panel call) | L2 slice — **never L1** |
| `b`-panel, 2 streams | 512 KB | 64 MB (all 128 panels, one full i-step) | **DRAM** — misses even shared L2 |
| `out` block | 4 KB/call | — | L1 |

Per k iteration the kernel touches **256 B of fresh operand lines** — 2 A loads at `%apk` and
`%apk+64`, one fresh 128 B line as `%aoff` advances 32 floats; 2 B loads on streams `%bjo1` apart,
one fresh line combined. That is *byte-identical to `loadcost.c`'s 4-load pattern*, which is what
makes the probe's rows comparable to the real loop at all.

**This table is counting, not measuring — and S42's rule 17 exists because counting was wrong once
already.** §3 measures it.

### Two corrections this audit forces, recorded before any new number

1. **"1043 GF/s" is not the N=4096 cell.** Unblocked N=4096 is **171.179 ms = 803 GF/s**
   (2·4096³ / 0.171179 s). The 1043 figure quoted in `s42-sme-roofline.md` §5e's prize table and
   carried into `next-session.md` §1 belongs to a smaller size. The gap therefore **scales with N**,
   which is itself evidence that the binding stream is the one whose footprint scales with N (`b`),
   not the one that does not (`ap`). To reconcile at §7 after the run.
2. **`loadcost.c` has no L2 row.** Its two points are 32 KB and 64 MB, so it proves
   *L1-vs-DRAM ≈ 2.4×*, **not** *L1-vs-L2*. Every claim of the form "our operands miss L1, and that
   is worth 1.79×" is currently resting on a table that never measured the level in between.

## 3. The instrument — an operand window, patched into the emitted `.ll`

Mask the two k-derived operand offsets so the four loads wrap inside a window of chosen size. Same
instruction sequence, same `fmopa` count, same pack, same ZA read-out, same output stores; **only
the addresses' upper bits change.**

`module.rs::sme_panel` emits exactly:

```llvm
  %aoff = mul nuw nsw i64 %k, 32
  %apk  = getelementptr inbounds float, ptr %ap, i64 %aoff
  %boff = mul nuw nsw i64 %k, %bn
  %bk   = getelementptr inbounds float, ptr %b, i64 %boff
```

The patch inserts one `and` per stream and redirects the GEP:

```llvm
  %aoff  = mul nuw nsw i64 %k, 32
  %aoffm = and i64 %aoff, <A_MASK>
  %apk   = getelementptr inbounds float, ptr %ap, i64 %aoffm
```

**It is a text patch on the emitted `.ll`, applied before `clang` runs — NOT an emitter flag.**
Rationale, and it is the ponytail call: an `EmitOpts` knob would add permanent shipped surface for a
throwaway instrument, and the repo is *already carrying a P1 to delete `kc_nest` for exactly that
reason.* A patch script is equally verifiable (§4 checks the assembly either way) and owes no
deletion. Both lines are unique inside `@mapal_sme_panel`, so the substitution is unambiguous.

Legality: masking only ever *shrinks* the offset, so `inbounds` is preserved and there is no UB.
Masks are `2^n − 1` with `n ≥ 5`, so the 32-float and `%bn`-float k-strides keep their alignment.

**The control is not "no mask".** It is `A_MASK = B_MASK = 2^44 − 1`, larger than any real offset:
the `and` is present and dead-in-effect, values stay bit-identical, and control and treatment differ
in **one immediate**. That satisfies rule 15 by construction rather than by inspection.

## 4. Arms, protocol, and the gates that run before any number is read

N=4096 (`benches/matmul/matmul4096_cap_f32.mapal`), `--rewrite --contract
--target=apple-m4-sme`, packed, `kc` off, 1 thread. Harness cloned from `benches/sme/sme_ab.sh`.

| arm | W_a / W_b (elements) | operand footprint | isolates |
| --- | --- | --- | --- |
| 0 | shipped, unpatched | A 512 KB, B 2×256 KB | prices the `and` against arm 1 |
| 1 | **control**, 2⁴⁴−1 both | real | baseline; values **bit-identical to arm 0** |
| 2 | 4096 / real | A→16 KB, B real | A's L1-vs-L2 term alone |
| 3 | real / 4096 | A real, B→2×16 KB | B's DRAM term alone |
| 4 | 4096 / 4096 | 48 KB, all-L1 | the full residency bound, in situ |
| 5 | 32768 / 32768 | 384 KB, L2-slice | separates "L1 specifically" from "merely not DRAM" |

**Held constant:** `-O2 -march=armv8-a+sme2` (never `armv9-a` — it implies `+sve`, which this part
lacks, and SIGILLs), packing on, `kc` off, same machine. **21 round-robin cycles**, process order
rotated per cycle so the cold-clock ramp is paid symmetrically (rule 14). Report **absolute
milliseconds** — min/median/max per arm — with explicit overlap statements against arm 1.

**Replications:** N=1024 (B fits shared L2 there, so arm 3's win *must* shrink — a built-in
consistency check on the whole design), and N=4096 at full thread width (rule 16's lesson: the `kc`
result *inverted* between 1 thread and threaded, and threaded is what ships).

### Gates — all three run before a single timing is read

1. **Assembly (rules 15 + 18).** `clang -S` on the arm-1 and arm-4 modules; inside
   `mapal_sme_panel` confirm per k iteration exactly **4 `ld1w`, 4 `fmopa`, 2 `and`**, and confirm
   the full 16-row × 4-tile read-out is **intact and identical across arms**. This is precisely the
   trap the retracted `K=1` probe fell into.
2. **Control fidelity.** Arm 1 output bit-identical to arm 0, and arm-1 median within ~2% of arm 0
   — otherwise the `and` itself costs, and arm 1 is the baseline regardless.
3. **Wrong-values tripwire, inverted.** Arms 2–5 **must** print values *differing* from arm 1; they
   are wrong by construction. **A windowed arm that prints the correct answer is void — its mask did
   not survive.** The harness must not route windowed arms through `sme_ab.sh`'s value-identity gate
   as pass/fail; that gate is inverted here, and saying so is the point.

### Predicted numbers

Control ≈ 171 ms ≈ 803 GF/s; k-loop share ≈ 160 ms after ~8 ms pack (`packcost.c`) and read-out.

| arm | binding level = DRAM (the design's bet) | binding level = L1 literally | hypothesis false |
| --- | --- | --- | --- |
| 2 (A→L1) | 165–172 ms (≤4%) | ~145–155 ms | ~171 ms |
| 3 (B→L1) | **95–115 ms** | ~145–155 ms | ~171 ms |
| 4 (all L1) | **80–95 ms** (floor ≈ 84 ms) | 80–95 ms | ≥155 ms |
| 5 (all L2) | ≈ arm 4 within ~10% | ≈ control | ~171 ms |

Cross-check `gain(2) + gain(3) ≈ gain(4)`. **Large non-additivity is itself a finding** (inter-stream
bandwidth contention) — report it, do not smooth it.

## 5. The falsifiable hypothesis

> **H** — In the emitted N=4096 f32 kernel (packed, `kc` off, 1 thread, median of 21 interleaved
> runs, control = the huge-mask arm at ≈171 ms), forcing all four operand streams into L1-resident
> windows — with the `and` + 4-load + 4-`fmopa` + full-read-out sequence verified in the assembly
> and the windowed outputs verified *different* from control — yields a median **≤ 128 ms with
> distributions disjoint from control.**

**≤128 ms and disjoint ⇒ CONFIRMED**: operand residency is the gap, and arms 2/3/5 name the stream
and the level. **>128 ms, or overlapping ⇒ REFUTED**: operand residency is not the primary gap and
the search moves off blocking entirely.

128 ms is ≥1.34×, i.e. at least half the residency-bound gap (171 → ~84 ms). **The threshold is
declared here, before the first timed run.** No third outcome is admitted.

## 6. What each verdict makes the next move

| result | what it means | the cascade rung it selects |
| --- | --- | --- |
| arm 3 dominant | B's reuse distance spans all of B | build the **outer B level** first — `nc`, packed B panel resident in shared L2 (GotoBLAS's L3 rung) |
| arm 5 ≈ control, arm 4 wins | L1 specifically is the binding level | build a genuine **L1 A-micro-panel** rung |
| arm 4 ≈ control | residency is not the gap | **stop blocking-directed work**; the P0's justification needs re-grounding, and that is the finding |

## 7. Honest statement of the tension this plan does not paper over

**The depth sweep already voted against the literal hypothesis once.** `kc = 512` *is* the
"operands L1-resident" configuration (256·kc bytes = 128 KB = L1D exactly) and it measured
**0.785×**; `kc = 256` measured 0.501×. The swept optimum sits at **2× L1D**.

That does not refute operand residency — it refutes the naive fix, because `kc` pays `k/kc` output
read-modify-write sweeps for the residency it buys (~7 extra passes over a 64 MB output at
kc=512, N=4096: order 900 MB of added DRAM traffic against an operand saving of at most a few
hundred MB). The optimum at 2× L1D is where those two marginal costs cross. It says nothing about
what residency is worth *unpriced* — which is exactly what §3's window measures.

**But it does damage the claim's level, and the honest version is this:** *"do the operands miss
L1"* is close to the wrong question. A streamed operand with no intra-call reuse always "misses" on
first touch; L1 residency can only ever come from **cross-call reuse**. The well-posed question,
and the one §4 answers per-stream and per-level, is *which level transition in the operand feed
accounts for how much of the 803 → 1864 GF/s gap.*

The design's stated bet, on the evidence in §2: **the dominant term is `b`'s reuse distance spanning
the whole of B — a DRAM/shared-L2 problem, fixed by the missing `nc`/`mc` levels — and the A
L1-vs-L2 term is worth ≤5%** (`loadcost.c`'s own L1 rows put 4 loads at 1864 against 0 loads at
1957, so *all* L1 load cost is ~5%). Sapir's hierarchical direction is right. Its stated
justification — "L1 residency" — presupposes the answer to the question this experiment exists to
ask.

## 8. Cost, and what is explicitly out of scope

~1 emitter-adjacent script + 1 harness + 6 arms × 21 cycles × 3 configurations. No `mapal-ir`
change, no `EmitOpts` change, no shipped byte moved — **arm 0 is the shipping binary and it is in
the run precisely so that claim is measured rather than asserted.**

Out of scope until the verdict lands: `mc`/`nc` derivations, the cascade nest, any change to
`Sme::panel_l1d_ratio`, the f16/bf16 rung, and deleting `kc_nest`.

### Deferred, with reason

- **`loadcost.c` buffer-size sweep (E-B)** — the missing L2 rows (32 K, 128 K, 192 K, 256 K, 512 K,
  1 M, 2 M, 3 M, 4 M, 8 M, 16 M, 64 M) that would calibrate the per-level ceilings and retire the
  "no L2 row" defect in §2. Cheap and standalone. **Worth running alongside** — it prices the arms
  but cannot settle them (rule 16), so it is support, never the verdict.
- **PMU counters via `xctrace` CPU Counters (E-C)** — would read the miss rate directly.
  `xctrace` and the CPU Counters template are confirmed present on this machine. Gated on a
  mandatory validity step: calibrate against `loadcost.c`'s arms, where line traffic is known
  exactly, **before** pointing it at the kernel. Unresolved risk: whether streaming-mode SME loads
  are attributed to core L1D counters at all — the SME unit is shared and sits outside the core.
  Abandon on a failed calibration rather than spend a day.
