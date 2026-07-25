//! `TargetProfile` — the emitter's machine facts as data (plan-s31-target-profiles).
//!
//! ADR-0032 D4: tile factors, grain sizes and arena thresholds are backend
//! config, not language. Every field here is **value-invariant** — changing one
//! changes how fast the answer arrives, never the answer — which is exactly what
//! keeps the differential suite a valid gate under every profile.
//!
//! The split this file exists to enforce (the backend-genericity contract):
//! **geometry comes from the record, constants come from the profile.** flow-ir
//! never learns a machine fact; the emitter never re-derives graph analysis.
//! What used to be six literals swept on one M4 Pro is now one table plus
//! arithmetic, and the arithmetic reproduces those literals for the default
//! profile — that is the correctness gate for this change.

use flow_ir::Ty;

/// One target's machine facts, plus the two policy ratios that are honestly
/// search space rather than facts (see the field docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetProfile {
    pub name: &'static str,
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
    /// in the `flow_rt_alloc` arena instead of the stack.
    pub heap_min_bytes: u64,
}

/// The element width a tile site accumulates at. Tile sites are numeric-gated
/// by recognition (`flow_ir::algo` requires `is_numeric` for the element, the
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
}

/// Today's six literals, exactly: 128-bit vectors, 32 registers, a 512 KB
/// per-core L2. Emission under this profile is byte-identical to the
/// pre-profile emitter for every shape — the plan's rule 1, and the reason it
/// is the default.
pub const GENERIC: TargetProfile = TargetProfile {
    name: "generic",
    vec_bytes: 16,
    vec_regs: 32,
    acc_vecs_per_row: 4,
    nc_tiles: 32,
    l2_bytes: 512 * 1024,
    heap_min_bytes: 256 * 1024,
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

const PROFILES: [&TargetProfile; 3] = [&GENERIC, &APPLE_M, &ZEN3];

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
    /// `apple-m`. Pinned because `func.rs`'s heap note used to claim a flat
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
        assert!(resolve("apple_m").is_none());
        assert!(resolve("").is_none());
    }
}
