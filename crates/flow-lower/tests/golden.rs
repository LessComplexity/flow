//! Golden Mermaid snapshots (DESIGN §14.1): the six acceptance programs plus the
//! two §9 non-example regressions (countdown, effectful call). Each snapshot is
//! lint-clean (asserted in `lower_ok`) and was hand-verified against §9's shape
//! contract before acceptance (see the agent report's per-golden notes).

mod common;
use common::{example, lower_ok};

/// Snapshot the Mermaid dump of a lowered program.
fn snap(name: &str, src: &str) {
    let ir = lower_ok(src);
    let dump = ir.to_mermaid();
    insta::assert_snapshot!(name, dump);
}

#[test]
fn golden_pipeline() {
    snap("pipeline", &example("pipeline"));
}

#[test]
fn golden_abs() {
    snap("abs", &example("abs"));
}

#[test]
fn golden_sum_to_n() {
    snap("sum_to_n", &example("sum_to_n"));
}

#[test]
fn golden_fir() {
    snap("fir", &example("fir"));
}

#[test]
fn golden_fanout() {
    snap("fanout", &example("fanout"));
}

#[test]
fn golden_sepia() {
    snap("sepia", &example("sepia"));
}

/// countdown (§9 non-example regression, golden h's surface): mut param carried,
/// `U = (i32, IoToken)` (token last), print before the guard, value-less ret exit
/// carrying the post-print snapshot token, `Output` to Ret.
const COUNTDOWN: &str = r#"fn countdown(mut n: i32) {
    loop {
        n -> print;
        (n > 0) -> {
            -true-> { n - 1 -> n; -> loop; }
            -false-> -> ret;
        }
    }
}

fn main() {
    5 -> countdown;
}
"#;

#[test]
fn golden_countdown() {
    snap("countdown", COUNTDOWN);
}

/// effectful call (§9 non-example regression): callee `(IoToken, i32) → IoToken`
/// (B absent), caller packs `(tok, 5)`, `tok := r` directly (no proj — §6.3).
const EFFECTFUL_CALL: &str = r#"fn log(x: i32) {
    x -> print;
}

fn main() {
    5 -> log;
}
"#;

#[test]
fn golden_effectful_call() {
    snap("effectful_call", EFFECTFUL_CALL);
}
