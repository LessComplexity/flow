//! Layer 2 (DESIGN §4, §9.2) — the `Zip`/`Enumerate` naturality laws as a **data
//! table only**. The four equalities recorded in ir DESIGN §17 are natural
//! transformations, so their squares are free layer-2 rewrites — but v1 ships no
//! pass (a cost direction is still open, ir §17 / rewrite §11). The catalogue
//! lives here so §9.2 has its home without an unproven pass.

/// One recorded layer-2 rewrite law (DESIGN §4). `lhs`/`rhs` are the categorical
/// equation as written in ir DESIGN §17; no renderer, no builder pattern — data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NaturalityLaw {
    /// A short identifier for the law.
    pub name: &'static str,
    /// The left-hand side of the equation (the pattern that would be matched).
    pub lhs: &'static str,
    /// The right-hand side (the rewrite result).
    pub rhs: &'static str,
    /// v1 status — every entry is `planned` (no pass this increment).
    pub status: LawStatus,
}

/// The implementation status of a recorded law.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LawStatus {
    /// Recorded for a later increment; no pass emits it in v1.
    Planned,
}

/// The four ir DESIGN §17 `Zip`/`Enumerate` naturality laws (ADR-0018), all
/// `Planned`. Order matches ir §17's enumeration.
pub const NATURALITY_LAWS: &[NaturalityLaw] = &[
    NaturalityLaw {
        name: "zip-naturality",
        lhs: "zip ∘ (map f × map g)",
        rhs: "map (f × g) ∘ zip",
        status: LawStatus::Planned,
    },
    NaturalityLaw {
        name: "enumerate-naturality",
        lhs: "enumerate ∘ map f",
        rhs: "map (id_i32 × f) ∘ enumerate",
        status: LawStatus::Planned,
    },
    NaturalityLaw {
        name: "enumerate-fst",
        lhs: "map π₁ ∘ enumerate",
        rhs: "id",
        status: LawStatus::Planned,
    },
    NaturalityLaw {
        name: "iota-deduction",
        lhs: "iota_n",
        rhs: "map π₀ ∘ enumerate",
        status: LawStatus::Planned,
    },
];
