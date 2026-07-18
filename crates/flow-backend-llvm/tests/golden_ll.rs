//! DESIGN §6 test plan item 2/4/5: golden `.ll` insta snapshots (byte-stable,
//! L2), determinism (emit twice → byte-equal), and the `Unsupported` pin on the
//! hand-built nested-loop graph (L3).
//!
//! Micro shapes cover the templates the 10 examples don't isolate cleanly:
//! signed `Div`/`Mod` guards (zero + `MIN/-1`), `Update` memcpy+store, and two
//! sequential loops in one fn (the S12 P0 shape). Select-Phi, index guards, and
//! loop CFG are additionally covered by the `abs` / `fir` / `sum_to_n` example
//! snapshots.

use flow_backend_llvm::{EmitError, emit};
use flow_ir::{CategoryIr, Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value};

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

const EXAMPLES: &[&str] = &[
    "abs",
    "calc",
    "fanout",
    "fir",
    "pipeline",
    "sepia",
    "seq_demo",
    "sum_to_n",
    "vector_add",
    "zip_demo",
];

fn lower_src(src: &str) -> CategoryIr {
    let po = flow_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
    flow_lower::lower(src, &po.program).unwrap_or_else(|d| panic!("lower: {d:?}"))
}

fn build_example(name: &str) -> CategoryIr {
    let path = format!(
        "{}/../../examples/{}.flow",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    let src = std::fs::read_to_string(&path).unwrap();
    lower_src(&src)
}

#[test]
fn golden_examples() {
    for name in EXAMPLES {
        let ir = build_example(name);
        let ll = emit(&ir).unwrap();
        insta::assert_snapshot!(format!("example_{name}"), ll);
    }
}

// --- micro shapes ---------------------------------------------------------

/// Signed `Div`/`Mod`: zero-guard trap block + `MIN/-1` guard (Div ⇒ MIN, Mod ⇒
/// 0), plus wrapping `add` (no nsw).
#[test]
fn golden_micro_arith() {
    let src = r#"
fn f(a: i32, b: i32) -> i32 {
    a / b -> q;
    a % b -> r;
    q + r -> ret;
}
fn main() {
    (7, 3) -> f -> v;
    v -> println;
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    insta::assert_snapshot!("micro_arith", ll);
}

/// `Update` (ADR-0021): index guard + `llvm.memcpy` + dynamic GEP store. Built
/// via `IrBuilder` (surface rebind sugar is a later WP).
#[test]
fn golden_micro_update() {
    let mut b = IrBuilder::new();
    let arr_ty = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: 4,
    };
    let f = b
        .declare(FuncKind::Named, "main", arr_ty.clone(), arr_ty, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let a = fb.input();
        let i = fb.constant(Value::I32(2), L).unwrap();
        let v = fb.constant(Value::I32(99), L).unwrap();
        fb.update(a, i, v, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let ll = emit(&ir).unwrap();
    insta::assert_snapshot!("micro_update", ll);
}

/// Two sequential canonical loops in one fn (S12 P0 shape) — each its own
/// guard-first quartet, exit attribution by route-feeder membership.
#[test]
fn golden_micro_two_loops() {
    let src = r#"
fn f(n: i32) -> i32 {
    mut i: i32 <- 0;
    mut a: i32 <- 0;
    loop {
        (i < n) -> {
            -true-> { a + 2 -> a; i + 1 -> i; -> loop; }
            -false-> a -> aa;
        }
    }
    mut j: i32 <- 0;
    mut b: i32 <- 0;
    loop {
        (j < n) -> {
            -true-> { b + 3 -> b; j + 1 -> j; -> loop; }
            -false-> b -> bb;
        }
    }
    aa + bb -> ret;
}
fn main() {
    4 -> f -> r;
    r -> println;
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    insta::assert_snapshot!("micro_two_loops", ll);
}

// --- determinism (L2) -----------------------------------------------------

#[test]
fn determinism_emit_twice_byte_equal() {
    for name in ["sum_to_n", "sepia", "vector_add"] {
        let a = emit(&build_example(name)).unwrap();
        let b = emit(&build_example(name)).unwrap();
        assert_eq!(a, b, "{name}: emit is not byte-deterministic");
    }
}

// --- Unsupported pin (L3) -------------------------------------------------

/// A multi-merge nested loop (two loops cross-fed into one SCC): not the
/// canonical quartet. Shape copied from `flow-rewrite/tests/identity.rs`.
fn multi_merge_nested_loop() -> CategoryIr {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "nested", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let c0 = fb.constant(Value::I32(0), L).unwrap();
        let c1 = fb.constant(Value::I32(0), L).unwrap();
        let outer = fb.begin_loop(c0, L).unwrap();
        let om = fb.merge_of(&outer);
        let inner = fb.begin_loop(c1, L).unwrap();
        let im = fb.merge_of(&inner);
        let ct = fb.constant(Value::Bool(true), L).unwrap();
        let cf = fb.constant(Value::Bool(false), L).unwrap();
        let inext = fb
            .binop(Operation::Add, om, im, Dest::Fresh(Some("inext".into())), L)
            .unwrap();
        fb.loop_back(&inner, inext, ct, L).unwrap();
        let onext = fb
            .binop(Operation::Add, im, om, Dest::Fresh(Some("onext".into())), L)
            .unwrap();
        fb.loop_back(&outer, onext, ct, L).unwrap();
        fb.loop_exit(&inner, im, cf, Dest::Fresh(Some("iout".into())), L)
            .unwrap();
        fb.end_loop(inner).unwrap();
        fb.loop_exit(&outer, om, cf, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(outer).unwrap();
        fb.finish().unwrap();
    }
    b.seal(f).expect("fused nested loops seal")
}

#[test]
fn nested_loop_is_unsupported() {
    let ir = multi_merge_nested_loop();
    let e = emit(&ir).unwrap_err();
    assert!(
        matches!(e, EmitError::Unsupported { ref feature, .. } if feature == "nested loops"),
        "expected Unsupported {{ nested loops }}, got {e:?}"
    );
}

/// S13 orchestrator-review regression: an exit-only computed payload
/// (`acc + 12345` consumed only by the exit route) belongs to the decide cone
/// but lives outside the loop SCC. The walk must not re-emit it after the loop
/// (dead recompute for values; a DOUBLE side effect for an exit-arm Print) —
/// the constant appears exactly once in the module.
#[test]
fn exit_only_payload_emitted_once() {
    let src = r#"
fn f(n: i32) -> i32 {
    mut i: i32 <- 0;
    mut acc: i32 <- 0;
    loop {
        (i < n) -> {
            -true-> { acc + 2 -> acc; i + 1 -> i; -> loop; }
            -false-> acc + 12345 -> ret;
        }
    }
}
fn main() { 4 -> f -> r; r -> println; }
"#;
    let ll = emit(&lower_src(src)).unwrap();
    assert_eq!(
        ll.matches("12345").count(),
        1,
        "exit-only payload must be emitted exactly once (driver-owned):\n{ll}"
    );
}
