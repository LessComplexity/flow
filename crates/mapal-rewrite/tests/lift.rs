//! R-LF / R-LM directed pins: both accepted forms and every ratified v1
//! rejection. Sources go through the real parser/lowerer; interp is the R1
//! oracle.

use mapal_interp::run;
use mapal_ir::{CategoryIr, Operation, validate};
use mapal_rewrite::{PassId, analyze_lift, rewrite_with};

const BUDGET: u64 = 200_000;

fn lower_src(src: &str) -> CategoryIr {
    let po = mapal_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
    mapal_lower::lower(src, &po.program).unwrap_or_else(|d| panic!("lower: {d:?}"))
}

fn loop_count(ir: &CategoryIr) -> usize {
    ir.funcs().map(|(f, _)| ir.loop_structure(f).len()).sum()
}

fn op_count(ir: &CategoryIr, pred: impl Fn(Operation) -> bool) -> usize {
    ir.morphisms().filter(|(_, m)| pred(m.op)).count()
}

#[test]
fn canonical_counter_acc_lifts_to_captured_fold() {
    let src = r#"
fn fold4(x: i32) -> i32 {
    mut i: i32 <- 0;
    mut acc: i32 <- x;
    loop {
        (i < 4) -> {
            -true-> {
                acc + i + x -> acc;
                i + 1 -> i;
                -> loop;
            }
            -false-> acc -> ret;
        }
    }
}
fn main() { 3 -> fold4 -> r; r -> println; }
"#;
    let raw = lower_src(src);
    let before = run(&raw, BUDGET);
    let plan = analyze_lift(&raw);
    assert_eq!(plan.lift.len(), 1);

    let out = rewrite_with(raw, &[PassId::LiftLoops]);
    assert!(validate(&out.ir).is_empty(), "{:?}", validate(&out.ir));
    assert_eq!(run(&out.ir, BUDGET), before);
    assert_eq!(loop_count(&out.ir), 0);
    assert_eq!(
        op_count(&out.ir, |op| matches!(
            op,
            Operation::Fold { captures: 1, .. }
        )),
        1,
        "x is the synthesized FoldBody capture"
    );
    assert_eq!(op_count(&out.ir, |op| op == Operation::Iota), 1);

    let twice = rewrite_with(out.ir, &[PassId::LiftLoops]);
    assert!(twice.report.applied.is_empty(), "lift is idempotent");
}

#[test]
fn canonical_counter_identity_update_lifts_to_captured_map() {
    let src = r#"
fn map3(x: i32) -> [i32; 3] {
    mut c: [i32; 3] <- [9, 9, 9];
    mut t: i32 <- 0;
    loop {
        (t < 3) -> {
            -true-> {
                c[t] <- x + t;
                t + 1 -> t;
                -> loop;
            }
            -false-> c -> ret;
        }
    }
}
fn main() { 7 -> map3 -> r; r[2] -> println; }
"#;
    let raw = lower_src(src);
    let before = run(&raw, BUDGET);
    let plan = analyze_lift(&raw);
    assert_eq!(plan.lift.len(), 1);

    let out = rewrite_with(raw, &[PassId::LiftLoops]);
    assert!(validate(&out.ir).is_empty(), "{:?}", validate(&out.ir));
    assert_eq!(run(&out.ir, BUDGET), before);
    assert_eq!(loop_count(&out.ir), 0);
    assert_eq!(
        op_count(&out.ir, |op| matches!(
            op,
            Operation::Map { captures: 1, .. }
        )),
        1,
        "x is the synthesized MapBody capture"
    );
    assert_eq!(op_count(&out.ir, |op| op == Operation::Update), 0);
}

fn assert_stays_loop(name: &str, src: &str) {
    let ir = lower_src(src);
    let before = ir.to_mermaid();
    assert_eq!(loop_count(&ir), 1, "{name}: test shape");
    assert!(
        analyze_lift(&ir).lift.is_empty(),
        "{name}: rejection unexpectedly planned"
    );
    let out = rewrite_with(ir, &[PassId::LiftLoops]);
    assert!(out.report.applied.is_empty(), "{name}: pass fired");
    assert_eq!(out.ir.to_mermaid(), before, "{name}: graph changed");
    assert_eq!(loop_count(&out.ir), 1, "{name}: loop disappeared");
}

