# Plan — S31: `TargetProfile` — the emitter's machine facts become data

**Status: SHIPPED S31** (see "As built" at the end for the four deviations).
Written pre-build, S31. Sapir: *"the backend needs to employ a strategy pattern
for architecture selection to choose the correct parameters, it shouldn't be off with
hardcoded numbers, this is a wrong way to do this."* Implements the unbuilt half of
**ADR-0032 D4** (backend config = performance tailors, machine tailoring); the
deterministic prerequisite for **ADR-0034** (candidate — placement constants are searched,
not set), which can later fill this table by measurement instead of by hand.

## Why (the evidence, not the principle)

Six machine facts are literals in the backend today:

| constant | value | what it assumes |
| --- | --- | --- |
| `tile_j_for` | 16 f32 / 8 f64 | SIMD width × register file (128-bit NEON, 32 regs) |
| `TILE_I` | 4 | accumulator vectors that fit before spilling |
| `TILE_KC` | 128 | an L2 budget for one k-panel |
| `tile_nc_for` | `TJ × 32` | its own doc says *"256 KB … the L2 budget that keeps the panel resident"* — a zen3 number |
| `HEAP_MIN_BYTES` | 256 KB | a stack-ceiling policy |
| `GRAIN` (mapal-rt) | 4096 | slice size vs core count — the same disease at the runtime placement |

S30 measured the consequence: with both legs finally emitted optimally, the KC nest still
loses on M4 Pro at every size **and the deficit grows with N** (+5% @1024 → +14% @4096).
`NC` was sized for 512 KB of per-core L2; this machine has 16 MB shared. The rung is not
wrong — it is *parameterized for a different machine*, and the emitter has no way to know.

**Sharpening, from the same data:** a table of better numbers is necessary but not
sufficient. At 4096 the working set is ~192 MB — past every cache on any target — so
whether a rung *applies at all* is itself target- and shape-dependent. The profile must
therefore be consulted for the **gate**, not only for the constants.

## Categorical model

The profile is a `Cmp`-level configuration object, not a `Dat` of the language. Its
defining property, straight from ADR-0032: **every field is value-invariant** — changing
one changes how fast the answer arrives, never the answer. That is what keeps the
differential suite a valid gate under every profile, and it is the line between this and
the type system's precision contracts.

Selection is the framework's **strategy shape** (§4.4): the rungs are parallel `TrnLoc`s
over one contract, and the profile is the resolver that picks among them — the same
2-category as the codec/adapter pattern, with machine facts as the key instead of a name.

| Item | Kind | Model |
| --- | --- | --- |
| `TargetProfile` | `Cmp` config | `{ name, vec_bytes, vec_regs, acc_vecs_per_row, nc_tiles, l2_bytes, heap_min_bytes }` — machine facts + two policy ratios, nothing program-specific |
| `tile_j`, `tile_i`, `tile_kc`, `nc`, `heap_min` | **deduced** morphisms `TargetProfile × Ty → ℕ` | derived per §5 "deduce, don't store": today's six literals become one table plus arithmetic |
| profile → rung gate | `Trn` (the resolver) | `kc_nest_applies(profile, site)` replaces the `EmitOpts::kc_nest` bool as the *default*; the flag survives only as an explicit override |

**The derivations** (they must reproduce today's numbers exactly for the default profile —
that is the correctness gate for this change):

```
lanes(elem)   = vec_bytes / sizeof(elem)              ; f32 4, f64 2   (vec_bytes = 16)
tile_j(elem)  = lanes(elem) × acc_vecs_per_row        ; f32 16, f64 8  (acc_vecs_per_row = 4)
tile_i        = vec_regs / (2 × acc_vecs_per_row)     ; 4             (vec_regs = 32)
              ; the `2 ×` is the headroom policy: spend at most HALF the vector
              ; register file on accumulators, leaving the rest for the shared b
              ; tile, the a splat and the products. It reproduces S26's swept
              ; result INCLUDING its failure — TI=8 needs 32 accumulator
              ; registers, and the sweep recorded "8 spills: 128 accumulators ≫
              ; 32 NEON regs" (sessions/2026-07-23-s26-register-blocking.md:59)
nc(elem)      = tile_j(elem) × nc_tiles               ; f32 512, f64 256 (nc_tiles = 32)
panel_budget  = l2_bytes / 2
tile_kc(elem) = panel_budget / (nc(elem) × sizeof(elem))
                                                      ; 512 KB/2 ÷ (512×4) = 128  ✓
```

