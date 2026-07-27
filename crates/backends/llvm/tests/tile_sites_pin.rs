//! Tile-site recognition pin (plan-s37-stage-structure §8, §9 step 1).
//!
//! `tile_site` decides whether a `map`-of-`fold` becomes the register-blocked
//! micro-kernel or falls back to the scalar `emit_map`/`emit_fold` pair. The
//! fallback is **silent** — `lib.rs` does `tile_plan(f).sites.get(m)` and simply
//! emits the untiled form when the site is absent, with no diagnostic and
//! byte-identical output. The measured cost of that fall
//! (`docs/performance/matmul/s25.md:46-48`) is matmul f32 N=1024 238.3 ms tiled
//! vs 947.6 ms untiled — **4.0x**, f64 3.5x, attn 4.6x.
//!
//! So recognition needs a tripwire, and it has to exist *before* anything
//! touches the recognizer. This is that tripwire: one exact site count per
//! shipped benchmark source. A change that loses a site fails here instead of
//! showing up as a 4x regression several sessions later.
//!
//! Both published harnesses emit with `--rewrite` (`benches/shapes/shapes_ab.sh:52`,
//! `benches/matmul/regen.sh:40`), so the rewritten count is the one that governs
//! shipped numbers; the raw count is pinned beside it because a divergence
//! between them is itself information.
//!
//! **Updating this file is a deliberate act.** A count going *up* is a win and
//! may be re-pinned with the measurement that justifies it. A count going *down*
//! is a blocker, not a re-pin.

use std::fmt::Write as _;

use mapal_ir::CategoryIr;

fn lower_src(src: &str) -> CategoryIr {
    let po = mapal_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
    mapal_lower::lower(src, &po.program).unwrap_or_else(|d| panic!("lower: {d:?}"))
}

fn read_bench(rel: &str) -> String {
    let path = format!("{}/../../../{}", env!("CARGO_MANIFEST_DIR"), rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// Total recognized tile sites across every function in the module. Summed
/// rather than read off `main` alone because `matmul*_cap*` sources carry the
/// kernel in a named `fn` that the rewriter's `Inline` pass may or may not have
/// folded into the entry by the time the plan is taken.
fn site_count(ir: &CategoryIr) -> usize {
    ir.funcs().map(|(f, _)| ir.tile_plan(f).sites.len()).sum()
}

/// The sources the published numbers come from: every ladder shape at its
/// measured size, plus the matmul/attn class where the 4.0x cliff was measured.
const SOURCES: &[&str] = &[
    "benches/shapes/saxpy_1048576.mapal",
    "benches/shapes/reduce_1048576.mapal",
    "benches/shapes/transpose_1024.mapal",
    "benches/shapes/gather_1048576.mapal",
    "benches/shapes/fir_1048576.mapal",
    "benches/shapes/fir_65536.mapal",
    "benches/shapes/conv2d_1024.mapal",
    "benches/shapes/conv2d_512.mapal",
    "benches/shapes/attn_256.mapal",
    "benches/shapes/attn_256_rowmajor.mapal",
    "benches/matmul/matmul1024_cap_f32.mapal",
    "benches/matmul/matmul1024_cap.mapal",
    "benches/matmul/matmul512_cap_f32.mapal",
    "benches/matmul/matmul256_cap_f32.mapal",
    "benches/matmul/matmul128_cap_f32.mapal",
];

/// `source  raw=<n>  rewritten=<n>` — the rewritten column is the governing one.
///
/// **`raw` is 0 for every source**, and that is a real fact rather than an
/// accident of this table: no tile site is recognized before `rewrite` runs,
/// because the kernels arrive as `Call`s or loops that `Inline` and `LiftLoops`
/// have to fold first. The entire tiled path is downstream of the rewriter. It
/// is pinned so that a future change making raw recognition possible shows up
/// as a signal instead of passing unnoticed.
///
/// `attn_256` is 2 because it chains two matmuls (S25) — the one row here that
/// is not 0 or 1.
const PINNED: &str = "\
benches/shapes/saxpy_1048576.mapal        raw=0 rewritten=0
benches/shapes/reduce_1048576.mapal       raw=0 rewritten=0
benches/shapes/transpose_1024.mapal       raw=0 rewritten=0
benches/shapes/gather_1048576.mapal       raw=0 rewritten=0
benches/shapes/fir_1048576.mapal          raw=0 rewritten=1
benches/shapes/fir_65536.mapal            raw=0 rewritten=1
benches/shapes/conv2d_1024.mapal          raw=0 rewritten=1
benches/shapes/conv2d_512.mapal           raw=0 rewritten=1
benches/shapes/attn_256.mapal             raw=0 rewritten=2
benches/shapes/attn_256_rowmajor.mapal    raw=0 rewritten=1
benches/matmul/matmul1024_cap_f32.mapal   raw=0 rewritten=1
benches/matmul/matmul1024_cap.mapal       raw=0 rewritten=1
benches/matmul/matmul512_cap_f32.mapal    raw=0 rewritten=1
benches/matmul/matmul256_cap_f32.mapal    raw=0 rewritten=1
benches/matmul/matmul128_cap_f32.mapal    raw=0 rewritten=1
";

fn observed() -> String {
    let mut out = String::new();
    for rel in SOURCES {
        let src = read_bench(rel);
        let raw = lower_src(&src);
        let rewritten = mapal_rewrite::rewrite(lower_src(&src)).ir;
        let _ = writeln!(
            out,
            "{:<41} raw={} rewritten={}",
            rel,
            site_count(&raw),
            site_count(&rewritten)
        );
    }
    out
}

#[test]
fn tile_sites_recognized_per_bench_source() {
    assert_eq!(
        observed(),
        PINNED,
        "\ntile-site recognition changed. A count going DOWN is a BLOCKER: the \
         affected kernel now emits the scalar fallback silently (4.0x on matmul \
         1024, docs/performance/matmul/s25.md:46-48). A count going UP is a win \
         — re-pin with the measurement that justifies it.\n"
    );
}
