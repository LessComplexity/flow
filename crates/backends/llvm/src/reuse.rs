//! `i_reuse` — how much of a read two adjacent output rows share
//! (plan-s31-deduced-blocking item 3).
//!
//! Register blocking pays exactly when stepping the output row index does *not*
//! move a read's addresses by a full row's worth. That question is already
//! answered by the recorded address coefficients:
//!
//! ```text
//! addr(i, lane, k) = base + ci·i + clane·lane + ck·k + cq·(k÷div) + cr·(k%div)
//! ```
//!
//! so it is arithmetic over `TileRead`, **not** new graph analysis — ADR-0032
//! category (b), emitter-local cashing with zero flow-ir change. The point of
//! writing it this way is that `ci == 0` (matmul's `b`) and `ci == cq` (conv's
//! `b`) stop being two different rungs: they are the same predicate at `q = 0`
//! and `q = 1`, which is what makes blocking generic per algorithm rather than
//! per shape.
//!
//! What this module does NOT decide is how large a block may be. That is the
//! register file — a machine fact, and [`crate::profile::TargetProfile::tile_i`]
//! owns it. Geometry here, constants there.

use flow_ir::{TileRead, TileSite};

/// How a read's addresses move when the output row index advances by one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reuse {
    /// `ci == 0`: the addresses do not move at all, so ONE fetched vector
    /// serves every row of a block. Matmul's `b` — the fact the S26 rung-2 gate
    /// already keys on (`site.rows > 1 && site.b.ci == 0`).
    ///
    /// Vacuous on a `rows == 1` site (there is no row axis to block over), so
    /// callers must gate on `site.rows > 1` as the matmul rung does.
    Invariant,
    /// The read slides by `q` whole tap-rows per output row: row `i+1`'s tap
    /// `(kq − q, kr)` addresses bit-identically what row `i`'s tap `(kq, kr)`
    /// did. Conv2d's `b`, where `ci == cq` gives `q == 1` — output row `i+1`
    /// re-reads all but one of the image rows row `i` read.
    Sliding { q: u64 },
    /// Adjacent rows share nothing: matmul's `a`, whose `ci` is the row stride
    /// of a genuinely different row of data.
    None,
}

/// The extent of the `k÷div` (tap-row) axis, when the read has one.
fn tap_rows(site: &TileSite, read: &TileRead) -> Option<u64> {
    let ks = read.ksplit.as_ref()?;
    (ks.div != 0).then(|| site.k / ks.div)
}

/// Classify one read's row-to-row sharing. Total — every read gets an answer,
/// and [`Reuse::None`] is the honest one for reads that share nothing.
pub(crate) fn i_reuse(site: &TileSite, read: &TileRead) -> Reuse {
    if read.ci == 0 {
        return Reuse::Invariant;
    }
    let Some(ks) = read.ksplit.as_ref() else {
        // No derived axis to slide along: `ci` moves to unrelated data.
        return Reuse::None;
    };
    let Some(rows) = tap_rows(site, read) else {
        return Reuse::None;
    };
    // Sliding requires the row step to be a whole number of tap-rows, and the
    // slide to be shorter than the window — otherwise consecutive output rows
    // address disjoint tap-rows and there is nothing to share.
    if ks.cq == 0 || read.ci % ks.cq != 0 {
        return Reuse::None;
    }
    let q = read.ci / ks.cq;
    if q == 0 || q >= rows {
        return Reuse::None;
    }
    Reuse::Sliding { q }
}

