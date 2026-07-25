# Plan — S31: `TargetProfile` — the emitter's machine facts become data

**Status:** written pre-build, S31. Sapir: *"the backend needs to employ a strategy pattern
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
| `GRAIN` (flow-rt) | 4096 | slice size vs core count — the same disease at the runtime placement |

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
nc(elem)      = tile_j(elem) × nc_tiles               ; f32 512, f64 256 (nc_tiles = 32)
panel_budget  = l2_bytes / 2
tile_kc(elem) = panel_budget / (nc(elem) × sizeof(elem))
                                                      ; 512 KB/2 ÷ (512×4) = 128  ✓
```

The last line is the point. On a 512 KB-per-core L2 it yields `TILE_KC = 128` — today's
literal, reproduced. On this M4 Pro (`hw.perflevel0.l2cachesize` = 16 MB) it yields
`kc ≈ 4096 ≥ K`, so `site.k > tile_kc` is false and **the KC nest disables itself**. The
measured behaviour falls out of the model instead of being hardcoded as a default-off flag.

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
   (flow-rt, runtime). Out of scope here; recorded so the split is deliberate.

## Work items (backend-llvm)

1. `TargetProfile` + a `PROFILES` table: `generic` (default — today's numbers exactly),
   `apple-m` (vec 16 B, 32 regs, 16 MB L2), `zen3` (AVX2: vec 32 B, 16 regs, 512 KB L2 —
   **untested, marked as such**), `native`.
2. Replace `tile_j_for`, `tile_nc_for`, `TILE_I`, `TILE_KC`, `HEAP_MIN_BYTES` with profile
   lookups. The constants stop existing as literals.
3. `native`: probe `hw.l1dcachesize` / `hw.perflevel0.l2cachesize` (Darwin), `/sys/devices/
   system/cpu/cpu0/cache/index*/size` (Linux), vector width from target features. Explicit
   fallback to `generic` when a probe fails — never a silent guess.
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

## Ceilings (recorded, not built)

- `GRAIN` (flow-rt) stays hand-set — different placement, same disease (rule 4).
- The `zen3` profile's values are read off documentation, not measured. The box leg is
  what validates them, and until then the profile is labelled untested.
- Autotuning (ADR-0034) is explicitly out of scope. This plan builds the table the
  autotuner would write into; whether the values are searched or set is that ADR's call,
  and it is still a candidate owned by another session.
- `acc_vecs_per_row` and `nc_tiles` remain policy ratios rather than derived facts —
  they encode "how much of the register file to spend on accumulators" and "how wide a
  j-block", which are search space, not machine facts.