The last line is the point. On a 512 KB-per-core L2 it yields `TILE_KC = 128` — today's
literal, reproduced. On this M4 Pro (`hw.perflevel0.l2cachesize` = 16 MB) it yields
`kc ≈ 4096 ≥ K`, so `site.k > tile_kc` is false and **the KC nest disables itself**. The
measured behavior falls out of the model instead of being hardcoded as a default-off flag.

**Composition rules.**
1. The default profile emits **byte-identical** text to today for every shape. This is
   checked, not asserted: goldens must not move and the differential must stay green.
2. Every profile field is value-invariant. A profile change may not alter any output bit —
   the differential suite is run under a non-default profile at least once to prove it.
3. Profiles are selected **by name**, never implicitly. `native` is one named entry that
   probes the host; nothing probes the host unless asked. Emission stays reproducible and
   cross-compilable by default, and a box run names `zen3` rather than inheriting whatever
   machine the build happened on.
4. `GRAIN` belongs to the same table conceptually but lives at a different placement
   (mapal-rt, runtime). Out of scope here; recorded so the split is deliberate.

## Work items (backend-llvm)

1. `TargetProfile` + a `PROFILES` table: `generic` (default — today's numbers exactly),
   `apple-m` (vec 16 B, 32 regs, 16 MB L2), `zen3` (AVX2: vec 32 B, 16 regs, 512 KB L2 —
   **untested, marked as such**), `native`.
2. Replace `tile_j_for`, `tile_nc_for`, `TILE_I`, `TILE_KC`, `HEAP_MIN_BYTES` with profile
   lookups. The constants stop existing as literals. **`TILE_I` is two quantities sharing a
   number and only one of them is `profile.tile_i`:** `func.rs:3016/4074/4806` (matmul) is a
   row-block count over `phi <TJ x elem>` accumulators, which is what the register budget
   bounds; `func.rs:3127/3165/3201/3290/3377` (FIR window rung) is a *lane*-block multiplier
   over a `[TI·TJ x elem]` **memory** accumulator — `emit_tile_trio_vec` is unreachable from
   `emit_tile_window_block`, so no register-file constraint binds it. The FIR use becomes its
   own named constant at its current value (`WINDOW_SUBROWS = 4`), marked unjustified, rather
   than silently inheriting a derivation from a constraint that does not apply to it.
3. `native`: probe `hw.l1dcachesize` / `hw.perflevel0.l2cachesize` (Darwin), `/sys/devices/
   system/cpu/cpu0/cache/index*/size` (Linux), vector width from target features. Explicit
   fallback to `generic` when a probe fails — never a silent guess. **`vec_regs` is not
   probed**: it is an architectural constant of the ISA (NEON 32, AVX2 16, AVX-512 32), so it
   comes from the target features with the vector width, not from a sysctl. Naming it here
   because "probe the host" would otherwise imply a runtime query that does not exist.
4. `EmitOpts::target: &'static str` (default `generic`) + `--target=` on the emit example.
   `EmitOpts::kc_nest` becomes a tri-state override (auto / force-on / force-off) whose
   *auto* is `profile.kc_nest_applies(site)`.

## Tests

- **Byte-identity under the default profile** — the whole existing golden set must not
  move. That is rule 1 and it is the main safety property.
- A golden per non-default profile pinning that the numbers actually change (`apple-m`
  must show the KC gate closed; `zen3` must show `TILE_KC = 128`).
- The differential suite run once under `apple-m` — same outputs, different schedule
  (rule 2: value-invariance is a claim, so it gets a test).
- `native` on this machine resolves to `apple-m`-like values and, notably, disables the KC
  nest by derivation — the S30 measurement reproduced as a *deduction*.

## ADR-0033 D2 — the three-line record

- **Record fields consumed:** `TileSite.{k, elem}` for the gate and the per-width derivations;
  `TileSite.{rows, c}` are read by the rungs, not by the profile. The profile itself consumes
  **no** graph facts — that is the point of the split, and it is why nothing here touches
  mapal-ir.
- **CUDA realization against the record:** the same resolver shape with different fields —
  registers-per-thread and shared-memory bytes in place of `vec_regs`/`l2_bytes`, selecting
  smem tile sizes and thread-tile extents. Named, not executed: `tile_plan` still has exactly
  one consumer (`backends/llvm`), and `backends/cuda/src/` contains no reference to it.
- **Machine facts the record does not carry:** vector width, vector register count, L2 bytes,
  the stack ceiling. All four are what this plan exists to make data.

## Ceilings (recorded, not built)

- **The thread-count deduction needs fields this record does not have.** `docs/next-session.md`
  §"S31 focus" sources "core count, P/E split and their throughput ratio" from `TargetProfile`;
  no such field is proposed here. Those are facts about a *runtime* placement (mapal-rt's pool),
  the same side of rule 4 as `GRAIN` — so either this record grows a runtime half or the pool
  gets its own profile. Recorded as a collision to settle when that item starts, not resolved
  here.
- **The `zen3` row moves two constants at once.** With `vec_bytes = 32`, `vec_regs = 16` the
  derivations give `tile_j(f32) = 32` (today 16) and `tile_i = 2` (today 4) — a doubled tile
  width and a halved row block, simultaneously, on a machine nobody has measured. It is
  self-consistent arithmetic, not a validated configuration; the box leg is what settles it,
  and until then `zen3` must stay explicitly untested rather than shipping as "the AVX2 answer".
- `GRAIN` (mapal-rt) stays hand-set — different placement, same disease (rule 4).
- The `zen3` profile's values are read off documentation, not measured. The box leg is
  what validates them, and until then the profile is labeled untested.
- Autotuning (ADR-0034) is explicitly out of scope. This plan builds the table the
  autotuner would write into; whether the values are searched or set is that ADR's call,
  and it is still a candidate owned by another session.
- `acc_vecs_per_row` and `nc_tiles` remain policy ratios rather than derived facts —
  they encode "how much of the register file to spend on accumulators" and "how wide a
  j-block", which are search space, not machine facts.

---

## As built (S31) — where the code went, and four deviations

`crates/backends/llvm/src/profile.rs` (new): `TargetProfile` + `GENERIC`/`APPLE_M`/`ZEN3`
+ `resolve`/`names`, with the five derivations as methods (`lanes`, `tile_j`, `tile_i`, `nc`,
`tile_kc`). `EmitOpts::target: &'static str` (default `generic`) resolves once in
`emit_with_opts`; `FnEmit` carries `profile: &'static TargetProfile`; `TileCtx` gained
`tile_i`/`tile_kc` beside the `tile_j` it already carried, so the deep helpers read them the
same way. `tile_j_for`, `tile_nc_for`, `TILE_I`, `TILE_KC` and `HEAP_MIN_BYTES` no longer
exist. `--target=<name>` on the emit example.

**The headline property works and is pinned.** `golden_ll::profile_closes_the_kc_gate_by_derivation`:
at K=300, `generic` (512 KB L2 ⟹ kc=128) opens the KC gate and `apple-m` (16 MB ⟹ kc=4096)
does not — and in its strong form, **apple-m WITH the nest requested is byte-equal to generic
WITHOUT it**, so a closed gate falls back to the real j-outer nest rather than a
differently-shaped near-miss.

**Stated honestly (Sapir's catch, mid-session): this is a THRESHOLD, not an off-switch.**
`site.k > tile_kc` closes at every K we run because apple-m's panel is 4096 — but at
`K = 8192` the gate reopens, verified by emission (`alloca [16384 x float], align 64`, the
`tile_i × tile_kc` a-panel). Two consequences, both recorded rather than fixed:

- **Past the threshold the derivation disagrees with the measurement.** S30 measured this
  nest losing at every size on this machine *with the deficit growing in N* (+5% @1024 →
  +14% @4096) — the opposite of what the traffic model behind `tile_kc` predicts. So the
  formula is sound as "how deep a panel fits in L2" and NOT as "when the nest pays"; the two
  coincide only below the threshold. `kc_nest` staying default-OFF is what keeps the
  disagreement out of shipped builds, and settling it needs the box leg (suggestions #16),
  not more arithmetic. Pinned by `profile::tests::apple_m_raises_the_kc_threshold_above_every_k_we_run`,
  whose `8192` assertion exists so the other three are not misread.
- **The a-panel scales with L2**: `tile_i × tile_kc × sizeof` reduces to `2 · l2_bytes /
  nc_tiles` — 2 KB under `generic`, **64 KB under `apple-m`**. `func.rs`'s heap note claimed a
  flat 2 KB for "the largest tile scratch the emitter mints"; that number was corrected in the
  same change (the conclusion — under 256 KB, stays on the stack — survives) and pinned by
  `profile::tests::kc_apanel_scales_with_l2`. A profile past ~64 MB of L2 would push it over
  `heap_min_bytes`; no shipped profile is close.

### Deviations

1. **`native` NOT built** (work item 3). Nothing measurable this session needs it — `apple-m`
   is this machine and `zen3` is the box — and rule 3 already says nothing probes unless
   asked. A probe that silently half-succeeds (L2 read, register count guessed) is worse than
   no probe, and `vec_regs` is not probeable anyway. Deferred with its reason rather than
   shipped thin.
2. **`EmitOpts::kc_nest` stays a bool, not a tri-state** (work item 4). The plan's *auto* =
   `kc_nest_applies(profile, site)` **would have broken rule 1**: under `generic` (kc=128)
   every K>128 site would enable the nest by default, changing emission for every real matmul
   and regressing S30's measured +5…14%. What *auto* was reaching for is delivered instead by
   the profile joining the existing gate — `self.kc_nest && packed.is_some() && site.k >
   tile_kc` — so `--kc` on `apple-m` correctly does nothing. The tri-state was complexity for
   a behavior the derivation already provides.
3. **`TILE_I` split in two, not renamed once** (revised work item 2). `TargetProfile::tile_i`
   is the matmul rung's row block; `func.rs:WINDOW_SUBROWS` is the FIR window rung's lane
   block over a **memory** accumulator, which no register budget bounds. Same value today,
   different quantities, each set at its own source.
4. **The byte-identity gate could not be `regen.sh`.** The 72 checked-in `benches/matmul/*.ll`
   are **stale at HEAD** — proven by stashing this change and regenerating with unmodified
   HEAD code, which also differs (they predate S30b's `time` migration of the `.mapal`
   sources; `regen.sh` additionally exits 1 on the CUDA leg, which rejects `time` — the
   recorded ✋ cell). Rule 1 was therefore checked the honest way: **A/B emission against HEAD
   over 11 sources × 6 flag combinations = 66 emissions, all byte-identical** (matmul
   f32/f64/128/4096, fir 1M, conv2d 1024, attn 256 + the rowmajor refusal, three examples;
   with and without `--contract`, `--kc`, `--no-pack`, `--no-tile`, `--perf`). Plus the 29
   pre-existing goldens unmoved and the differential suite green. **Refreshing the stale
   bench `.ll` is a separate, pre-existing debt** — not folded into this change, because
   doing so would have hidden exactly the drift that proves rule 1.

### Corrections to the plan text above

- The test bullet "`zen3` must show `TILE_KC = 128`" is **wrong**: zen3's `nc(f32)` is 1024
  (TJ 32 × 32), so `tile_kc(f32) = (512 KB/2)/(1024×4) = 64`. Pinned at 64 in
  `profile::tests::zen3_moves_two_constants_at_once`'s neighbourhood; the row remains
  untested on hardware either way.
- **"the KC nest disables itself" (the derivations block) overstates it.** `site.k > tile_kc`
  is a threshold: it closes for every K we run on `apple-m` (kc = 4096) and **reopens at
  K = 8192**, verified by emission. Corrected wording and the reasoning are in the As-built
  section above; the pre-build text is left as written.
- `packed_type` became a free function taking `&TargetProfile` (it had no `self`).
