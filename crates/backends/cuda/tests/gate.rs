//! WP1 gate tests: the L3 capability gate (`Unsupported { "nested loops" }`
//! on a hand-built multi-merge graph, llvm BL6/BL7 parity), the positive gate
//! path on a canonical loop (WP4: the host-driven quartet emits), and
//! determinism of `emit` on the same sealed IR.

use mapal_backend_cuda::{EmitError, emit};
use mapal_ir::{CategoryIr, Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value};

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

fn lower_src(src: &str) -> CategoryIr {
    let po = mapal_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
    mapal_lower::lower(src, &po.program).unwrap_or_else(|d| panic!("lower: {d:?}"))
}

fn build_example(name: &str) -> CategoryIr {
    let path = format!(
        "{}/../../../examples/{}.mapal",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    let src = std::fs::read_to_string(&path).unwrap();
    lower_src(&src)
}

/// A multi-merge nested loop (two loops cross-fed into one SCC): not the
/// canonical quartet. Shape copied from `mapal-backend-llvm/tests/golden_ll.rs`
/// (itself from `mapal-rewrite/tests/identity.rs`).
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

/// A canonical guard-first loop (the `sum_to_n` example) passes the L3 gate
/// and (WP4) emits a full `.cu` module — the host-driven quartet.
#[test]
fn canonical_loop_passes_gate() {
    let ir = build_example("sum_to_n");
    let cu = emit(&ir).unwrap();
    assert!(cu.contains("while (true) {"), "{cu}");
    assert!(cu.contains("int main()"), "{cu}");
}

/// A loop-free program passes the gate and (WP2) emits a full `.cu` module.
#[test]
fn straight_line_passes_gate() {
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
    let ir = lower_src(src);
    let cu = emit(&ir).unwrap();
    assert!(cu.contains("int main()"), "{cu}");
    assert!(cu.contains("mapal_trap(0)"), "{cu}");
}

/// L2 on the emit surface: `emit` twice on the same sealed IR yields identical
/// results — on the gate-rejection path (trivially the same `Err`) and on the
/// WP4 loop-driver path (sum_to_n's canonical loop).
#[test]
fn determinism_emit_twice_identical() {
    let nested = multi_merge_nested_loop();
    assert_eq!(emit(&nested), emit(&nested));

    let canonical = build_example("sum_to_n");
    assert_eq!(emit(&canonical), emit(&canonical));
}
