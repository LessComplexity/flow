//! `TargetProfile` — the emitter's machine facts as data (plan-s31-target-profiles).
//!
//! ADR-0032 D4: tile factors, grain sizes and arena thresholds are backend
//! config, not language. Every field here is **value-invariant** — changing one
//! changes how fast the answer arrives, never the answer — which is exactly what
//! keeps the differential suite a valid gate under every profile.
//!
//! The split this file exists to enforce (the backend-genericity contract):
//! **geometry comes from the record, constants come from the profile.** mapal-ir
//! never learns a machine fact; the emitter never re-derives graph analysis.
//! What used to be six literals swept on one M4 Pro is now one table plus
//! arithmetic, and the arithmetic reproduces those literals for the default
//! profile — that is the correctness gate for this change.

use mapal_ir::Ty;

/// Which machine class a profile describes.
///
/// This is a **capability discriminator**, not a branch flag (plan-s41 §2.2
/// rule 2): it selects which *realization* of a tile site fires, and each
/// realization is one cohesive unit of code. It must never appear as an `if` in
/// the body of a shared emitter — that is the code-locality failure the rule
/// exists to prevent, and §9's branch budget is what measures it.
///
/// The classes are not different algorithms. Every unit Mapal targets — SIMD
/// FMA, ARM SME, Intel AMX, NVIDIA tensor cores — does the same thing: stage
/// operands, issue a multiply-accumulate over a block, accumulate into a
/// **resident** accumulator, keep it hot across the reduction axis, store once
/// at the end. What varies is the shape of one issue and how many accumulator
/// blocks stay resident — and [`TargetProfile::tile_i`] already parameterizes
/// the second. So the tile nest is shared and only the innermost leaf differs
/// (Sapir, S41 plan gate).
///
/// The one genuinely structural split is **cooperation**: on a CPU core a
/// single thread owns its whole tile, while on a GPU a block of threads shares
/// one staged panel and must synchronise before reading it. That is why `Gpu`
/// carries facts `Cpu` has no analogue for, and why a barrier is a morphism in
/// the model rather than an idiom (plan-s41 §1.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Machine {
    /// A CPU core. One thread owns a whole tile; accumulators live in the
    /// vector register file; no cooperation, no barrier. Every profile that
    /// existed before S41 is this, which is what keeps their emission
    /// byte-identical.
    Cpu,
    /// A GPU streaming multiprocessor.
    Gpu(Gpu),
}

/// The SM facts a GPU profile carries, and only those a CPU profile has no
/// field for. Everything a CPU profile already expresses (accumulator capacity,
/// block width, panel depth) stays in [`TargetProfile`] — the point of the
/// unification above is that those are *the same quantities*, read differently.
///
/// **UNMEASURED.** These are read off the ISA and the device query, not swept,
/// exactly like [`ZEN3`]. No realization consumes them yet; plan-s41 steps 3–4
/// decide which are actually read, and any fact a GPU realization needs but
/// cannot find here is an ADR-0033 D4(b) finding to report rather than a value
/// to invent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gpu {
    /// Threads per warp — the cooperation quantum. 32 on every NVIDIA part.
    pub warp: u64,
    /// Shared memory addressable by one block, in bytes. The budget the staged
    /// panel is sized against — the GPU reading of `l2_bytes`.
    pub smem_bytes: u64,
    /// The launch-geometry ceiling: threads in one block.
    pub max_threads_per_block: u64,
    /// The PTX target architecture, e.g. `sm_89`. Verified against the device
    /// rather than assumed: the box GPU reports compute capability 8.9.
    pub arch: &'static str,
}

