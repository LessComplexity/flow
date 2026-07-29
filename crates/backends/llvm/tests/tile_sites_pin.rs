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

// ---------------------------------------------------------------------------
// Gate B — record identity (plan-s41 §2.2 rule 1; ADR-0033 D4)
// ---------------------------------------------------------------------------
//
// The counts above pin *that* a site is recognized. This pins *what the record
// says* — every field a backend realization actually consumes.
//
// Why it is a separate obligation. ADR-0033's second risk is "silent CPU
// capture of mapal-ir": a cache-hierarchy or register-file assumption migrating
// into a query that calls itself generic, with no gate firing. `tile_plan` takes
// `(ir, f)` and no target, so today the record cannot depend on a machine — this
// file is what keeps that true. If anyone gives recognition a target, a profile,
// or a machine constant, these values move and the diff says exactly which field
// became machine-dependent.
//
// It is also ADR-0033 D4(a) answered in advance: "which TileRead fields did the
// smem emitter actually consume?" The answer has to be a fixed thing to point
// at, and per plan-s41 §1.5 composition rule 1 both the CPU and the GPU
// realization must see *identical* values. Two realizations reading one record
// is the whole claim; this is where the record is nailed down.
//
// The address law, for reading the table below:
//
//     addr = base + ci*i + ck*k + clane*lane        (in elements)
//
// `ci == 0` means the read does not move with the row — on CPU that licenses row
// blocking, and on GPU it is precisely the condition for staging into shared
// memory once per block. `clane` is the coalescing stride: 1 is a contiguous
// (coalesced) read, >1 is strided and wants a transposed stage. One record,
// two readings — which is the thing being proven.

/// One line per recognized site: the shape, then the two reads' address laws.
fn record_rows() -> String {
    let mut out = String::new();
    for rel in SOURCES {
        let ir = mapal_rewrite::rewrite(lower_src(&read_bench(rel))).ir;
        for (f, _) in ir.funcs() {
            let plan = ir.tile_plan(f);
            let mut sites: Vec<_> = plan.sites.iter().collect();
            // MorphismId order is an emitter-internal detail; sort by the record
            // itself so the pin cannot move when unrelated allocation changes.
            sites.sort_by_key(|(_, s)| (s.rows, s.c, s.k, s.a.slot, s.b.slot));
            for (_, s) in sites {
                let ks = |r: &mapal_ir::TileRead| match &r.ksplit {
                    None => "-".to_owned(),
                    Some(k) => format!("div{} cq{} cr{}", k.div, k.cq, k.cr),
                };
                let _ = writeln!(
                    out,
                    "{:<41} rows={} c={} k={} mul_a_first={} add_acc_first={}\n\
                     {:<41}   a: slot={} base={} ci={} ck={} clane={} ksplit={}\n\
                     {:<41}   b: slot={} base={} ci={} ck={} clane={} ksplit={}",
                    rel,
                    s.rows,
                    s.c,
                    s.k,
                    s.mul_a_first,
                    s.add_acc_first,
                    "",
                    s.a.slot,
                    s.a.base,
                    s.a.ci,
                    s.a.ck,
                    s.a.clane,
                    ks(&s.a),
                    "",
                    s.b.slot,
                    s.b.base,
                    s.b.ci,
                    s.b.ck,
                    s.b.clane,
                    ks(&s.b),
                );
            }
        }
    }
    out
}

/// The record's field values, pinned. Snapshot rather than hand-written
/// literals: the table is long, and a reviewer reading the *diff* is what
/// catches a field going machine-dependent.
#[test]
fn geometry_record_content_is_pinned_and_target_independent() {
    insta::assert_snapshot!("tile_record_content", record_rows());
}

/// The record is a pure function of the graph. Taking the plan repeatedly, and
/// on independently lowered copies of the same source, must give the same
/// answer — no hidden state, no ordering dependence, nothing ambient.
///
/// This is the runtime half of "no target in the signature": a query that began
/// consulting a machine, a global, or an environment variable would diverge
/// here even if its type still looked pure.
#[test]
fn the_record_is_a_pure_function_of_the_graph() {
    for rel in SOURCES {
        let src = read_bench(rel);
        let a = mapal_rewrite::rewrite(lower_src(&src)).ir;
        let b = mapal_rewrite::rewrite(lower_src(&src)).ir;
        for ((fa, _), (fb, _)) in a.funcs().zip(b.funcs()) {
            let (pa, pb) = (a.tile_plan(fa), b.tile_plan(fb));
            assert_eq!(
                pa.sites.len(),
                pb.sites.len(),
                "{rel}: site count differs between two lowerings of one source"
            );
            // A formatted key rather than a tuple: Rust implements Ord/Debug
            // only up to 12-element tuples, and the record has 13 fields worth
            // comparing. The string also makes an assertion failure readable.
            let key = |p: &mapal_ir::TilePlan| {
                let mut v: Vec<String> = p
                    .sites
                    .iter()
                    .map(|(_, s)| {
                        format!(
                            "rows={} c={} k={} a=({},{},{},{},{}) b=({},{},{},{},{})",
                            s.rows,
                            s.c,
                            s.k,
                            s.a.slot,
                            s.a.base,
                            s.a.ci,
                            s.a.ck,
                            s.a.clane,
                            s.b.slot,
                            s.b.base,
                            s.b.ci,
                            s.b.ck,
                            s.b.clane,
                        )
                    })
                    .collect();
                v.sort();
                v
            };
            assert_eq!(
                key(&pa),
                key(&pb),
                "{rel}: the geometry record is not a pure function of the graph"
            );
            // and stable under being asked twice
            assert_eq!(
                key(&pa),
                key(&a.tile_plan(fa)),
                "{rel}: plan is not idempotent"
            );
        }
    }
}
