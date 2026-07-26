//! S12 regressions: computed (multi-hop) loop-invariants inside loop bodies.
//!
//! A loop body reading a value DERIVED from a parameter (`x * 2`,
//! `a[i * 4 + k]`) used to panic the interpreter at read-before-write: FIFO-Kahn
//! `topo_order` released `LoopEnter` before second-generation-ready invariant
//! morphisms, so the driver fired with the invariant unwritten. Fixed in
//! `mapal-ir::topo_order` (LoopEnter deferral); these pins hold the full
//! parse → lower → run pipeline to the contract. fir covers the 1-hop case
//! (direct param proj); these cover the multi-hop class it missed.

const BUDGET: u64 = 100_000;

fn run_src(src: &str) -> mapal_interp::RunResult {
    let po = mapal_syntax::parse(src);
    assert!(
        po.diagnostics.is_empty(),
        "parse-clean: {:?}",
        po.diagnostics
    );
    let ir = mapal_lower::lower(src, &po.program).expect("lowers clean");
    mapal_interp::run(&ir, BUDGET)
}

#[test]
fn computed_invariant_in_loop_body_runs() {
    let src = r#"
fn f(x: i32) -> i32 {
    mut k: i32   <- 0;
    mut acc: i32 <- 0;
    loop {
        (k < 3) -> {
            -true-> {
                acc + x * 2 -> acc;
                k + 1 -> k;
                -> loop;
            }
            -false-> acc -> ret;
        }
    }
}

fn main() {
    5 -> f -> r;
    r -> println;
}
"#;
    let rr = run_src(src);
    assert_eq!(
        rr.output, "30\n",
        "3 iterations of +10; outcome {:?}",
        rr.outcome
    );
}

#[test]
fn matmul4_dynamic_indexing_with_computed_invariants() {
    // The user-found S12 shape: `a[i * 4 + k]` — dynamic indexing whose index
    // arithmetic mixes loop-invariant (i * 4) and carried (k) values.
    let src = r#"
fn cell(a: [f32; 16], b: [f32; 16], i: i32, j: i32) -> f32 {
    mut k: i32   <- 0;
    mut acc: f32 <- 0.0;
    loop {
        (k < 4) -> {
            -true-> {
                acc + a[i * 4 + k] * b[k * 4 + j] -> acc;
                k + 1 -> k;
                -> loop;
            }
            -false-> acc -> ret;
        }
    }
}

fn matmul4(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    (a, b, 0, 0) -> cell -> c00;  (a, b, 0, 1) -> cell -> c01;
    (a, b, 0, 2) -> cell -> c02;  (a, b, 0, 3) -> cell -> c03;
    (a, b, 1, 0) -> cell -> c10;  (a, b, 1, 1) -> cell -> c11;
    (a, b, 1, 2) -> cell -> c12;  (a, b, 1, 3) -> cell -> c13;
    (a, b, 2, 0) -> cell -> c20;  (a, b, 2, 1) -> cell -> c21;
    (a, b, 2, 2) -> cell -> c22;  (a, b, 2, 3) -> cell -> c23;
    (a, b, 3, 0) -> cell -> c30;  (a, b, 3, 1) -> cell -> c31;
    (a, b, 3, 2) -> cell -> c32;  (a, b, 3, 3) -> cell -> c33;
    [c00, c01, c02, c03, c10, c11, c12, c13,
     c20, c21, c22, c23, c30, c31, c32, c33] -> ret;
}

fn main() {
    [ 1.0,  2.0,  3.0,  4.0,
      5.0,  6.0,  7.0,  8.0,
      9.0, 10.0, 11.0, 12.0,
     13.0, 14.0, 15.0, 16.0] -> a: [f32; 16];

    [1.0, 0.0, 0.0, 0.0,
     0.0, 1.0, 0.0, 0.0,
     0.0, 0.0, 1.0, 0.0,
     0.0, 0.0, 0.0, 1.0] -> b: [f32; 16];

    (a, b) -> matmul4 -> c;

    c[7] -> println;
    (0.0, c) -> fold { acc, x -> acc + x } -> sum;
    sum -> println;
}
"#;
    let rr = run_src(src);
    assert_eq!(
        rr.output, "8\n136\n",
        "C = A under identity; outcome {:?}",
        rr.outcome
    );
}

#[test]
fn two_sequential_loops_in_one_fn() {
    // S12 regression #2: derive_plan attributed exits/body morphisms against the
    // per-function SCC *union*, so the second loop's exit was attributed to the
    // first merge — the "exactly one attributed LoopExit" assert panicked on a
    // legal Core program. Attribution is now per-merge-SCC.
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
    let rr = run_src(src);
    assert_eq!(rr.output, "20\n", "4*2 + 4*3; outcome {:?}", rr.outcome);
}