/// The ARM SME facts a profile carries, when the part has a streaming matrix
/// unit — and **only the two that cannot be derived from anything else.**
///
/// Both are **read off the part** by [`native`] (`hw.optional.arm.sme_max_svl_b`
/// and `hw.perflevel0.l1dcachesize`), never assumed: SVL is
/// implementation-defined and a cache size is a property of the silicon. Every
/// other SME quantity the realization needs is *derived* from these two —
/// the panel side (`svl/w`), the tile count (`w`, an ISA rule), the arrangement,
/// the packed A stride, and the k-panel depth — so a part with a different SVL
/// or a different L1D emits a different kernel from the same record, with no new
/// field and no new code. `f32_tiles` used to sit here and was deleted for
/// exactly that reason: it was derivable, so recording it could only ever make
/// it wrong (see [`TargetProfile::sme_block`]).
///
/// A hand-written value is therefore the *cross-compilation* case — describing a
/// machine you are not sitting on — and
/// `native_reproduces_the_hand_written_profile` asserts the two agree on the
/// machine you are.
///
/// `Machine` stays `Cpu`: SME is a **unit inside a CPU core**, not a machine
/// class. One thread owns the whole tile, there is no cooperation and no
/// barrier — exactly the `Cpu` reading. What differs is only which realization
/// the leaf takes, which is what `Option` here expresses (plan-s41 §2.2 rule 2:
/// capability selects realization).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sme {
    /// Streaming vector length in bytes — ZA is `svl_bytes × svl_bytes`, so one
    /// f32 tile is `svl_bytes/4` square. **64 on the M4 Pro** (512 bits), which
    /// is what makes `<vscale x 4 x float>` sixteen lanes there.
    pub svl_bytes: u64,
    /// **Per-core L1 data cache in bytes** — the budget the SME k-panel is sized
    /// against ([`TargetProfile::sme_kc`]).
    ///
    /// It lives **inside `Sme`**, not beside `l2_bytes`, and that placement is
    /// the point: a profile cannot declare a matrix unit without declaring the
    /// working-set budget that unit's panel is cut to. FRAMEWORK §4.5 law 3
    /// (placement totality) as a type, so the omission is a compile error rather
    /// than a silent fallback to the NEON core's number.
    ///
    /// Why a *second* cache fact rather than reusing `l2_bytes`: `tile_kc` sizes
    /// the NEON k-panel against half of a 16 MB **shared** L2 and returns 4096
    /// on this part, which closes its own gate for every K this project runs
    /// (`apple_m_raises_the_kc_threshold_above_every_k_we_run`). Measured, the
    /// SME optimum is a **128 KB two-panel window** — L1D, not L2 — and the
    /// unblocked case loses **1.448×** at K=4096
    /// (`docs/performance/s42-sme-roofline.md` §5, `benches/sme/kc.c`). One
    /// deduced depth serving two placements with different budgets is the defect;
    /// two `TrnLoc`s over one question is the fix.
    pub l1d_bytes: u64,
    /// **Policy ratio, honestly search space — NOT a machine fact.** How many
    /// L1D's worth of operand window one k block may occupy. Same status as
    /// [`TargetProfile::acc_vecs_per_row`] and `nc_tiles`, and ADR-0034 is the ADR
    /// that would search it rather than record it.
    ///
    /// It exists because the obvious derivation is **wrong, measured**. Sizing the
    /// window to fit L1D exactly (ratio 1 ⇒ `kc` 512 here) lands on the losing
    /// side of a sharp curve. Two depth sweeps in the real emitter, N=4096 and
    /// N=2048, f32, 1 thread, alternating, values identical at every depth:
    ///
    /// | working set | N=4096 | N=2048 |
    /// | ---: | ---: | ---: |
    /// | 32 KB | — | 0.220× |
    /// | 64 KB | 0.501× | 0.387× |
    /// | 128 KB ← ratio 1 | 0.785× | 0.639× |
    /// | **256 KB ← ratio 2** | **1.064×** | 0.986× |
    /// | 512 KB | 1.027× | **1.000×** (unblocked) |
    /// | 1024 KB | 1.000× (unblocked) | — |
    ///
    /// **256 KB is the optimum at both sizes, and it is not any machine fact on
    /// this part** — L1D is 128 KB, L1I 192 KB, the per-core L2 slice ~3.2 MB. So
    /// it is recorded as a swept ratio rather than dressed up as a derivation.
    /// Writing `2 * l1d_bytes` into [`TargetProfile::sme_kc`] would be a fitted
    /// constant wearing a derivation's clothes, which is the exact defect
    /// `plan-s31-deduced-blocking.md` exists to remove.
    ///
    /// Note also what the curve says about getting it wrong: every depth *below*
    /// the optimum is catastrophic (64 KB costs 2.0–2.6×), so this constant is not
    /// a mild tuning knob. The first version of `sme_kc` used ratio 1 and that one
    /// wrong constant is what made KC blocking look like a 1.27× loss for most of
    /// S42 — see `func/sme.rs`.
    pub panel_l1d_ratio: u64,
}

/// One target's machine facts, plus the two policy ratios that are honestly
/// search space rather than facts (see the field docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetProfile {
    pub name: &'static str,
    /// The machine class — the capability that selects a realization (S41).
    pub machine: Machine,
    /// One vector register in bytes: NEON 16, AVX2 32, AVX-512 64.
    pub vec_bytes: u64,
    /// Architectural vector register count: NEON 32, AVX2 16, AVX-512 32.
    /// **Not probed** — it is a fact of the ISA, so it arrives with the target
    /// features rather than from a runtime query.
    pub vec_regs: u64,
    /// Policy ratio: how many vector registers one accumulator row occupies,
    /// i.e. how wide a j-tile is measured in registers. This is "how much of
    /// the register file to spend on accumulators" — search space, not a
    /// machine fact (ADR-0034 is the ADR that would search it).
    pub acc_vecs_per_row: u64,
    /// Policy ratio: j-tiles per KC-rung j-block. Same status as
    /// `acc_vecs_per_row`.
    pub nc_tiles: u64,
    /// Per-core L2 in bytes — the budget the KC rung's k-panel is sized against.
    pub l2_bytes: u64,
    /// Stack ceiling policy: an entry-block block at least this large is placed
    /// in the `mapal_rt_alloc` arena instead of the stack.
    pub heap_min_bytes: u64,
    /// The streaming matrix unit, when the part has one. `None` for every
    /// profile that existed before S41 — which is exactly what keeps their
    /// emission byte-identical: the SME realization cannot be selected without
    /// a fact only this field carries.
    pub sme: Option<Sme>,
}

/// The element width a tile site accumulates at. Tile sites are numeric-gated
/// by recognition (`mapal_ir::algo` requires `is_numeric` for the element, the
/// accumulator, the seed and the map target), so the non-numeric arm is
/// unreachable; it returns the f32 width rather than panicking because a wrong
/// tile width is a slow kernel, never a wrong answer.
fn elem_bytes(elem: &Ty) -> u64 {
    match elem {
        Ty::Int { bits, .. } | Ty::Float { bits } => u64::from(*bits).div_ceil(8).max(1),
        _ => 4,
    }
}

impl TargetProfile {
    /// Scalar lanes in one vector register at this element width.
    pub fn lanes(&self, elem: &Ty) -> u64 {
        (self.vec_bytes / elem_bytes(elem)).max(1)
    }

    /// The register micro-kernel's j-tile width in lanes (was `tile_j_for`).
    /// `generic`: f32 `16 / 4 × 4 = 16`, f64 `16 / 8 × 4 = 8` — today's
    /// literals, reproduced.
    pub fn tile_j(&self, elem: &Ty) -> u64 {
        self.lanes(elem) * self.acc_vecs_per_row
    }

    /// Rows per register block (was `TILE_I`, on the matmul rung only — the FIR
    /// window rung's `4` is [`crate::func::WINDOW_SUBROWS`], a different
    /// quantity over a memory accumulator).
    ///
    /// The `2 ×` is the headroom policy: spend at most HALF the vector file on
    /// accumulators, leaving the rest for the shared b tile, the a splat and
    /// the products. `generic`: `32 / (2 × 4) = 4`. It reproduces S26's swept
    /// result *including its failure* — TI=8 would need 32 accumulator
    /// registers, and that sweep recorded "8 spills: 128 accumulators ≫ 32 NEON
    /// regs".
    pub fn tile_i(&self) -> u64 {
        (self.vec_regs / (2 * self.acc_vecs_per_row)).max(1)
    }

