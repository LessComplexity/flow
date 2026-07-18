//! Golden Mermaid snapshots (DESIGN §14.1): the eight acceptance programs plus the
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

/// zip_demo (ADR-0018 example): full showcase file — `zip` add + `enumerate` add.
#[test]
fn golden_zip_demo() {
    snap("zip_demo", &example("zip_demo"));
}

/// vector_add (ADR-0018 example): `zip` add + fold sum.
#[test]
fn golden_vector_add() {
    snap("vector_add", &example("vector_add"));
}

/// seq_demo (ADR-0019 example): pure fanout → join → `seq { … }` ordered prints.
/// `seq` has no IR footprint (pin d) — the snapshot shows only the two `Println`
/// primitives, ordered by the IoToken thread.
#[test]
fn golden_seq_demo() {
    snap("seq_demo", &example("seq_demo"));
}

/// countdown (§9 non-example regression, golden h's surface): mut param carried,
/// `U = (i32, IoToken)` (token last), `println` before the guard (ADR-0015),
/// value-less ret exit carrying the post-print snapshot token, `Output` to Ret.
const COUNTDOWN: &str = r#"fn countdown(mut n: i32) {
    loop {
        n -> println;
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

/// zip builtin (ADR-0018 / WP2): `(a, b) -> zip -> map { p -> p.0 + p.1 }`. The
/// tuple wire is projected into two arrays, re-paired, then `Zip`; the map body
/// consumes the `(i32, i32)` element. Mirrors the zip_demo showcase form without
/// touching the example file.
const ZIP_BUILTIN: &str = r#"fn main() {
    [0, 1, 2, 3] -> a: [i32; 4];
    [100, 101, 102, 103] -> b: [i32; 4];
    (a, b) -> zip -> map { p -> p.0 + p.1 } -> c: [i32; 4];
    c[0] -> println;
}
"#;

#[test]
fn golden_zip_builtin() {
    snap("zip_builtin", ZIP_BUILTIN);
}

/// enumerate builtin (ADR-0018 / WP2): `a -> enumerate -> map { p -> p.0 + p.1 }`.
/// Single-source `Enumerate` (no internal pair); element `(i32, A)`.
const ENUMERATE_BUILTIN: &str = r#"fn main() {
    [10, 20, 30] -> a: [i32; 3];
    a -> enumerate -> map { p -> p.0 + p.1 } -> c: [i32; 3];
    c[0] -> println;
}
"#;

#[test]
fn golden_enumerate_builtin() {
    snap("enumerate_builtin", ENUMERATE_BUILTIN);
}

/// Fanout legality (ADR-0018): `zip`/`enumerate` are PURE (no token), so they are
/// legal inside a parallel fanout — unlike `print`/effectful calls (E2). Both
/// branches lower and validate clean.
const FANOUT_PURE_OPS: &str = r#"fn main() {
    [10, 20, 30] -> arr: [i32; 3];
    arr -> {
        -> enumerate -> map { p -> p.0 + p.1 } -> e;
        -> map { x -> x * 2 } -> d;
    }
    e[0] -> println;
    d[0] -> println;
}
"#;

#[test]
fn fanout_pure_collection_ops_lower_clean() {
    // lower_ok asserts seal-clean, validate-empty, and lint-clean (DESIGN §14).
    let _ = lower_ok(FANOUT_PURE_OPS);
}

// --- seq statement block (ADR-0019 / WP2) -----------------------------------

/// Statement-form `seq` (ADR-0019 pin d): two ordered `println`s. `seq` has NO
/// IR footprint — the snapshot shows only the two `Println` primitives, ordered
/// solely by the IoToken thread (io → io → io → Output). This IS the ordering
/// guarantee; there is no `seq` node.
const SEQ_TWO_PRINTLNS: &str = r#"fn main() {
    42 -> seq {
        "first" -> println;
        "second" -> println;
    }
}
"#;

#[test]
fn golden_seq_two_printlns() {
    snap("seq_two_printlns", SEQ_TWO_PRINTLNS);
}

/// `seq` mid-chain with a tail (ADR-0019 pin c): `data -> seq { … tail } -> f`.
/// The headless statement `-> g -> a` seeds from the seq input (pin a); the
/// seq's value is its tail chain's value (`a`), which feeds the following `-> g`.
const SEQ_MID_CHAIN: &str = r#"fn g(x: i32) -> i32 { x -> ret; }
fn main() {
    5 -> seq { -> g -> a; a } -> g -> r;
    r -> println;
}
"#;

#[test]
fn golden_seq_mid_chain() {
    snap("seq_mid_chain", SEQ_MID_CHAIN);
}

/// `seq` in return position with a tail (ADR-0019 pin c): the tail value is the
/// fn's return. Bindings inside the seq (`a`) live in the enclosing scope
/// (pin b), and the tail `a` is written to `Output`/Ret.
const SEQ_RETURN_TAIL: &str = r#"fn f(x: i32) -> i32 {
    x -> seq { x * 2 -> a; a }
}
fn main() {}
"#;

#[test]
fn golden_seq_return_tail() {
    snap("seq_return_tail", SEQ_RETURN_TAIL);
}

/// `seq` stage immediately followed by an explicit `-> ret` marker in the same
/// chain (WP2 fixer regression, finding F7). `SeqBlock` is deliberately NOT a
/// `stage_writes_value`, so the seq value comes back **bare** and the following
/// `-> ret` routes it through `emit_ret_existing` (DESIGN §8.10) — exactly like a
/// pre-existing wire, never a double-write to Return. This snapshot pins that path.
const SEQ_EXPLICIT_RET: &str = r#"fn f(x: i32) -> i32 {
    x -> seq { x * 2 -> a; a } -> ret;
}
fn main() {}
"#;

#[test]
fn golden_seq_explicit_ret() {
    snap("seq_explicit_ret", SEQ_EXPLICIT_RET);
}

// --- array element update (ADR-0021) ----------------------------------------

/// Straight-line `c[i] <- v` (ADR-0021 §2 wiring a): the indexed bind is a
/// rebind emitting one `Update` (an internal `(arr, idx, val)` 3-tuple → the
/// pure `Update` op), then reads `c[0]`. `main` is effectful (`println`) — the
/// snapshot pins that the `Update` op takes NO token in-edge (pure), while the
/// IoToken thread runs only through the `println`.
const ARRAY_UPDATE_STRAIGHTLINE: &str = r#"fn main() {
    mut c: [i32; 4] <- [0, 0, 0, 0];
    c[2] <- 9;
    c[0] -> println;
}
"#;

#[test]
fn golden_array_update_straightline() {
    snap("array_update_straightline", ARRAY_UPDATE_STRAIGHTLINE);
}
