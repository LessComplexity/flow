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
/// unit. Both are **measured on the part** (`benches/sme/svl.c` on an M4 Pro),
/// never assumed: SVL is implementation-defined, and every literal the SME
/// realization emits — the panel side, the ZA row count, the packed A stride —
/// is derived from `svl_bytes`, so a part with a different SVL emits a
/// different kernel from the same record.
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
    /// Architectural f32 ZA tiles (`ZA0.S`…). 4 at f32, 8 at f64. Recorded
    /// because it is the accumulator-residency budget an unrolled realization
    /// would spend; the first rung accumulates into one of them.
    pub f32_tiles: u64,
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
    sme: Some(Sme {
        svl_bytes: 64,
        f32_tiles: 4,
    }),
    ..APPLE_M
};

const PROFILES: [&TargetProfile; 5] = [&GENERIC, &APPLE_M, &ZEN3, &CUDA_ADA, &APPLE_M4_SME];

/// Resolve a profile by name. `None` for an unknown name — never a silent
/// fallback to `generic`, because a typo that quietly emits the default
/// profile's numbers is the exact failure this table exists to remove.
pub fn resolve(name: &str) -> Option<&'static TargetProfile> {
    PROFILES.into_iter().find(|p| p.name == name)
}

/// The known profile names, for the error message on an unknown one.
pub fn names() -> String {
    PROFILES
        .iter()
        .map(|p| p.name)
        .collect::<Vec<_>>()
        .join(", ")
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
        assert_eq!(sme.svl_bytes, 64, "SVL = 512 bits, measured");
        assert_eq!(sme.f32_tiles, 4);
        assert_eq!(APPLE_M4_SME.sme_tile_side(&F32), Some(16));
        assert_eq!(APPLE_M4_SME.sme_tile_side(&F64), Some(8));
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