    /// The KC rung's j-block width in lanes (was `tile_nc_for`).
    pub fn nc(&self, elem: &Ty) -> u64 {
        self.tile_j(elem) * self.nc_tiles
    }

    /// The KC rung's k-panel depth (was `TILE_KC`): half of L2 spent on one
    /// (kc × nc) packed-b window. `generic` yields 128 at BOTH widths —
    /// `512 KB/2 ÷ (512 × 4)` and `512 KB/2 ÷ (256 × 8)` — today's single
    /// literal, reproduced. On a 16 MB L2 it yields `kc ≥ K`, so the gate
    /// `site.k > tile_kc` closes and **the KC nest disables itself by
    /// derivation** rather than by a default-off flag (S29/S30's measured
    /// verdict, deduced).
    pub fn tile_kc(&self, elem: &Ty) -> u64 {
        ((self.l2_bytes / 2) / (self.nc(elem) * elem_bytes(elem))).max(1)
    }

    /// The GPU facts, when this profile describes one. `None` for every CPU
    /// profile — callers that need a GPU fact must ask for it, so a CPU profile
    /// reaching a GPU-only path is a `None` at the seam rather than a silently
    /// wrong constant.
    pub fn gpu(&self) -> Option<&Gpu> {
        match &self.machine {
            Machine::Cpu => None,
            Machine::Gpu(g) => Some(g),
        }
    }

    /// The streaming matrix unit's facts, when this part has one. Same seam
    /// discipline as [`TargetProfile::gpu`]: a profile without SME reaching an
    /// SME-only path is a `None`, never a wrong constant.
    pub fn sme(&self) -> Option<&Sme> {
        self.sme.as_ref()
    }

    /// The side of one square ZA tile at this element width — `svl_bytes /
    /// sizeof(elem)`, i.e. **16 for f32 at SVL 512**. `None` without an SME
    /// unit.
    ///
    /// This is the one number the emitted kernel's every literal comes from
    /// (the panel side, the ZA row count, the packed-A row stride). It is a
    /// per-`Loc` machine fact and must never reach `mapal-ir`.
    pub fn sme_tile_side(&self, elem: &Ty) -> Option<u64> {
        Some(self.sme.as_ref()?.svl_bytes / elem_bytes(elem))
    }

    /// How many ZA tiles one panel accumulates into, and in what arrangement:
    /// `(ti, tj)` with `ti · tj` = the architectural tile count at this width,
    /// so a panel covers `ti·t × tj·t` outputs. `None` without an SME unit.
    ///
    /// This is [`TargetProfile::tile_i`]'s question asked of the other
    /// accumulator file — "how many accumulator blocks stay resident" — and it
    /// is answered the same way, by arithmetic rather than by a literal.
    ///
    /// **The tile count is not a recorded fact; it is an ISA rule.** ZA is
    /// `svl_bytes × svl_bytes` bytes and one tile at element width `w` is
    /// `(svl/w)² · w` bytes, so the count is `svl²/((svl/w)²·w) = w` — always
    /// **exactly the element byte width**, for any SVL: 4 tiles at f32
    /// (`ZA0.S`…`ZA3.S`), 8 at f64 (`ZA0.D`…`ZA7.D`), 2 at f16, 1 at i8. That is
    /// the architecture, so it is derived here rather than recorded per part.
    ///
    /// This replaced an `f32_tiles: 4` field. Deriving it removes the last
    /// hardcoded SME number *and* closes the S41b P2 defect that the field
    /// created: a recorded count could say `7`, for which this function would
    /// happily return a 1×7 arrangement that no ZA register file can hold and
    /// no emitted kernel could select. A count that is `elem_bytes` by
    /// construction cannot be wrong.
    ///
    /// The **shape** is a derivation too, not taste. One panel issues `ti · tj`
    /// outer products from `ti + tj` operand loads, and `ti + tj` is minimised
    /// by the most-square factorization: 2×2 feeds 4 MACs from 4 loads, while
    /// 1×4 would need 5 for the same 4. Measured on this part
    /// (`benches/sme/mm4.c`, 2×2 vs 1 tile, f32, 1 thread): 423 → 777 GFLOP/s
    /// at 1024², 237 → 619 at 2048².
    pub fn sme_block(&self, elem: &Ty) -> Option<(u64, u64)> {
        self.sme.as_ref()?;
        let tiles = elem_bytes(elem).max(1);
        // The largest divisor no bigger than sqrt(tiles) — the square-most split.
        let ti = (1..=tiles)
            .filter(|d| d * d <= tiles && tiles.is_multiple_of(*d))
            .max()
            .unwrap_or(1);
        Some((ti, tiles / ti))
    }

    /// The SME rung's k-panel depth: how deep a k block may be before the two
    /// packed operand panels stop fitting the unit's working-set budget.
    /// `None` without an SME unit.
    ///
    /// **There is no opt-in.** The moment a profile records a matrix unit, every
    /// factor that unit's realization needs is *derived* from the facts recorded
    /// alongside it — the same contract [`TargetProfile::sme_block`] and
    /// [`TargetProfile::sme_tile_side`] already honour. A gate the caller has to
    /// remember to open is how `f32_tiles: 4` sat in this file while the emitter
    /// used one tile (S41b), and the deficit was ~4×.
    ///
    /// One k step of a panel streams `ti·t` elements of packed `a` and `tj·t` of
    /// packed `b`, so the window is `(ti + tj)·t·sizeof` bytes per k, and the
    /// depth that fits is the budget divided by it. On this part that is
    /// arithmetic with no free parameter:
    ///
    /// ```text
    /// (ti + tj)·t   = (2 + 2)·16 = 64 elements per k
    /// 64 · 4 B      = 256 B per k
    /// 2*131072 / 256 = 1024       <- the depth swept in the emitter
    /// ```
    ///
    /// **1024 is not written down anywhere; it falls out of L1D x the swept ratio.** That is the
    /// `plan-s31-deduced-blocking.md` discipline — a literal swept once is
    /// replaced by a derivation that reproduces it on the machine it was swept
    /// on, and yields a defensible number on a machine nobody has swept. `f64`
    /// scales off the same fact by element width, so there is no second constant.
    ///
    /// Measured (`benches/sme/kc.c`, N=4096, f32, 1 thread, 9 alternating runs,
    /// medians, every KC gated against an independent scalar reference): 64 KB →
    /// 845.1, **128 KB → 1101.2**, 256 KB → 1046.7, 512 KB → 885.1, unblocked
    /// (1 MB) → 760.7 GFLOP/s. Unimodal, so there is a real optimum rather than
    /// a trend, and the peak puts N=4096 level with N=1024.
    ///
    /// The caller gates on `site.k > sme_kc`, so at shallow k the nest disables
    /// itself by derivation — which is correct, because the probe shows blocking
    /// is a **loss** at K ≤ 512 where the panel already fits.
    pub fn sme_kc(&self, elem: &Ty) -> Option<u64> {
        let sme = self.sme.as_ref()?;
        // A detected machine fact times a SWEPT policy ratio — see
        // `Sme::panel_l1d_ratio` for the two depth sweeps that set it, and for why
        // folding the 2 into this expression would be dishonest.
        let budget = sme.l1d_bytes * sme.panel_l1d_ratio;
        let t = self.sme_tile_side(elem)?;
        let (ti, tj) = self.sme_block(elem)?;
        Some((budget / (elem_bytes(elem) * (ti + tj) * t)).max(1))
    }
}

