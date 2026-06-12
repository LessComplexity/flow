//! Shared test helpers: parse-then-lower, with parse-clean and lint-clean
//! assertions (DESIGN §14).
//!
//! Each test binary compiles this module and uses a subset of its helpers, so
//! the unused-fn lint is silenced crate-wide here.
#![allow(dead_code)]

use flow_ir::CategoryIr;
use flow_syntax::parse;

/// Parse `src` (asserting zero parse diagnostics, J4) and lower it, asserting
/// success and a lint-clean, validate-empty IR. Returns the sealed IR.
pub fn lower_ok(src: &str) -> CategoryIr {
    let po = parse(src);
    assert!(
        po.diagnostics.is_empty(),
        "parse diagnostics: {:?}",
        po.diagnostics
    );
    let ir = match flow_lower::lower(src, &po.program) {
        Ok(ir) => ir,
        Err(ds) => panic!("lower failed: {ds:?}"),
    };
    // Round-trip: seal Ok ⇒ validate empty (DESIGN §14.2).
    let v = flow_ir::validate(&ir);
    assert!(v.is_empty(), "validate violations: {v:?}");
    // Every dump is lint-clean (DESIGN §14.1).
    let dump = ir.to_mermaid();
    let lints = flow_ir::lint_mermaid(&dump);
    assert!(lints.is_empty(), "mermaid lint failures: {lints:?}");
    ir
}

/// Read one of the six acceptance programs from `examples/`.
pub fn example(name: &str) -> String {
    let path = format!(
        "{}/../../examples/{}.flow",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// Lower `src` expecting failure; return the diagnostic codes (as strings).
pub fn lower_err_codes(src: &str) -> Vec<String> {
    let po = parse(src);
    // The rejection-matrix programs are parse-clean (lower owns the rejection).
    assert!(
        po.diagnostics.is_empty(),
        "expected a parse-clean program; parse diagnostics: {:?}",
        po.diagnostics
    );
    match flow_lower::lower(src, &po.program) {
        Ok(_) => Vec::new(),
        Err(ds) => ds.iter().map(|d| d.code.0.to_string()).collect(),
    }
}

/// Assert lowering `src` yields a diagnostic with code `code`.
pub fn assert_rejects(src: &str, code: &str) {
    let codes = lower_err_codes(src);
    assert!(
        codes.iter().any(|c| c == code),
        "expected `{code}`, got {codes:?}\nsource:\n{src}"
    );
}
