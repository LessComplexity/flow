//! Property tests (DESIGN §14.5), bounded and fast:
//!
//! (i) random arithmetic/pipeline/guard Core programs: `lower` never panics; an
//!     `Ok(ir)` is `validate`-empty and `lint_mermaid`-clean.
//! (ii) random literal-width scenarios: a resolved annotation is never
//!     contradicted (the program lowers clean).

use proptest::prelude::*;

use flow_lower::lower;
use flow_syntax::parse;

/// A small grammar-directed generator for arithmetic pipelines:
/// `fn f(data: i32) -> i32 { data <op> <lit> -> + <lit> -> ... -> ret; }`.
fn pipeline_program() -> impl Strategy<Value = String> {
    let op = prop::sample::select(vec!["+", "-", "*"]);
    let stages = prop::collection::vec((op, 1i32..50), 0..6);
    stages.prop_map(|stages| {
        let mut body = String::from("    data * 2\n");
        for (op, lit) in &stages {
            body.push_str(&format!("        -> {op} {lit}\n"));
        }
        body.push_str("        -> ret;\n");
        format!("fn f(data: i32) -> i32 {{\n{body}}}\n\nfn main() {{ 7 -> f -> r; r -> print; }}\n")
    })
}

/// A small generator for bool-guard programs:
/// `fn g(x: i32) -> i32 { (x > <lit>) -> { -true-> <e>; -false-> <e>; } -> ret; }`.
fn guard_program() -> impl Strategy<Value = String> {
    (1i32..40, 0i32..100, 0i32..100).prop_map(|(thresh, a, b)| {
        format!(
            "fn g(x: i32) -> i32 {{\n    (x > {thresh}) -> {{\n        -true-> {a};\n        -false-> {b};\n    }} -> ret;\n}}\n\nfn main() {{ 9 -> g -> r; r -> print; }}\n"
        )
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 96, ..ProptestConfig::default() })]

    /// (i) Random Core programs never panic; success ⇒ validate-empty + lint-clean.
    #[test]
    fn random_programs_never_panic(src in prop_oneof![pipeline_program(), guard_program()]) {
        let po = parse(&src);
        // Generated programs are syntactically valid.
        prop_assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
        match lower(&src, &po.program) {
            Ok(ir) => {
                prop_assert!(flow_ir::validate(&ir).is_empty(), "validate violations");
                let dump = ir.to_mermaid();
                prop_assert!(flow_ir::lint_mermaid(&dump).is_empty(), "lint failures");
            }
            Err(_) => { /* a diagnostic is fine; the point is no panic. */ }
        }
    }
}

/// (ii) Random literal-width scenarios: a literal bound to a float/int
/// annotation resolves to that width and lowers clean (no annotation contradiction).
fn width_program() -> impl Strategy<Value = (String, &'static str)> {
    prop::sample::select(vec![
        ("f32", "1.5"),
        ("f64", "2.25"),
        ("i32", "7"),
        ("i64", "123"),
        ("u8", "42"),
    ])
    .prop_map(|(ty, lit)| {
        (
            format!("fn f() -> {ty} {{ {lit} -> ret; }}\n\nfn main() {{ }}\n"),
            ty,
        )
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, ..ProptestConfig::default() })]

    #[test]
    fn literal_width_never_contradicts((src, ty) in width_program()) {
        let po = parse(&src);
        prop_assert!(po.diagnostics.is_empty());
        let ir = lower(&src, &po.program).expect("annotated literal must lower");
        // The function's Return ty matches the annotation (the literal took it).
        let f = ir.funcs().find(|(_, d)| d.name == "f").unwrap().1;
        let ret_ty = &ir.object(f.output).unwrap().ty;
        let expected = match ty {
            "f32" => flow_ir::Ty::f32(),
            "f64" => flow_ir::Ty::f64(),
            "i32" => flow_ir::Ty::i32(),
            "i64" => flow_ir::Ty::i64(),
            "u8" => flow_ir::Ty::u8(),
            _ => unreachable!(),
        };
        prop_assert_eq!(ret_ty, &expected);
    }
}