/// How many distinct tap-rows a `ti`-row block must fetch for this read — the
/// denominator of the reuse factor. Unblocked, `ti` rows would need
/// `ti × tap_rows` of them; the ratio is what blocking buys.
///
/// `Invariant → 1` · `Sliding{q} → (ti−1)·q + tap_rows` · `None → ti`.
pub(crate) fn distinct_runs(site: &TileSite, read: &TileRead, ti: u64) -> u64 {
    match i_reuse(site, read) {
        Reuse::Invariant => 1,
        Reuse::Sliding { q } => {
            let rows = tap_rows(site, read).expect("Sliding implies a tap axis");
            (ti - 1) * q + rows
        }
        Reuse::None => ti,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flow_ir::{TileKSplit, Ty, Value};

    fn read(ci: u64, ck: u64, clane: u64, ksplit: Option<TileKSplit>) -> TileRead {
        TileRead {
            slot: 0,
            base: 0,
            ci,
            ck,
            clane,
            ksplit,
        }
    }

    fn site(rows: u64, c: u64, k: u64, a: TileRead, b: TileRead) -> TileSite {
        TileSite {
            rows,
            c,
            k,
            a,
            b,
            seed: Value::F32(0.0),
            elem: Ty::f32(),
            mul_a_first: true,
            add_acc_first: true,
        }
    }

    /// The recorded matmul oracle (`flow-ir/tests/algos.rs`): `b.ci == 0` is
    /// row-invariance — the fact the S26 gate already cashes — and `a`'s row
    /// stride shares nothing.
    #[test]
    fn matmul_b_is_invariant_and_a_shares_nothing() {
        let s = site(512, 512, 512, read(512, 1, 0, None), read(0, 512, 1, None));
        assert_eq!(i_reuse(&s, &s.b), Reuse::Invariant);
        assert_eq!(i_reuse(&s, &s.a), Reuse::None);
        assert_eq!(distinct_runs(&s, &s.b, 4), 1, "one b tile serves TI rows");
        assert_eq!(distinct_runs(&s, &s.a, 4), 4, "TI distinct a rows");
    }

    /// The recorded conv2d oracle (`algos.rs:2090` — 16×16 out over an 18×18
    /// image, K=9): `b.ci == cq == 18` is the sliding case at `q = 1`, which is
    /// the SAME predicate as matmul's at `q = 0`. This is the whole point of
    /// item 3 — conv needs no rung of its own to be blocked.
    #[test]
    fn conv_b_slides_by_one_tap_row() {
        let s = site(
            16,
            16,
            9,
            read(0, 1, 0, None),
            read(
                18,
                0,
                1,
                Some(TileKSplit {
                    div: 3,
                    cq: 18,
                    cr: 1,
                }),
            ),
        );
        assert_eq!(i_reuse(&s, &s.b), Reuse::Sliding { q: 1 });
        // The broadcast weight vector does not depend on the row at all.
        assert_eq!(i_reuse(&s, &s.a), Reuse::Invariant);
    }

    /// The reuse factor at the register-feasible TI, and its ceiling. This is
    /// the arithmetic that corrected suggestion #11: **2.0× at TI=4**, not 3×.
    /// The 3× is the `TI → ∞` limit and needs TI ≥ 12 to get close.
    #[test]
    fn conv_reuse_is_two_at_ti_four_not_three() {
        let s = site(
            16,
            16,
            9,
            read(0, 1, 0, None),
            read(
                18,
                0,
                1,
                Some(TileKSplit {
                    div: 3,
                    cq: 18,
                    cr: 1,
                }),
            ),
        );
        let tap_rows = 3;
        // TI=1 is the status quo: three image rows per output row.
        assert_eq!(distinct_runs(&s, &s.b, 1), 3);
        // TI=4: six image rows serve four output rows.
        assert_eq!(distinct_runs(&s, &s.b, 4), 6);
        assert_eq!(4 * tap_rows / distinct_runs(&s, &s.b, 4), 2, "2.0x at TI=4");
        // The ceiling is `div` = 3, approached only as TI grows.
        assert_eq!(distinct_runs(&s, &s.b, 12), 14);
        assert!(
            12 * tap_rows * 10 / distinct_runs(&s, &s.b, 12) < 30,
            "still under 3x at TI=12"
        );
    }

    /// A k-split read whose row step is NOT a whole number of tap-rows shares
    /// nothing — consecutive output rows address disjoint windows, so blocking
    /// it would fetch the same volume in a worse order.
    #[test]
    fn unaligned_or_oversized_slide_shares_nothing() {
        let ks = Some(TileKSplit {
            div: 3,
            cq: 18,
            cr: 1,
        });
        // ci is not a multiple of cq.
        let s = site(16, 16, 9, read(0, 1, 0, None), read(19, 0, 1, ks));
        assert_eq!(i_reuse(&s, &s.b), Reuse::None);
        // ci steps a full window (q == tap_rows): disjoint, nothing shared.
        let s = site(16, 16, 9, read(0, 1, 0, None), read(54, 0, 1, ks));
        assert_eq!(i_reuse(&s, &s.b), Reuse::None);
        assert_eq!(distinct_runs(&s, &s.b, 4), 4);
    }

    /// A 1-D site (FIR) has no row axis — `rows == 1` collapses it, so every
    /// `ci` is structurally 0 and `Invariant` is vacuously true. Callers gate
    /// on `rows > 1`, exactly as the matmul rung does; recorded so the vacuous
    /// answer is never mistaken for a blocking opportunity.
    #[test]
    fn one_dimensional_sites_report_vacuous_invariance() {
        let s = site(1, 64, 4, read(0, 0, 0, None), read(0, 1, 1, None));
        assert_eq!(i_reuse(&s, &s.b), Reuse::Invariant);
        assert_eq!(s.rows, 1, "and the caller must therefore not block it");
    }
}
