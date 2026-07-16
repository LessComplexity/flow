//! Pass 1 rejection fixtures (DESIGN §7.2 / §7.4) — hand-built IrBuilder graphs.
//!
//! Multi-writer Return graphs are unconstructible through lower (its output is
//! always single-writer; L1405/L1409 upstream), so these are hand-built with
//! `IrBuilder` — exactly the shape validate's §9 scope note disclaims ("a
//! validate-clean graph may still have two unconditional full-value Return
//! writers"). The tree is an empty `Program` (pass 2 has nothing to walk).

use flow_check::check;
use flow_ir::{CategoryIr, FuncKind, IrBuilder, SourceLoc, Ty, Value, validate};
use flow_syntax::{Program, SourceLoc as SynLoc};

/// An empty program stub — pass 2 walks `items`, so no items = no effect diags.
fn empty_program() -> Program {
    Program {
        items: Vec::new(),
        span: SynLoc { start: 0, end: 0 },
    }
}

fn loc(n: u32) -> SourceLoc {
    SourceLoc::new(n, n + 1)
}

/// Build a single-`main` graph writing `n_full_writers` full-value i32
/// constants to the i32 Return, each `Output` edge at a distinct loc `loc(i+1)`.
fn multi_writer_ir(n_full_writers: u32) -> CategoryIr {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::Unit, Ty::i32(), loc(0))
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        for i in 0..n_full_writers {
            let c = fb.constant(Value::I32(i as i32), loc(i + 1)).unwrap();
            fb.output(c, None, loc(i + 1)).unwrap();
        }
        fb.finish().unwrap();
    }
    b.seal(f).unwrap()
}

#[test]
fn two_full_value_writers_one_t0101_at_second() {
    let ir = multi_writer_ir(2);
    assert!(validate(&ir).is_empty(), "fixture must be validate-clean");
    let diags = check("", &empty_program(), &ir);
    assert_eq!(diags.len(), 1, "expected exactly one T0101, got {diags:?}");
    assert_eq!(diags[0].code.0, "T0101");
    // Span is the SECOND writer's morphism loc — loc(2) = (2, 3).
    assert_eq!((diags[0].span.start, diags[0].span.end), (2, 3));
}

#[test]
fn three_full_value_writers_two_t0101() {
    let ir = multi_writer_ir(3);
    let diags = check("", &empty_program(), &ir);
    assert_eq!(diags.len(), 2, "expected two T0101, got {diags:?}");
    assert!(diags.iter().all(|d| d.code.0 == "T0101"));
    // At the second and third writers: loc(2)=(2,3), loc(3)=(3,4).
    assert_eq!((diags[0].span.start, diags[0].span.end), (2, 3));
    assert_eq!((diags[1].span.start, diags[1].span.end), (3, 4));
}

#[test]
fn single_writer_clean() {
    let ir = multi_writer_ir(1);
    let diags = check("", &empty_program(), &ir);
    assert!(
        diags.is_empty(),
        "single writer must be clean, got {diags:?}"
    );
}

#[test]
fn slot_form_pair_writers_clean() {
    // A product Return (i32, i32) written one full-value producer per slot: each
    // slot has a single Pair writer, so there are ZERO full-value writers → the
    // exclusivity rule (|W| > 1) does not fire. (validate's I-RET arms guarantee
    // per-slot single-writer, so no mixing.)
    let mut b = IrBuilder::new();
    let out_ty = Ty::Tuple(vec![Ty::i32(), Ty::i32()]);
    let f = b
        .declare(FuncKind::Named, "main", Ty::Unit, out_ty, loc(0))
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let c0 = fb.constant(Value::I32(10), loc(1)).unwrap();
        fb.output(c0, Some(0), loc(1)).unwrap();
        let c1 = fb.constant(Value::I32(20), loc(2)).unwrap();
        fb.output(c1, Some(1), loc(2)).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty());
    let diags = check("", &empty_program(), &ir);
    assert!(
        diags.is_empty(),
        "slot-form writers must be clean, got {diags:?}"
    );
}

/// §7.4: a real loop-bearing function (`sum_to_n`) funnels its exit through one
/// `Output` — |W| = 1 by construction, so exclusivity stays clean even with a
/// loop. Uses the real parse → lower pipeline (dev-dep flow-lower).
#[test]
fn clean_under_loops_sum_to_n() {
    let src = std::fs::read_to_string(format!(
        "{}/../../examples/sum_to_n.flow",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let po = flow_syntax::parse(&src);
    assert!(po.diagnostics.is_empty());
    let ir = flow_lower::lower(&src, &po.program).unwrap();
    let diags = check(&src, &po.program, &ir);
    assert!(diags.is_empty(), "sum_to_n must be clean, got {diags:?}");
}

/// §7.4 token-bearing variant: a countdown that PRINTS in-loop (so `countdown`'s
/// signature carries the token) yet still funnels its loop exit through a single
/// `Output` — |W| = 1. This is the case §7.4 names explicitly: a loop-exit
/// Output writer combined with an effectful signature must still check clean on
/// the exclusivity side (the in-loop print is in sequential guard-arm context,
/// E2-legal). Real parse → lower → check.
#[test]
fn clean_under_loops_token_bearing_countdown() {
    let src = r#"
fn countdown(n: i32) -> i32 {
    mut i: i32 <- n;
    loop {
        (i > 0) -> {
            -true-> {
                i -> println;
                i - 1 -> i;
                -> loop;
            }
            -false-> i -> ret;
        }
    }
}

fn main() {
    3 -> countdown -> done;
    done -> println;
}
"#;
    let po = flow_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
    let ir = flow_lower::lower(src, &po.program).unwrap_or_else(|ds| panic!("lower: {ds:?}"));
    let diags = check(src, &po.program, &ir);
    assert!(
        diags.is_empty(),
        "token-bearing countdown must be clean, got {diags:?}"
    );
}
