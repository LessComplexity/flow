//! The S44 move-panel rung's gate, at the text level.
//!
//! The rung is a **permutation of a bulk map's loop counter**
//! (`src/func/bulk.rs::move_panel_index`), and the only branchy part of it is
//! the eligibility gate. So what needs pinning is the gate, not the arithmetic:
//! it must fire exactly when the panel tiles the recorded geometry both ways,
//! and be a **byte-identical no-op** everywhere else — including, crucially, on
//! a recognized tile site, whose emission belongs to a different rung entirely.
//!
//! There was no test covering any of this: without the last case here, a change
//! that let the flag reach the matmul/FIR/conv2d tile rungs would fail nothing.
//!
//! The permutation itself is checked as a bijection in
//! `move_panel_is_a_bijection_of_the_iteration_space` — the property the whole
//! value-identity argument rests on. It is checked against the formula the
//! emitter writes, at the sizes the emitter would use.

use mapal_backend_llvm::{EmitOpts, emit_with_opts};
use mapal_ir::CategoryIr;

/// The ladder's transpose shape at 16x16: a `map` with a captured array read,
/// **no fold in the body**, so it is not a tile site and takes the generic
/// `emit_map` path the flag gates.
const TRANSPOSE_16: &str = r#"
fn main() {
    256 -> iota -> ia;
    ia -> map { t -> (t * 7 + 13) % 101 - 50 -> widen_f32 } -> a;
    256 -> iota -> ib;
    ib -> map { t -> a[(t % 16) * 16 + t / 16] } -> b;
    b[0] -> println;
    b[255] -> println;
}
"#;

/// A matmul-shaped site at 16: recognized as a tile site, so `emit_map` hands
/// it to the tile rung and never reaches the move-panel gate. `256 % 16 == 0`,
/// so the geometry WOULD divide — which is what makes this a real negative
/// rather than a vacuous one.
const MATMUL_16: &str = r#"
fn main() {
    256 -> iota -> tq;
    tq -> map { t -> (t * 7 + 13) % 101 - 50 -> widen_f32 } -> q;
    tq -> map { t -> (t * 5 + 29) % 101 - 50 -> widen_f32 } -> kt;
    16 -> iota -> kr;
    tq -> map { t ->
        t / 16 -> i;
        t % 16 -> j;
        (0.0, kr) -> fold { acc, k -> acc + q[i * 16 + k] * kt[k * 16 + j] }
    } -> s;
    s[0] -> println;
    s[255] -> println;
}
"#;

fn lower_src(src: &str) -> CategoryIr {
    let po = mapal_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
    mapal_lower::lower(src, &po.program).unwrap_or_else(|d| panic!("lower: {d:?}"))
}

fn emit_with(src: &str, move_panel: Option<(u64, u64)>) -> String {
    let ir = mapal_rewrite::rewrite(lower_src(src)).ir;
    emit_with_opts(
        &ir,
        &EmitOpts {
            move_panel,
            ..EmitOpts::default()
        },
    )
    .expect("emits")
}

/// The gate fires when the panel tiles the geometry. Positive control: without
/// this, every negative below could pass because the rung never works at all.
#[test]
fn the_rung_fires_when_the_panel_tiles_the_geometry() {
    let off = emit_with(TRANSPOSE_16, None);
    let on = emit_with(TRANSPOSE_16, Some((16, 4)));
    assert_ne!(off, on, "16x16 with a 4-panel must emit the permutation");
    // 16 rows / 4 = 4 column blocks: the decomposition's one non-trivial
    // constant, and the only place `col_blocks` is visible in the text.
    assert!(
        on.contains("urem i64") && on.contains("udiv i64"),
        "the counter decomposition must be in the emitted text"
    );
}

/// Default OFF is what `EmitOpts::default()` emits, character for character.
/// This is the byte-identity argument at the type level: the gate reads the
/// `Option` before any text diverges, so `None` cannot reach the permutation.
#[test]
fn default_off_is_character_identical() {
    let ir = mapal_rewrite::rewrite(lower_src(TRANSPOSE_16)).ir;
    let default = emit_with_opts(&ir, &EmitOpts::default()).expect("emits");
    assert_eq!(
        default,
        emit_with(TRANSPOSE_16, None),
        "the default profile must be exactly the `move_panel: None` emission"
    );
}

