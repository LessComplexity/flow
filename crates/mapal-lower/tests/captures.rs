//! ADR-0027 positive lowering pins for capture semantics (plan-capture-semantics
//! §lower): the free-variable analysis, the hidden-parameter wiring (the body
//! input product `(c₁…cₖ, …)`), and the narrowed L1108's remaining cases.
//!
//! The two `*_lowers_with_capture_input_product` tests are the pre-ADR-0027
//! rejection-matrix rows `l1108_capture_in_body` /
//! `l1108_capture_in_seq_in_map_body`, converted: reads of enclosing bindings
//! are legal captures now (D1), so the same programs lower clean.

mod common;
use common::{lower_err_codes, lower_ok};
use mapal_ir::{FuncKind, Ty};

/// The single MapBody/FoldBody fn's input ty in `ir` (exactly one expected).
fn body_input_ty(ir: &mapal_ir::CategoryIr, kind: FuncKind) -> Ty {
    let mut found = None;
    for (_, def) in ir.funcs() {
        if def.kind == kind {
            assert!(found.is_none(), "more than one {kind:?} fn");
            found = Some(ir.object(def.input).expect("body input").ty.clone());
        }
    }
    found.expect("a body fn")
}

#[test]
fn capture_map_lowers_with_capture_input_product() {
    // Was `l1108_capture_in_body` (rejection.rs): the read of enclosing `k`
    // inside the map body is a legal capture now — the body input is the
    // `(k, x)` product with the capture leading.
    let ir = lower_ok(
        "fn main() {\n    mut k: i32 <- 5;\n    [1, 2, 3] -> map { x -> x + k } -> ys: [i32; 3];\n}\n",
    );
    assert_eq!(
        body_input_ty(&ir, FuncKind::MapBody),
        Ty::Tuple(vec![Ty::i32(), Ty::i32()])
    );
}

#[test]
fn capture_in_seq_in_map_body_lowers() {
    // Was `l1108_capture_in_seq_in_map_body` (rejection.rs): the capture walk
    // descends into the `seq` body (ADR-0019 §8.10) and the read of enclosing
    // `k` lowers as a capture — never the misleading L1101.
    let ir = lower_ok(
        "fn main() { 5 -> k; [1,2,3] -> map { e -> e -> seq { e + k -> a; a } } -> r; r[0] -> println; }\n",
    );
    assert_eq!(
        body_input_ty(&ir, FuncKind::MapBody),
        Ty::Tuple(vec![Ty::i32(), Ty::i32()])
    );
}

#[test]
fn free_var_collection_is_source_order_of_first_use_and_deduped() {
    // `a + bs[0] + e` reads `a` then `bs` then the param: the capture order is
    // (a, bs) — visible as the leading component tys of the body input
    // product (i32, [i32; 2], i32). Repeat reads dedup (`a + a` → one).
    let ir = lower_ok(
        "fn main() {\n    1 -> a;\n    [10, 20] -> bs: [i32; 2];\n    [1, 2, 3] -> map { e -> a + bs[0] + e } -> ys: [i32; 3];\n}\n",
    );
    assert_eq!(
        body_input_ty(&ir, FuncKind::MapBody),
        Ty::Tuple(vec![
            Ty::i32(),
            Ty::Array {
                elem: Box::new(Ty::i32()),
                size: 2
            },
            Ty::i32()
        ])
    );

    let ir2 = lower_ok(
        "fn main() {\n    1 -> a;\n    [1, 2, 3] -> map { e -> a + a + e } -> ys: [i32; 3];\n}\n",
    );
    assert_eq!(
        body_input_ty(&ir2, FuncKind::MapBody),
        Ty::Tuple(vec![Ty::i32(), Ty::i32()])
    );

    // ADR-0027 review major #10: first-use order, NOT name order — `zarr`
    // (used first) leads `a` even though `a` sorts earlier; within one
    // expression operands record left-to-right (`zarr[0] + a`, not `a` then
    // `zarr`). Distinct tys make the order observable: ([i32; 2], i32, i32).
    let ir3 = lower_ok(
        "fn main() {\n    [9, 8] -> zarr: [i32; 2];\n    5 -> a;\n    [1, 2, 3] -> map { e -> zarr[0] + a + e } -> ys: [i32; 3];\n}\n",
    );
    assert_eq!(
        body_input_ty(&ir3, FuncKind::MapBody),
        Ty::Tuple(vec![
            Ty::Array {
                elem: Box::new(Ty::i32()),
                size: 2
            },
            Ty::i32(),
            Ty::i32()
        ])
    );
}

#[test]
fn capture_fold_lowers_with_capture_input_product() {
    // A capturing fold: body input is `(scale, acc, x)` — capture leading,
    // then acc, then the element.
    let ir = lower_ok(
        "fn main() {\n    3 -> scale;\n    [1, 2, 3] -> a;\n    (0, a) -> fold { acc, x -> acc + x * scale } -> total;\n    total -> println;\n}\n",
    );
    assert_eq!(
        body_input_ty(&ir, FuncKind::FoldBody),
        Ty::Tuple(vec![Ty::i32(), Ty::i32(), Ty::i32()])
    );
}

#[test]
fn str_capture_reports_l1206() {
    // A `Str` value cannot be a capture: packing it into the body-input
    // product is `StrOutsidePrint` → L1206.
    let src = "fn main() {\n    \"hi\" -> s;\n    [1, 2] -> map { e -> s } -> ys;\n}\n";
    let codes = lower_err_codes(src);
    assert!(
        codes.iter().any(|c| c == "L1206"),
        "expected L1206, got {codes:?}"
    );
}

#[test]
fn write_in_nested_body_reports_exactly_one_l1108() {
    // ADR-0027 review major #9: a write to an enclosing name inside a NESTED
    // body (a fold inside a map body) must draw exactly ONE L1108 — the inner
    // body owns the write diagnostic. The outer body_captures walk descends
    // into the nested MapFold stage for READS (transitive captures, Q3) but
    // must not also record the write (before the fix the same write surfaced
    // twice: once from the outer walk's shared `acc`, once from the inner).
    let src = "fn main() {\n    mut c: [i32; 3] <- [0, 0, 0];\n    7 -> k;\n    [1, 2] -> map { e ->\n        (0, [e]) -> fold { acc, x ->\n            c[0] <- k;\n            acc + x\n        }\n    } -> ys: [i32; 2];\n}\n";
    let codes = lower_err_codes(src);
    let n = codes.iter().filter(|c| c.as_str() == "L1108").count();
    assert_eq!(n, 1, "exactly one L1108, got {codes:?}");
}