#[test]
fn rejections_stay_loops() {
    let sum_to_n = include_str!("../../../examples/sum_to_n.mapal");
    assert_stays_loop("non-const bound (sum_to_n)", sum_to_n);

    let fib = r#"
fn fib3(dummy: i32) -> i32 {
    mut a: i32 <- 0;
    mut b: i32 <- 1;
    mut i: i32 <- 0;
    loop {
        (i < 3) -> {
            -true-> { a + b -> n; b -> a; n -> b; i + 1 -> i; -> loop; }
            -false-> a -> ret;
        }
    }
}
fn main() { 0 -> fib3 -> r; r -> println; }
"#;
    assert_stays_loop("extra carried state (fib)", fib);

    let countdown = r#"
fn countdown(dummy: i32) -> i32 {
    mut i: i32 <- 0;
    loop {
        (i < 3) -> {
            -true-> { i -> println; i + 1 -> i; -> loop; }
            -false-> i -> ret;
        }
    }
}
fn main() { 0 -> countdown -> r; r -> println; }
"#;
    assert_stays_loop("effects/token in SCC (countdown)", countdown);

    let multiple_updates = r#"
fn f(dummy: i32) -> [i32; 3] {
    mut c: [i32; 3] <- [0, 0, 0];
    mut i: i32 <- 0;
    loop {
        (i < 3) -> {
            -true-> { c[i] <- i; c[i] <- i + 1; i + 1 -> i; -> loop; }
            -false-> c -> ret;
        }
    }
}
fn main() { 0 -> f -> r; r[0] -> println; }
"#;
    assert_stays_loop("multiple Updates", multiple_updates);

    let non_identity_index = r#"
fn f(dummy: i32) -> [i32; 3] {
    mut c: [i32; 3] <- [0, 0, 0];
    mut i: i32 <- 0;
    loop {
        (i < 3) -> {
            -true-> { c[0] <- i; i + 1 -> i; -> loop; }
            -false-> c -> ret;
        }
    }
}
fn main() { 0 -> f -> r; r[0] -> println; }
"#;
    assert_stays_loop("non-identity index", non_identity_index);

    let size_mismatch = r#"
fn f(dummy: i32) -> [i32; 3] {
    mut c: [i32; 3] <- [0, 0, 0];
    mut i: i32 <- 0;
    loop {
        (i < 2) -> {
            -true-> { c[i] <- i; i + 1 -> i; -> loop; }
            -false-> c -> ret;
        }
    }
}
fn main() { 0 -> f -> r; r[0] -> println; }
"#;
    assert_stays_loop("n != T", size_mismatch);

    let step_two = r#"
fn f(dummy: i32) -> i32 {
    mut i: i32 <- 0;
    mut acc: i32 <- 0;
    loop {
        (i < 4) -> {
            -true-> { acc + i -> acc; i + 2 -> i; -> loop; }
            -false-> acc -> ret;
        }
    }
}
fn main() { 0 -> f -> r; r -> println; }
"#;
    assert_stays_loop("step != +1", step_two);

    let init_one = r#"
fn f(dummy: i32) -> i32 {
    mut i: i32 <- 1;
    mut acc: i32 <- 0;
    loop {
        (i < 4) -> {
            -true-> { acc + i -> acc; i + 1 -> i; -> loop; }
            -false-> acc -> ret;
        }
    }
}
fn main() { 0 -> f -> r; r -> println; }
"#;
    assert_stays_loop("init != 0", init_one);

    let zero_trip = r#"
fn f(dummy: i32) -> i32 {
    mut i: i32 <- 0;
    mut acc: i32 <- 7;
    loop {
        (i < 0) -> {
            -true-> { acc + i -> acc; i + 1 -> i; -> loop; }
            -false-> acc -> ret;
        }
    }
}
fn main() { 0 -> f -> r; r -> println; }
"#;
    assert_stays_loop("K = 0", zero_trip);

    let reads_output = r#"
fn f(dummy: i32) -> [i32; 3] {
    mut c: [i32; 3] <- [1, 2, 3];
    mut i: i32 <- 0;
    loop {
        (i < 3) -> {
            -true-> { c[i] <- c[0] + i; i + 1 -> i; -> loop; }
            -false-> c -> ret;
        }
    }
}
fn main() { 0 -> f -> r; r[0] -> println; }
"#;
    assert_stays_loop("value cone reads c", reads_output);

    let extra_advance_work = r#"
fn f(dummy: i32) -> i32 {
    mut i: i32 <- 0;
    mut acc: i32 <- 0;
    loop {
        (i < 3) -> {
            -true-> {
                10 / (i - 1) -> unused;
                acc + i -> acc;
                i + 1 -> i;
                -> loop;
            }
            -false-> acc -> ret;
        }
    }
}
fn main() { 0 -> f -> r; r -> println; }
"#;
    assert_stays_loop("extra counter-dependent advance work", extra_advance_work);
}