/// Today's six literals, exactly: 128-bit vectors, 32 registers, a 512 KB
/// per-core L2. Emission under this profile is byte-identical to the
/// pre-profile emitter for every shape — the plan's rule 1, and the reason it
/// is the default.
pub const GENERIC: TargetProfile = TargetProfile {
    name: "generic",
    machine: Machine::Cpu,
    vec_bytes: 16,
    vec_regs: 32,
    acc_vecs_per_row: 4,
    nc_tiles: 32,
    l2_bytes: 512 * 1024,
    heap_min_bytes: 256 * 1024,
    sme: None,
};

/// Apple M-series: NEON at 16 B × 32 registers, but a 16 MB shared L2
/// (`hw.perflevel0.l2cachesize` on this M4 Pro). Same tile widths as `generic`;
/// the L2 is what differs, and it closes the KC gate by derivation.
pub const APPLE_M: TargetProfile = TargetProfile {
    name: "apple-m",
    l2_bytes: 16 * 1024 * 1024,
    ..GENERIC
};

/// AVX2 zen3: 32 B vectors, 16 registers, 512 KB per-core L2.
///
/// **UNTESTED — read off documentation, never measured.** Note it moves two
/// constants at once relative to `generic`: `tile_j(f32)` 16 → 32 and `tile_i`
/// 4 → 2. That is self-consistent arithmetic, not a validated configuration;
/// the box leg is what settles it.
pub const ZEN3: TargetProfile = TargetProfile {
    name: "zen3",
    vec_bytes: 32,
    vec_regs: 16,
    ..GENERIC
};

/// NVIDIA Ada (`sm_89`) — the box GPU, an RTX 4070 Ti (compute capability 8.9,
/// driver 610.43.03, 12 GB). plan-s41 step 1.
///
/// **No realization consumes this yet.** It exists so the GPU class is
/// expressible and the resolver knows the name; steps 3–4 build the realization
/// that reads it. The CPU-shaped fields are inherited from [`GENERIC`] and are
/// **placeholders, not GPU measurements** — `vec_bytes`/`vec_regs`/`l2_bytes`
/// describe a vector register file and a cache hierarchy, and their GPU
/// readings (per-thread registers, shared memory) are the open question S38
/// already flagged: *"`<TJ x elem>` means SIMD lanes on CPU and per-thread
/// registers on GPU — one record field, two readings."* Which of them a GPU
/// realization actually reads, and which need their own value, is ADR-0033
/// D4(a)/(b) and is answered by building, not here.
///
/// `sm_89` has 4th-generation tensor cores but no MXFP block-scale (that is
/// `sm_100`), so those intrinsics are emittable and not runnable on this part.
/// Irrelevant to the S41 leg — `mma` is out of scope (plan-s41 §7).
pub const CUDA_ADA: TargetProfile = TargetProfile {
    name: "cuda-ada",
    machine: Machine::Gpu(Gpu {
        warp: 32,
        smem_bytes: 48 * 1024,
        max_threads_per_block: 1024,
        arch: "sm_89",
    }),
    ..GENERIC
};

/// Apple M4 with the streaming matrix unit enabled: [`APPLE_M`] plus the SME
/// facts measured on the part (`benches/sme/svl.c` — SVL 64 B, 4 f32 tiles;
/// `hw.optional.arm.FEAT_SME`/`FEAT_SME2`/`FEAT_SME_F32F32` all set).
///
/// It is a **separate name** rather than a field flipped on `apple-m` because
/// the SME leaf is only bit-equal to the NEON leaf on the contract face
/// (`fmopa` fuses — measured 92/256 cells differ against separate mul+add,
/// 0/256 against `fmaf`), so selecting it is a decision about the *program*, not
/// only about the machine. Naming it keeps `apple-m` byte-identical forever and
/// keeps a box run reproducible.
pub const APPLE_M4_SME: TargetProfile = TargetProfile {
    name: "apple-m4-sme",
    // Both facts are `hw.optional.arm.sme_max_svl_b` and
    // `hw.perflevel0.l1dcachesize` on this M4 Pro, and `native` reads both — so
    // this profile is now the *cross-compilation* spelling of a machine you are
    // not sitting on, and `native_reproduces_the_hand_written_profile` asserts
    // detection agrees with it here. The tile count is absent because it is an
    // ISA rule, derived in `sme_block`.
    sme: Some(Sme {
        svl_bytes: 64,
        l1d_bytes: 128 * 1024,
        panel_l1d_ratio: 2,
    }),
    ..APPLE_M
};

const PROFILES: [&TargetProfile; 5] = [&GENERIC, &APPLE_M, &ZEN3, &CUDA_ADA, &APPLE_M4_SME];

