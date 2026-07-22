//! ADR-0027 review blocker #1: computed exit-arm payloads (loop_plan gap).
//!
//! `loop_plan`'s `body_order` admitted SCC-incident morphisms plus *direct*
//! route-target Pair edges only, so a computation whose result feeds just the
//! exit route (and whose inputs trace to the merge) was never scheduled by the
//! loop driver — interp panicked read-before-write; llvm emitted it after the
//! loop. The fix widens the cone to the transitive backward cone of the route
//! feeders. These pins hold the full parse → lower → run pipeline to the
//! contract (the interp is the oracle).

const BUDGET: u64 = 100_000;

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
fn computed_exit_payload_runs() {
    // Zero captures: the exit arm is a *computed* payload (`t * 2`), not a
    // bare carried name. `t * 2` feeds only the exit route; its inputs trace
    // to the merge.
    let src = r#"
fn f(n: i32) -> i32 {
    mut t: i32 <- 1;
    mut k: i32 <- 0;
    loop {
        (k < n) -> {
            -true-> {
                t + 1 -> t;
                k + 1 -> k;
                -> loop;
            }
            -false-> t * 2 -> ret;
        }
    }
}

fn main() {
    3 -> f -> r;
    r -> println;
}
"#;
    let rr = run_src(src);
    // k=0..2 advance t to 4; exit step computes 4 * 2 = 8.
    assert_eq!(rr.output, "8\n", "outcome {:?}", rr.outcome);
}

#[test]
fn exit_arm_effect_fires_once_on_the_exit_step() {
    // An exit-arm effect with a merge-derived value is in the decide cone but
    // NOT in the SCC (its token never cycles back). The straight-line walk
    // must skip plan-owned morphisms (the llvm `func.rs` walk rule) or the
    // println fires a second time after the loop.
    let src = r#"
fn f(n: i32) -> i32 {
    mut k: i32 <- 0;
    mut acc: i32 <- 0;
    loop {
        (k < n) -> {
            -true-> {
                acc + 1 -> acc;
                k + 1 -> k;
                -> loop;
            }
            -false-> {
                acc -> println;
                acc -> ret;
            }
        }
    }
}

fn main() {
    3 -> f -> r;
    r -> println;
}
"#;
    let rr = run_src(src);
    // The exit-arm println fires exactly once (the exit step's acc = 3).
    assert_eq!(rr.output, "3\n3\n", "outcome {:?}", rr.outcome);
}

#[test]
fn captured_map_in_exit_arm_runs() {
    // Exit arm computes a CAPTURED map: `xs -> map { e -> e + t } -> ret;` —
    // the fanout reads the loop-carried `t` (read-at-position: the exit
    // step's current value) and feeds only the exit route.
    let src = r#"
fn f(xs: [i32; 3], n: i32) -> [i32; 3] {
    mut t: i32 <- 10;
    mut k: i32 <- 0;
    loop {
        (k < n) -> {
            -true-> {
                t + 1 -> t;
                k + 1 -> k;
                -> loop;
            }
            -false-> xs -> map { e -> e + t } -> ret;
        }
    }
}

fn main() {
    ([1, 2, 3], 2) -> f -> r;
    r[0] -> println;
    r[2] -> println;
}
"#;
    let rr = run_src(src);
    // k=0..1 advance t to 12; exit step maps e + 12 over [1,2,3].
    assert_eq!(rr.output, "13\n15\n", "outcome {:?}", rr.outcome);
}
