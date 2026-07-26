//! Acceptance — the green line (DESIGN §7.1) + determinism (§7.5).
//!
//! Every in-Core example runs `parse → lower → check` and asserts **zero
//! diagnostics** (accept = empty word). Plus: a two-violation program checked
//! twice yields a byte-identical `Vec<Diagnostic>` (C-check-5), in the order
//! exclusivity-then-effects.

use mapal_check::check;
use mapal_ir::{CategoryIr, FuncKind, IrBuilder, SourceLoc as IrLoc, Ty, Value};
use mapal_syntax::Program;

/// Read one of the acceptance programs from `examples/`.
fn example(name: &str) -> String {
    let path = format!(
        "{}/../../examples/{}.mapal",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// Parse + lower a Mapal source into `(program, sealed IR)` (asserting parse +
/// lower clean), mirroring interp's acceptance helper.
fn build(src: &str) -> (Program, CategoryIr) {
    let po = mapal_syntax::parse(src);
    assert!(
        po.diagnostics.is_empty(),
        "parse diagnostics: {:?}",
        po.diagnostics
    );
    let ir =
        mapal_lower::lower(src, &po.program).unwrap_or_else(|ds| panic!("lower failed: {ds:?}"));
    (po.program, ir)
}

/// Assert `example(name)` checks clean.
fn assert_clean(name: &str) {
    let src = example(name);
    let (program, ir) = build(&src);
    let diags = check(&src, &program, &ir);
    assert!(diags.is_empty(), "{name}: expected clean, got {diags:?}");
}

#[test]
fn accept_abs() {
    assert_clean("abs");
}

#[test]
fn accept_sum_to_n() {
    assert_clean("sum_to_n");
}

#[test]
fn accept_pipeline() {
    assert_clean("pipeline");
}

#[test]
fn accept_fanout() {
    assert_clean("fanout");
}

#[test]
fn accept_fir() {
    assert_clean("fir");
}

#[test]
fn accept_sepia() {
    assert_clean("sepia");
}

#[test]
fn accept_zip_demo() {
    assert_clean("zip_demo");
}

#[test]
fn accept_vector_add() {
    assert_clean("vector_add");
}

#[test]
fn accept_calc() {
    assert_clean("calc");
}

/// seq_demo (ADR-0019): effectful `println`s inside a top-level `seq` block are
/// legal (sticky context stays sequential — the seq never opens the E2 context).
#[test]
fn accept_seq_demo() {
    assert_clean("seq_demo");
}

// --- determinism (DESIGN §7.5) --------------------------------------------

/// A program that violates E2 twice (two `print`s in a `Plain` fanout branch),
/// so `check` returns a non-trivial Vec whose order and content must be stable.
const TWO_VIOLATIONS_SRC: &str = r#"
fn main() {
    6 -> {
        -> println;
        -> println;
    }
}
"#;

#[test]
fn determinism_identical_across_runs() {
    let (program, ir) = build(TWO_VIOLATIONS_SRC);
    let a = check(TWO_VIOLATIONS_SRC, &program, &ir);
    let b = check(TWO_VIOLATIONS_SRC, &program, &ir);
    assert_eq!(a, b, "check is not deterministic");
    assert!(a.len() >= 2, "expected ≥2 diagnostics, got {a:?}");
    assert!(
        a.iter().all(|d| d.code.0 == "T0201"),
        "expected all T0201, got {a:?}"
    );
}

/// Build a single-`main` IR writing `n` full-value i32 constants to the i32
/// Return (each `Output` a distinct loc) — the hand-built multi-writer shape
/// lower never emits (validate §9 disclaims it). `n > 1` ⇒ `n-1` T0101s.
fn multi_writer_ir(n: u32) -> CategoryIr {
    let mut b = IrBuilder::new();
    let f = b
        .declare(
            FuncKind::Named,
            "main",
            Ty::Unit,
            Ty::i32(),
            IrLoc::new(0, 1),
        )
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        for i in 0..n {
            let c = fb
                .constant(Value::I32(i as i32), IrLoc::new(i + 1, i + 2))
                .unwrap();
            fb.output(c, None, IrLoc::new(i + 1, i + 2)).unwrap();
        }
        fb.finish().unwrap();
    }
    b.seal(f).unwrap()
}

/// §7.5 cross-pass ordering — the pin the same-rule determinism test above cannot
/// cover. DESIGN §7.5 fixes `order = exclusivity-then-effects`, and C-check-5
/// pins the concatenation in lib.rs. A program that violates BOTH rules must emit
/// its T0101s (exclusivity, pass 1) *before* its T0201s (effects, pass 2).
///
/// This is the only way to exercise that concatenation order: no real
/// `parse → lower` program yields a T0101 (lower is always single-writer), so we
/// pair a hand-built multi-writer IR (T0101 source) with the fanout-of-prints
/// tree (T0201 source). Reordering the two passes in lib.rs would flip this Vec
/// and fail here — the single-rule tests above would all still pass.
#[test]
fn cross_pass_order_is_exclusivity_then_effects() {
    // Tree: `main` with a Plain fanout of two `println`s → two T0201.
    let po = mapal_syntax::parse(TWO_VIOLATIONS_SRC);
    assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
    // IR: `main` with two full-value Return writers → one T0101.
    let ir = multi_writer_ir(2);

    let diags = check(TWO_VIOLATIONS_SRC, &po.program, &ir);
    assert_eq!(
        diags.iter().map(|d| d.code.0).collect::<Vec<_>>(),
        ["T0101", "T0201", "T0201"],
        "order must be exclusivity-then-effects: {diags:?}"
    );
    // Determinism holds on the combined-rule input too (byte-identical Vec).
    assert_eq!(diags, check(TWO_VIOLATIONS_SRC, &po.program, &ir));
}
