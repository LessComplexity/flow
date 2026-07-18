//! End-to-end `c[i] <- x` contracts through the full pipeline (ADR-0021, WP U4).
//!
//! The IR-builder value contracts live in `update.rs` (WP U1); these drive the
//! whole parse → lower → interp path so the `c[i] <- x` sugar (U2 syntax + U3
//! lower rebind wiring) is exercised against the interpreter oracle:
//!   - matmul4 as ONE flattened loop building the result via mut-array carried
//!     state — the ADR's motivating program (same `8\n136\n` contract the
//!     unrolled pin in `loop_invariants.rs` holds);
//!   - fanout: one source array fanned into two pure branches that each update
//!     it independently — value semantics, no cross-branch interference, source
//!     untouched;
//!   - a runtime-OOB index through the pipeline ⇒ `Trapped(IndexOob)`.

use flow_interp::{Outcome, TrapKind};

const BUDGET: u64 = 1_000_000;

fn run_src(src: &str) -> flow_interp::RunResult {
    let po = flow_syntax::parse(src);
    assert!(
        po.diagnostics.is_empty(),
        "parse-clean: {:?}",
        po.diagnostics
    );
    let ir = flow_lower::lower(src, &po.program).expect("lowers clean");
    flow_interp::run(&ir, BUDGET)
}

#[test]
fn matmul4_loop_driven_builds_result_via_mut_array() {
    // The ADR-0021 motivating shape: instead of 16 unrolled named bindings
    // (see the pinned `matmul4_dynamic_indexing_with_computed_invariants`), a
    // SINGLE flattened loop `t in 0..16` with `i = t / 4`, `j = t % 4` writes
    // each cell into a loop-carried `mut c` array via `c[t] <- v`. Same
    // `8\n136\n` contract — carried-array state through the merge is the thing
    // U3's `collect_assigns_stmt` wiring makes work.
    //
    // `c` is seeded with `b` (the identity), NOT `a`: because C = A·I = A, a
    // seed of `a` would make every `c[t] <- v` write back the value already in
    // slot t, so a dropped write or a lost loop-carry would still read the seed
    // and yield `8\n136\n` — the test would be blind to the exact miscompile
    // the ADR names. Seeding with `b` (≠ answer) means every one of the 16
    // writes must land AND ride the merge to reach `8\n136\n`.
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
    mut c: [f32; 16] <- b;
    mut t: i32       <- 0;
    loop {
        (t < 16) -> {
            -true-> {
                t / 4 -> i;
                t % 4 -> j;
                (a, b, i, j) -> cell -> v;
                c[t] <- v;
                t + 1 -> t;
                -> loop;
            }
            -false-> c -> ret;
        }
    }
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
        "C = A under identity, loop-driven; outcome {:?}",
        rr.outcome
    );
}

#[test]
fn fanout_two_branches_update_same_source_independently() {
    // One source array fanned to two pure branches (`src` used twice). Each
    // seeds a `mut c <- src` copy and writes a different slot. Value semantics:
    // neither branch observes the other's write, and `src` is untouched — the
    // fanout-safety attack from the plan's risk note (§Risks 1).
    let src = r#"
fn set_first(a: [i32; 4]) -> [i32; 4] {
    mut c: [i32; 4] <- a;
    c[0] <- 100;
    c -> ret;
}
fn set_last(a: [i32; 4]) -> [i32; 4] {
    mut c: [i32; 4] <- a;
    c[3] <- 300;
    c -> ret;
}
fn main() {
    [1, 2, 3, 4] -> src: [i32; 4];
    src -> set_first -> l;
    src -> set_last  -> r;
    l[0] -> println;
    l[3] -> println;
    r[0] -> println;
    r[3] -> println;
    src[0] -> println;
    src[3] -> println;
}
"#;
    let rr = run_src(src);
    assert_eq!(
        rr.output,
        // l = [100,2,3,4]  r = [1,2,3,300]  src = [1,2,3,4]
        "100\n4\n1\n300\n1\n4\n",
        "each branch sees only its own write; source intact; outcome {:?}",
        rr.outcome
    );
}

#[test]
fn runtime_oob_update_traps_through_pipeline() {
    // The index is a runtime parameter (`i = 4`), not a literal — so the OOB is
    // caught at eval, not lower: `c[i] <- x` with `i >= n` ⇒ Trapped(IndexOob),
    // the same trap class as a read `Index` OOB.
    let src = r#"
fn oob(a: [i32; 4], i: i32) -> i32 {
    mut c: [i32; 4] <- a;
    c[i] <- 99;
    c[0] -> ret;
}
fn main() {
    [1, 2, 3, 4] -> a: [i32; 4];
    (a, 4) -> oob -> r;
    r -> println;
}
"#;
    let rr = run_src(src);
    assert_eq!(
        rr.outcome,
        Outcome::Trapped(TrapKind::IndexOob),
        "runtime index 4 on [i32;4] traps; output {:?}",
        rr.output
    );
}