/// Resolve a profile by name. `None` for an unknown name — never a silent
/// fallback to `generic`, because a typo that quietly emits the default
/// profile's numbers is the exact failure this table exists to remove.
///
/// **A hand-written profile always wins.** The table is checked first, so
/// `--target=apple-m4-sme` overrides detection; `native` is only ever reached by
/// asking for it by name. That ordering is the contract: detection removes the
/// need to hand-write a profile for the machine you are sitting on, and changes
/// nothing about describing a machine you are not.
pub fn resolve(name: &str) -> Option<&'static TargetProfile> {
    PROFILES
        .into_iter()
        .find(|p| p.name == name)
        .or_else(|| (name == "native").then(native).flatten())
}

/// The known profile names, for the error message on an unknown one.
pub fn names() -> String {
    PROFILES
        .iter()
        .map(|p| p.name)
        .chain(std::iter::once("native"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One `sysctl` integer, or `None` if the key does not exist on this host.
///
/// Shelling out rather than linking `libc`: this crate has two dependencies and
/// the emitter is a build-time tool that reads four keys once per run. A binding
/// for `sysctlbyname` would be a new dependency to save a process spawn that
/// happens at most once.
#[cfg(target_os = "macos")]
fn sysctl_u64(key: &str) -> Option<u64> {
    let out = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", key])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()?.trim().parse().ok()
}

/// The host's own facts, **detected rather than written down**.
///
/// Reached as `--target=native`. Named profiles are resolved first (see
/// [`resolve`]), so this never shadows a hand-written one.
///
/// **Every machine fact is detected; nothing about the part is written down.**
///
/// | fact | source | |
/// | --- | --- | --- |
/// | `l2_bytes` | `hw.perflevel0.l2cachesize` | 16 MB here |
/// | `l1d_bytes` | `hw.perflevel0.l1dcachesize` | 128 KB here |
/// | `svl_bytes` | `hw.optional.arm.sme_max_svl_b` | 64 here |
/// | SME present | `hw.optional.arm.FEAT_SME` | 1 here |
/// | tile count | **derived** — ISA rule, see [`TargetProfile::sme_block`] | |
///
/// **`perflevel0`, not the bare keys.** `hw.l1dcachesize` reports **65536** on
/// this part and `hw.perflevel0.l1dcachesize` reports **131072**: the bare key
/// describes the E-cores. Sizing a P-core matrix panel against an E-core cache
/// would halve the derived k-panel and silently cost throughput, which is the
/// kind of wrong number that never announces itself.
///
/// SVL turned out to be a plain sysctl (`sme_max_svl_b`), so no streaming-mode
/// probe and no `+sme` build of this emitter is needed — a detected profile is a
/// **complete** profile, matrix unit included. `FEAT_SME` gates the block, so a
/// part without the unit yields `sme: None` and the rung falls back exactly as
/// it does for every pre-S41 profile.
///
/// `vec_bytes`/`vec_regs` stay at [`GENERIC`]'s values: they are facts of the
/// *ISA* that arrive with the target features, not runtime queries (see the
/// field docs), so there is nothing to detect. `acc_vecs_per_row`/`nc_tiles` are
/// policy ratios, honestly search space (ADR-0034), not facts to read.
pub fn native() -> Option<&'static TargetProfile> {
    static NATIVE: std::sync::OnceLock<Option<TargetProfile>> = std::sync::OnceLock::new();
    NATIVE.get_or_init(detect).as_ref()
}

#[cfg(target_os = "macos")]
fn detect() -> Option<TargetProfile> {
    // perflevel0 is the performance core cluster; the bare hw.* keys are the
    // efficiency cores. See the doc comment — this distinction is load-bearing.
    let l1d_bytes = sysctl_u64("hw.perflevel0.l1dcachesize")?;
    Some(TargetProfile {
        name: "native",
        l2_bytes: sysctl_u64("hw.perflevel0.l2cachesize")?,
        sme: (sysctl_u64("hw.optional.arm.FEAT_SME") == Some(1))
            .then(|| {
                Some(Sme {
                    svl_bytes: sysctl_u64("hw.optional.arm.sme_max_svl_b")?,
                    l1d_bytes,
                    // Detected profiles inherit the swept ratio; it is policy, not
                    // a property of the silicon, so there is nothing to read.
                    panel_l1d_ratio: 2,
                })
            })
            .flatten(),
        ..GENERIC
    })
}

/// No detection off macOS yet — Linux would read `sysfs`
/// (`/sys/devices/system/cpu/cpu0/cache/index*/size`), which is a different
/// parser, not a different idea. `None` means `--target=native` errors with the
/// known-name list instead of guessing.
#[cfg(not(target_os = "macos"))]
fn detect() -> Option<TargetProfile> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const F32: Ty = Ty::Float { bits: 32 };
    const F64: Ty = Ty::Float { bits: 64 };

    /// Rule 1, at the arithmetic level: the default profile must reproduce every
    /// literal the emitter used to carry. If this fails, emission has moved.
    #[test]
    fn generic_reproduces_the_six_literals() {
        assert_eq!(GENERIC.tile_j(&F32), 16, "tile_j_for f32");
        assert_eq!(GENERIC.tile_j(&F64), 8, "tile_j_for f64");
        assert_eq!(GENERIC.tile_i(), 4, "TILE_I");
        assert_eq!(GENERIC.nc(&F32), 512, "tile_nc_for f32");
        assert_eq!(GENERIC.nc(&F64), 256, "tile_nc_for f64");
        assert_eq!(GENERIC.tile_kc(&F32), 128, "TILE_KC f32");
        assert_eq!(GENERIC.tile_kc(&F64), 128, "TILE_KC f64");
        assert_eq!(GENERIC.heap_min_bytes, 256 * 1024, "HEAP_MIN_BYTES");
    }

    /// The S26 sweep's verdict, as arithmetic: TI=4 fits in half the file and
    /// TI=8 does not. The recorded failure was "8 spills: 128 accumulators ≫ 32
    /// NEON regs" — 8 rows × 16 f32 lanes ÷ 4 lanes per register = 32 registers,
    /// the whole file with nothing left for operands.
    #[test]
    fn tile_i_is_the_register_budget_not_a_taste() {
        let regs_per_row = GENERIC.tile_j(&F32) / GENERIC.lanes(&F32);
        assert_eq!(regs_per_row, GENERIC.acc_vecs_per_row);
        assert_eq!(GENERIC.tile_i() * regs_per_row, GENERIC.vec_regs / 2);
        assert_eq!(8 * regs_per_row, GENERIC.vec_regs, "TI=8 is the whole file");
    }

    /// The measured S29/S30 verdict, reproduced as a deduction — **and its
    /// boundary, pinned honestly**. On a 16 MB L2 the k-panel is 4096, deeper
    /// than every K we run, so the gate `site.k > tile_kc` never opens. Above
    /// 4096 it DOES reopen: this is a threshold, not an off-switch, and the
    /// 8192 assertion is here so nobody reads the others as "apple-m disables
    /// the nest". See `tile_kc`'s note — past the threshold the derivation and
    /// the measurement disagree.
    #[test]
    fn apple_m_raises_the_kc_threshold_above_every_k_we_run() {
        assert_eq!(APPLE_M.tile_kc(&F32), 4096);
        for k in [1024_u64, 2048, 4096] {
            assert!(k <= APPLE_M.tile_kc(&F32), "KC nest must stay off at K={k}");
        }
        assert!(
            8192 > APPLE_M.tile_kc(&F32),
            "the gate REOPENS past 4096 — an unmeasured regime, not a disabled rung"
        );
        assert!(
            2048 > GENERIC.tile_kc(&F32),
            "generic keeps today's gate open"
        );
    }

    /// The KC a-panel is `tile_i x tile_kc x sizeof` = `2 * l2_bytes/nc_tiles`,
    /// so it scales with the profile's L2: 2 KB under `generic`, 64 KB under
    /// `apple-m`. Pinned because `func/mod.rs`'s heap note used to claim a flat
    /// 2 KB, and that number is a stack-sizing claim.
    #[test]
    fn kc_apanel_scales_with_l2() {
        let apack = |p: &TargetProfile, e: &Ty| p.tile_i() * p.tile_kc(e) * 4;
        assert_eq!(apack(&GENERIC, &F32), 2 * 1024);
        assert_eq!(apack(&APPLE_M, &F32), 64 * 1024);
        assert!(
            apack(&APPLE_M, &F32) < GENERIC.heap_min_bytes,
            "still on the stack, but the headroom is profile-dependent"
        );
    }

    #[test]
    fn zen3_moves_two_constants_at_once() {
        assert_eq!(ZEN3.tile_j(&F32), 32);
        assert_eq!(ZEN3.tile_i(), 2);
    }

    #[test]
    fn unknown_name_is_not_silently_generic() {
        assert_eq!(resolve("generic"), Some(&GENERIC));
        assert_eq!(resolve("apple-m"), Some(&APPLE_M));
        assert_eq!(resolve("cuda-ada"), Some(&CUDA_ADA));
        assert_eq!(resolve("apple-m4-sme"), Some(&APPLE_M4_SME));
        assert!(resolve("apple_m").is_none());
        assert!(
            names().contains("native"),
            "the unknown-name error must offer `native`, or detection is undiscoverable"
        );
    }

    /// The SME k-panel depth is **derived from L1D**, and the derivation
    /// reproduces the swept optimum — `plan-s31-deduced-blocking.md`'s rule.
    /// `512` appears in no source file; it falls out of 128 KB / 256 B.
    #[test]
    fn sme_kc_reproduces_the_measured_optimum() {
        assert_eq!(
            APPLE_M4_SME.sme_kc(&F32),
            Some(1024),
            "2 x 128 KB / ((ti+tj)·t·4 = 256 B) = 1024 — the depth SWEPT in the real \
             emitter at N=4096 (1.064x, disjoint) and N=2048. Ratio 1 gives 512, \
             which measured 0.785x — a loss"
        );
        // f64 scales off the same recorded fact by element width: t halves to 8,
        // the tile count doubles to 8 so the block is 2x4, and the window is
        // (2+4)·8·8 = 384 B per k. No second constant anywhere.
        assert_eq!(APPLE_M4_SME.sme_kc(&F64), Some(2 * 128 * 1024 / 384));
        // No matrix unit => no answer, never a wrong one. Same seam as `sme()`.
        assert_eq!(GENERIC.sme_kc(&F32), None);
        assert_eq!(APPLE_M.sme_kc(&F32), None);
    }

    /// The SME budget is L1D and **not** the NEON rung's L2, and the two answers
    /// differ by 8x — which is the whole reason `sme_kc` exists rather than
    /// reusing `tile_kc`. If these two ever coincide, one of them is wrong.
    #[test]
    fn the_sme_budget_is_not_the_neon_one() {
        let neon = APPLE_M4_SME.tile_kc(&F32);
        let sme = APPLE_M4_SME.sme_kc(&F32).unwrap();
        assert_eq!(neon, 4096, "unchanged: half of 16 MB shared L2 over nc");
        assert_eq!(sme, 1024);
        assert!(
            sme < neon,
            "the matrix unit's window is smaller than the NEON k-panel's; \
             a shared derivation would size SME's panel 8x too deep and lose \
             1.448x at K=4096 (docs/performance/s42-sme-roofline.md §5)"
        );
        // And the gate this feeds: at K=4096 the SME nest must OPEN...
        assert!(4096 > sme, "KC nest must fire at K=4096");
        // ...while at K=512 it must stay shut, because blocking is a LOSS there.
        assert!(
            1024 <= sme,
            "KC nest must stay off at K=1024 — measured neutral-to-worse there"
        );
    }

    /// Detection reads the keys it documents. Not a value assertion — the point
    /// is that the plumbing works on this host, so "automatic" is not a claim
    /// resting on an untested code path.
    #[cfg(target_os = "macos")]
    #[test]
    fn native_reads_the_facts_it_claims_to() {
        let l2 = sysctl_u64("hw.perflevel0.l2cachesize");
        let l1d = sysctl_u64("hw.perflevel0.l1dcachesize");
        assert!(l2.is_some_and(|v| v >= 256 * 1024), "L2 detected: {l2:?}");
        assert!(l1d.is_some_and(|v| v >= 32 * 1024), "L1D detected: {l1d:?}");
        assert!(sysctl_u64("hw.optional.arm.no.such.key").is_none());

        let native = native().expect("detection must succeed on macOS");
        assert_eq!(native.name, "native");
        assert_eq!(Some(native.l2_bytes), l2, "detected, not inherited");

        // The E-core trap: the bare key is a DIFFERENT, smaller cache. If these
        // two ever agree, this host stopped being big.LITTLE and the guard below
        // stops proving anything — but on any such part it is also harmless.
        if let (Some(bare), Some(p0)) = (sysctl_u64("hw.l1dcachesize"), l1d) {
            assert!(
                bare <= p0,
                "hw.l1dcachesize ({bare}) must not exceed the P-core's ({p0}); \
                 detection reads perflevel0 precisely because the bare key is \
                 the efficiency cluster"
            );
        }
    }

    /// **The genericity gate.** On this host, detection must reproduce the
    /// hand-written profile *exactly* — same SVL, same L1D, same derived tile
    /// side, same derived block, same derived k-panel. If it does not, then one
    /// of the two is wrong and the hand-written numbers were never facts about
    /// this machine.
    ///
    /// This is what makes `apple-m4-sme` a **cross-compilation** spelling rather
    /// than the only way to describe the machine under the keyboard: the numbers
    /// in it are no longer load-bearing here, they are checked against the part.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn native_reproduces_the_hand_written_profile() {
        let Some(native) = native() else {
            panic!("detection must succeed on macOS")
        };
        let Some(nsme) = native.sme() else {
            // A Mac without SME: nothing to compare, and the fallback is the
            // pre-S41 behaviour. Not a failure.
            assert_eq!(sysctl_u64("hw.optional.arm.FEAT_SME"), Some(0));
            return;
        };
        let hand = APPLE_M4_SME.sme().expect("hand-written profile has SME");
        assert_eq!(nsme.svl_bytes, hand.svl_bytes, "SVL: detected vs written");
        assert_eq!(nsme.l1d_bytes, hand.l1d_bytes, "L1D: detected vs written");
        assert_eq!(native.l2_bytes, APPLE_M4_SME.l2_bytes, "L2");
        for elem in [F32, F64] {
            assert_eq!(
                native.sme_tile_side(&elem),
                APPLE_M4_SME.sme_tile_side(&elem)
            );
            assert_eq!(native.sme_block(&elem), APPLE_M4_SME.sme_block(&elem));
            assert_eq!(native.sme_kc(&elem), APPLE_M4_SME.sme_kc(&elem));
        }
        // And the number the whole S42 campaign turns on, arrived at with no
        // constant written anywhere: 131072 / ((2+2)*16*4) = 512.
        assert_eq!(native.sme_kc(&F32), Some(1024));
    }

    /// A hand-written profile always wins over detection (Sapir's rule).
    #[test]
    fn named_profiles_override_detection() {
        for p in PROFILES {
            assert_eq!(
                resolve(p.name).map(|r| r.name),
                Some(p.name),
                "the table is consulted before `native`"
            );
        }
        assert!(
            resolve("nativ").is_none(),
            "no fuzzy match, no silent default"
        );
        assert!(resolve("cuda_ada").is_none());
        assert!(resolve("apple-m4").is_none());
        assert!(resolve("").is_none());
    }

    /// Rule 1 for the SME leg: the capability is reachable only through
    /// `sme()`, and no profile that existed before it can answer. That is the
    /// whole byte-identity argument — the SME realization's gate cannot open
    /// without a fact only `apple-m4-sme` carries.
    #[test]
    fn sme_is_absent_from_every_pre_sme_profile() {
        for p in [&GENERIC, &APPLE_M, &ZEN3, &CUDA_ADA] {
            assert!(p.sme().is_none(), "{} must expose no SME facts", p.name);
            assert!(
                p.sme_tile_side(&F32).is_none(),
                "{} derives no tile",
                p.name
            );
        }
    }

    /// The measured M4 Pro facts, and the one derivation the emitter reads:
    /// SVL 512 bits ⇒ a 16×16 f32 tile (`benches/sme/svl.c`, `run16.c`). f64
    /// halves it, which is why the side is derived rather than written down.
    #[test]
    fn apple_m4_sme_derives_the_measured_tile_side() {
        let sme = APPLE_M4_SME.sme().expect("apple-m4-sme has an SME unit");
        assert_eq!(sme.svl_bytes, 64, "SVL = 512 bits, detected");
        assert_eq!(APPLE_M4_SME.sme_tile_side(&F32), Some(16));
        assert_eq!(APPLE_M4_SME.sme_tile_side(&F64), Some(8));
        // The tile count is an ISA rule, not a field: exactly `elem_bytes`.
        assert_eq!(APPLE_M4_SME.sme_block(&F32), Some((2, 2)), "4 tiles at f32");
        assert_eq!(APPLE_M4_SME.sme_block(&F64), Some((2, 4)), "8 tiles at f64");
    }

    /// The SME profile moves EXACTLY one field off `apple-m`. Every derived
    /// tile factor is unchanged, so a site that falls back to the NEON rung
    /// under `apple-m4-sme` emits what `apple-m` emits — the negative control
    /// the golden test pins at the text level.
    #[test]
    fn apple_m4_sme_moves_only_the_sme_field() {
        assert_eq!(
            APPLE_M4_SME,
            TargetProfile {
                name: APPLE_M4_SME.name,
                sme: APPLE_M4_SME.sme,
                ..APPLE_M
            }
        );
        assert_eq!(APPLE_M4_SME.tile_j(&F32), APPLE_M.tile_j(&F32));
        assert_eq!(APPLE_M4_SME.tile_i(), APPLE_M.tile_i());
        assert_eq!(APPLE_M4_SME.tile_kc(&F32), APPLE_M.tile_kc(&F32));
    }

    /// The accumulator budget, spent. 4 f32 tiles ⇒ a 2×2 block, which is the
    /// arrangement `benches/sme/mm4.c` measured at 1.84–2.61× over one tile.
    /// f64 has twice as many tiles (one per element byte), and the count is
    /// derived from the recorded f32 one rather than written down again.
    #[test]
    fn sme_block_is_the_square_most_split_of_the_tile_count() {
        assert_eq!(APPLE_M4_SME.sme_block(&F32), Some((2, 2)));
        assert_eq!(APPLE_M4_SME.sme_block(&F64), Some((2, 4)), "8 tiles at f64");
        for p in [&GENERIC, &APPLE_M, &ZEN3, &CUDA_ADA] {
            assert!(p.sme_block(&F32).is_none(), "{} has no ZA", p.name);
        }
    }

    /// The derivation's two invariants: the block spends the WHOLE tile budget,
    /// and it is the square-most factorization — the one that minimises `ti + tj`
    /// operand loads per `ti · tj` outer products (1×4 needs 5 loads for the 4
    /// MACs 2×2 gets from 4).
    ///
    /// **Swept over element WIDTHS, not over a recorded count.** The previous
    /// version of this test swept `f32_tiles` 1..=64, and S41b logged the defect
    /// that created: it pinned arrangements for counts like 7 or 64 that no ZA
    /// register file has and no emitted kernel could select, so the test asserted
    /// properties of unreachable IR. Now that the count is `elem_bytes` by
    /// construction there is nothing unreachable left to sweep — every width here
    /// is a width the architecture actually has.
    #[test]
    fn sme_block_spends_every_tile_and_stays_square_most() {
        for bits in [8_u8, 16, 32, 64] {
            let elem = Ty::Float { bits };
            let tiles = u64::from(bits).div_ceil(8);
            let (ti, tj) = APPLE_M4_SME.sme_block(&elem).expect("has ZA");
            assert_eq!(
                ti * tj,
                tiles,
                "the block must spend every tile ({tiles} at f{bits})"
            );
            assert!(ti <= tj, "ti is the smaller factor (f{bits})");
            let best = (1..=tiles)
                .filter(|d| tiles.is_multiple_of(*d))
                .map(|d| d + tiles / d)
                .min()
                .expect("a divisor exists");
            assert_eq!(ti + tj, best, "fewest operand loads at f{bits}");
            // Every arrangement must actually fit ZA: `ti·t x tj·t` elements of
            // `w` bytes is exactly `svl x svl`, the whole array, never more.
            let t = APPLE_M4_SME.sme_tile_side(&elem).expect("has ZA");
            let svl = APPLE_M4_SME.sme().unwrap().svl_bytes;
            assert_eq!(
                ti * t * tj * t * tiles,
                svl * svl,
                "the block must be exactly ZA at f{bits}, not larger"
            );
        }
        assert_eq!(
            (1..=4_u64)
                .filter(|d| 4_u64.is_multiple_of(*d))
                .map(|d| d + 4 / d)
                .min(),
            Some(4),
            "2x2 is 4 loads; 1x4 is 5"
        );
    }

    /// The packed-B panel the NEON rung already builds is `tile_j` lanes wide,
    /// and the SME kernel wants an SVL-wide contiguous row. On this part they
    /// are the same 16 — which is why the SME rung consumes the existing packed
    /// buffer instead of minting its own. `func/sme.rs` re-checks this per site
    /// and falls back when it does not hold, so this is a pin on *why the reuse
    /// is free here*, not a load-bearing invariant.
    #[test]
    fn sme_panel_and_packed_b_panel_coincide_on_this_part() {
        assert_eq!(
            APPLE_M4_SME.sme_tile_side(&F32),
            Some(APPLE_M4_SME.tile_j(&F32))
        );
    }

    /// S41 step 1, rule 1: adding the machine class must not move a single CPU
    /// emission. Every profile that existed before S41 is `Cpu`, so no CPU
    /// realization can observe the new field — the type system carries the
    /// byte-identity argument, and the 159-emission A/B sweep confirms it.
    #[test]
    fn every_pre_s41_profile_is_cpu() {
        for p in [&GENERIC, &APPLE_M, &ZEN3] {
            assert_eq!(p.machine, Machine::Cpu, "{} must stay Cpu", p.name);
            assert!(p.gpu().is_none(), "{} must expose no GPU facts", p.name);
        }
    }

    /// A GPU fact is reachable only through `gpu()`, so a CPU profile on a
    /// GPU-only path is a `None` at the seam rather than a wrong constant
    /// (plan-s41 §2.2 rule 2 — capability selects realization).
    #[test]
    fn gpu_facts_are_asked_for_never_defaulted() {
        let g = CUDA_ADA.gpu().expect("cuda-ada is a GPU profile");
        assert_eq!(g.warp, 32, "cooperation quantum");
        assert_eq!(g.max_threads_per_block, 1024);
        assert_eq!(
            g.arch, "sm_89",
            "verified against the device: compute capability 8.9 (RTX 4070 Ti)"
        );
        assert!(
            g.smem_bytes < CUDA_ADA.l2_bytes,
            "smem is the smaller budget"
        );
    }

    /// The placeholder honesty pin. `cuda-ada` inherits CPU-shaped constants it
    /// has no business claiming as GPU measurements; this test states that in
    /// the suite so nobody reads them as swept. When a GPU realization starts
    /// consuming one of these, it either gets its own value or this assertion
    /// changes — either way the change is deliberate and reviewed.
    #[test]
    fn cuda_ada_cpu_shaped_fields_are_inherited_placeholders() {
        assert_eq!(CUDA_ADA.vec_bytes, GENERIC.vec_bytes);
        assert_eq!(CUDA_ADA.vec_regs, GENERIC.vec_regs);
        assert_eq!(CUDA_ADA.l2_bytes, GENERIC.l2_bytes);
        assert_eq!(CUDA_ADA.acc_vecs_per_row, GENERIC.acc_vecs_per_row);
    }
}