/// The rung DECLINES rather than emitting a partial tiling. Every one of these
/// would need a remainder arm, and the honest answer is that this rung has
/// nothing to say about that shape — so it must emit today's text exactly.
#[test]
fn a_panel_that_does_not_tile_the_geometry_declines_silently() {
    let off = emit_with(TRANSPOSE_16, None);
    for (w, b, why) in [
        (16, 3, "3 does not divide 16 rows"),
        (16, 5, "5 divides neither axis"),
        (7, 7, "7 does not divide n = 256"),
        (256, 16, "one row: rows < 2, nothing to block"),
        (16, 0, "a zero panel"),
        (0, 4, "a zero width"),
    ] {
        assert_eq!(
            off,
            emit_with(TRANSPOSE_16, Some((w, b))),
            "--move-panel={w}:{b} must decline byte-identically ({why})"
        );
    }
}

/// **The one that had no coverage at all: a recognized tile site is SHIELDED
/// from the flag**, because the tile rung consumes it before `emit_map` reaches
/// the generic path the flag gates.
///
/// The obvious spelling of this — "OFF and ON emit the same text for a matmul
/// source" — is FALSE, and finding that out is why the test is written this way.
/// A matmul source also contains *generator* maps (`tq -> map {...} -> q`) which
/// are ordinary eligible maps, so the module does move; it just does not move
/// *at the site*. The byte-identity sweep shows the same thing from the other
/// side: `fir|rew` and `conv2d|rew` stay put while their `raw` faces move.
///
/// So the assertion is differential: turn tiling OFF and the site falls into the
/// generic path, which must expose **strictly more** maps to the flag. Each
/// permuted map contributes exactly two `urem i64`, so counting them measures
/// how many maps the flag reached.
#[test]
fn the_flag_cannot_reach_a_recognized_tile_site() {
    let urems = |ll: &str| ll.matches("urem i64").count();
    let emit_tiling = |tiling: bool, move_panel| {
        let ir = mapal_rewrite::rewrite(lower_src(MATMUL_16)).ir;
        emit_with_opts(
            &ir,
            &EmitOpts {
                tiling,
                move_panel,
                ..EmitOpts::default()
            },
        )
        .expect("emits")
    };
    let panel = Some((16, 4));
    let tiled = urems(&emit_tiling(true, panel)) - urems(&emit_tiling(true, None));
    let untiled = urems(&emit_tiling(false, panel)) - urems(&emit_tiling(false, None));
    assert!(
        untiled > tiled,
        "with tiling off the site joins the generic path and the flag must reach \
         one more map: tiled added {tiled} `urem i64`, untiled added {untiled}"
    );
}

/// The property every value-identity claim in S44 rests on: the emitted
/// decomposition is a **bijection** of `[0, n)`, so the parallel slices'
/// partition of the counter maps to a partition of the outputs — every element
/// written exactly once, by exactly one worker.
///
/// This mirrors `move_panel_index`'s arithmetic; if the emitter's formula
/// changes and this is not updated, the two disagree and the next reviewer has
/// the statement of intent in front of them.
#[test]
fn move_panel_is_a_bijection_of_the_iteration_space() {
    for (w, rows, b) in [(16_u64, 16_u64, 4_u64), (1024, 1024, 16), (64, 32, 8)] {
        let n = w * rows;
        let col_blocks = w / b;
        let mut seen = vec![false; n as usize];
        for p in 0..n {
            let (dc, q) = (p % b, p / b);
            let (dr, q2) = (q % b, q / b);
            let (cb, rb) = (q2 % col_blocks, q2 / col_blocks);
            let t = (rb * b + dr) * w + cb * b + dc;
            assert!(t < n, "w={w} b={b}: index {t} escapes [0, {n})");
            assert!(!seen[t as usize], "w={w} b={b}: index {t} visited twice");
            seen[t as usize] = true;
        }
        assert!(
            seen.iter().all(|&s| s),
            "w={w} b={b}: not every index visited"
        );
    }
}
