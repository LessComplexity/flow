//! The rejection matrix (DESIGN §14.4): ≥1 test per L-code of §4 except L1901
//! (only a lower bug can raise it). Each program is parse-clean — lower owns the
//! rejection. Includes the named gotcha regressions from §14.4.

mod common;
use common::{assert_rejects, lower_err_codes, lower_ok};

// --- L1000–L1010: top-level structure ---------------------------------------

#[test]
fn l1001_no_main() {
    assert_rejects("fn f(x: i32) -> i32 { x -> ret; }\n", "L1001");
}

#[test]
fn l1002_main_shape() {
    // main with a param.
    assert_rejects("fn main(x: i32) { x -> print; }\n", "L1002");
}

#[test]
fn l1003_duplicate_fn() {
    assert_rejects("fn f() {}\nfn f() {}\nfn main() {}\n", "L1003");
}

#[test]
fn l1004_duplicate_type() {
    assert_rejects(
        "type P { x: i32 }\ntype P { y: i32 }\nfn main() {}\n",
        "L1004",
    );
}

#[test]
fn l1005_duplicate_field() {
    assert_rejects("type P { x: i32, x: i32 }\nfn main() {}\n", "L1005");
}

#[test]
fn l1006_duplicate_param() {
    assert_rejects("fn f(x: i32, x: i32) {}\nfn main() {}\n", "L1006");
}

#[test]
fn l1007_recursive_type() {
    assert_rejects("type A { b: B }\ntype B { a: A }\nfn main() {}\n", "L1007");
}

#[test]
fn l1008_recursive_call() {
    assert_rejects(
        "fn f() { -> g; }\nfn g() { -> f; }\nfn main() {}\n",
        "L1008",
    );
}

#[test]
fn l1009_reserved_name() {
    assert_rejects("fn print() {}\nfn main() {}\n", "L1009");
}

#[test]
fn l1010_empty_type() {
    assert_rejects("type Empty {}\nfn main() {}\n", "L1010");
}

// --- L1101–L1108: names / scope ---------------------------------------------

#[test]
fn l1101_unknown_name() {
    assert_rejects("fn main() { nope -> print; }\n", "L1101");
}

#[test]
fn l1102_unknown_type() {
    assert_rejects("fn f(x: Nope) {}\nfn main() {}\n", "L1102");
}

#[test]
fn l1103_unknown_field() {
    assert_rejects(
        "type P { x: i32 }\nfn f(p: P) -> i32 { p.y -> ret; }\nfn main() {}\n",
        "L1103",
    );
}

#[test]
fn l1104_assign_immutable() {
    assert_rejects(
        "fn f(x: i32) -> i32 { x + 1 -> x; x -> ret; }\nfn main() {}\n",
        "L1104",
    );
}

#[test]
fn l1105_function_as_value() {
    // a fn name in expression position.
    assert_rejects("fn g() {}\nfn main() { g + 1 -> print; }\n", "L1105");
}

#[test]
fn l1106_named_param_application() {
    assert_rejects(
        "fn add(a: i32, b: i32) -> i32 { a + b -> ret; }\nfn main() { 15 -> add.a; }\n",
        "L1106",
    );
}

#[test]
fn l1107_read_after_loop() {
    let src = r#"fn sum(n: i32) -> i32 {
    mut i: i32 <- 0;
    mut acc: i32 <- 0;
    loop {
        (i < n) -> {
            -true-> { acc + i -> acc; i + 1 -> i; -> loop; }
            -false-> acc -> ret;
        }
    }
    acc -> ret;
}
fn main() {}
"#;
    assert_rejects(src, "L1107");
}

#[test]
fn l1108_capture_in_body() {
    let src = r#"fn main() {
    mut k: i32 <- 5;
    [1, 2, 3] -> map { x -> x + k } -> ys: [i32; 3];
}
"#;
    assert_rejects(src, "L1108");
}

// --- L1201–L1209: types / literals ------------------------------------------

#[test]
fn l1201_type_mismatch() {
    // adding an i32 and a bool.
    assert_rejects(
        "fn f(x: i32, b: bool) -> i32 { x + b -> ret; }\nfn main() {}\n",
        "L1201",
    );
}

#[test]
fn l1202_literal_out_of_range() {
    // 300 does not fit u8.
    assert_rejects("fn f() -> u8 { 300 -> ret; }\nfn main() {}\n", "L1202");
}

#[test]
fn l1203_literal_type_conflict() {
    // an int literal unified with an f32 annotation.
    assert_rejects("fn f() -> f32 { 5 -> ret; }\nfn main() {}\n", "L1203");
}

#[test]
fn l1204_not_a_product() {
    // `.0` index on a scalar.
    assert_rejects(
        "fn f(x: i32) -> i32 { x.0 -> ret; }\nfn main() {}\n",
        "L1204",
    );
}

#[test]
fn l1205_slot_out_of_range() {
    // ret.5 on a 2-tuple output. (Also covers the u64→u32 lesson via OOB.)
    assert_rejects(
        "fn f() -> (i32, i32) { 1 -> ret.0; 2 -> ret.5; }\nfn main() {}\n",
        "L1205",
    );
}

#[test]
fn l1206_str_outside_print() {
    // a string literal returned (not feeding print).
    assert_rejects("fn f() -> i32 { \"hi\" -> ret; }\nfn main() {}\n", "L1206");
}

#[test]
fn l1207_unprintable() {
    // printing a tuple.
    assert_rejects("fn main() { (1, 2) -> print; }\n", "L1207");
}

#[test]
fn l1208_empty_array() {
    assert_rejects("fn f(a: [i32; 0]) {}\nfn main() {}\n", "L1208");
}

#[test]
fn l1209_type_too_deep() {
    // 65 levels of array nesting (parser guards at 128; 65 is lower's job).
    let mut ty = String::from("i32");
    for _ in 0..65 {
        ty = format!("[{ty}; 1]");
    }
    let src = format!("fn f(x: {ty}) {{}}\nfn main() {{}}\n");
    assert_rejects(&src, "L1209");
}

// --- L1301–L1307: chains / returns ------------------------------------------

#[test]
fn l1301_headless_chain() {
    // a statement-level headless op chain that is not `-> ret;`.
    assert_rejects("fn main() { -> print; }\n", "L1301");
}

#[test]
fn l1302_expr_stage() {
    // a general expression stage that does not consume the wire.
    assert_rejects(
        "fn f(x: i32, y: i32) -> i32 { x -> y + 1 -> ret; }\nfn main() {}\n",
        "L1302",
    );
}

#[test]
fn l1303_ret_mid_chain() {
    assert_rejects(
        "fn f(x: i32) -> i32 { x -> ret -> ret; }\nfn main() {}\n",
        "L1303",
    );
}

#[test]
fn l1304_jump_misplaced() {
    // `-> loop` outside a loop (Unit-output fn, so L1306 does not fire first).
    assert_rejects("fn f(x: i32) { x -> loop; }\nfn main() {}\n", "L1304");
}

#[test]
fn l1305_fanout_no_value() {
    // a fanout whose branch produces no value, then the chain continues.
    let src = r#"fn g(x: i32) { x -> print; }
fn sq(x: i32) -> i32 { x * x -> ret; }
fn main() {
    6 -> {
        -> sq -> a;
        -> g;
    } -> r;
    r -> print;
}
"#;
    assert_rejects(src, "L1305");
}

#[test]
fn l1306_incomplete_return() {
    // non-Unit output with no return write.
    assert_rejects("fn f(x: i32) -> i32 { x -> y; }\nfn main() {}\n", "L1306");
}

#[test]
fn l1306_mixed_bare_and_slot() {
    assert_rejects(
        "fn f() -> (i32, i32) { 1 -> ret; 2 -> ret.1; }\nfn main() {}\n",
        "L1306",
    );
}

#[test]
fn l1306_duplicate_slot() {
    assert_rejects(
        "fn f() -> (i32, i32) { 1 -> ret.0; 2 -> ret.0; }\nfn main() {}\n",
        "L1306",
    );
}

#[test]
fn l1307_effectful_return_shape() {
    // a `ret.k` write in an effectful fn.
    assert_rejects(
        "fn f(x: i32) -> (i32, i32) { x -> print; x -> ret.0; x -> ret.1; }\nfn main() {}\n",
        "L1307",
    );
}

// --- L1401–L1409: guards -----------------------------------------------------

#[test]
fn l1401_guard_arm_missing() {
    // a bool guard with only one pole, no default.
    assert_rejects(
        "fn f(b: bool, x: i32) -> i32 { b -> { -true-> x; } -> ret; }\nfn main() {}\n",
        "L1401",
    );
}

#[test]
fn l1402_guard_arm_duplicate() {
    assert_rejects(
        "fn f(k: i32) -> i32 { k -> { -0-> 1; -0-> 2; -_-> 3; } -> ret; }\nfn main() {}\n",
        "L1402",
    );
}

#[test]
fn l1403_guard_arm_mixed() {
    assert_rejects(
        "fn f(b: bool) -> i32 { b -> { -true-> 1; -0-> 2; } -> ret; }\nfn main() {}\n",
        "L1403",
    );
}

#[test]
fn l1404_guard_arm_effectful() {
    // print inside a Phi-position arm.
    assert_rejects(
        "fn f(b: bool, x: i32) -> i32 { b -> { -true-> { x -> print; x } -false-> x; } -> ret; }\nfn main() {}\n",
        "L1404",
    );
}

#[test]
fn l1405_guard_ret_in_phi_arm() {
    assert_rejects(
        "fn f(b: bool, x: i32) -> i32 { b -> { -true-> { x -> ret; x } -false-> x; } -> ret; }\nfn main() {}\n",
        "L1405",
    );
}

#[test]
fn l1406_guard_scrutinee_type() {
    // bool poles on a non-bool scrutinee.
    assert_rejects(
        "fn f(x: i32) -> i32 { x -> { -true-> 1; -false-> 2; } -> ret; }\nfn main() {}\n",
        "L1406",
    );
}

#[test]
fn l1407_routing_guard_shape() {
    // a routing guard that is not the loop body's final item.
    let src = r#"fn f(n: i32) -> i32 {
    mut i: i32 <- 0;
    loop {
        (i < n) -> {
            -true-> { i + 1 -> i; -> loop; }
            -false-> i -> ret;
        }
        i -> print;
    }
}
fn main() {}
"#;
    assert_rejects(src, "L1407");
}

#[test]
fn l1408_assign_in_phi_arm() {
    // assignment to an enclosing mut inside a Phi-position arm.
    let src = r#"fn f(b: bool) -> i32 {
    mut x: i32 <- 0;
    b -> { -true-> { 5 -> x; x } -false-> x; } -> ret;
}
fn main() {}
"#;
    assert_rejects(src, "L1408");
}

#[test]
fn l1409_routing_guard_arms() {
    // a three-arm routing guard (integer discriminants in a routing guard).
    let src = r#"fn f(n: i32) -> i32 {
    mut i: i32 <- 0;
    loop {
        i -> {
            -0-> { i + 1 -> i; -> loop; }
            -1-> { i + 2 -> i; -> loop; }
            -_-> i -> ret;
        }
    }
}
fn main() {}
"#;
    assert_rejects(src, "L1409");
}

// --- L1501–L1504: loops ------------------------------------------------------

#[test]
fn l1501_loop_no_exit() {
    // a loop body with no routing guard at all (a plain Phi guard only).
    let src = r#"fn f() {
    mut i: i32 <- 0;
    loop {
        (i < 10) -> { -true-> i + 1; -false-> i; } -> i;
    }
}
fn main() {}
"#;
    assert_rejects(src, "L1501");
}

#[test]
fn l1502_loop_no_state() {
    // a loop assigning no enclosing mut.
    let src = r#"fn f() {
    loop {
        (1 < 2) -> {
            -true-> -> loop;
            -false-> -> ret;
        }
    }
}
fn main() {}
"#;
    assert_rejects(src, "L1502");
}

#[test]
fn l1503_non_derived_cond() {
    // the guard cond does not derive from the loop state (`5 -> acc; (acc > 0)`).
    let src = r#"fn f() {
    mut acc: i32 <- 0;
    loop {
        5 -> acc;
        (acc > 0) -> {
            -true-> { acc + 1 -> acc; -> loop; }
            -false-> acc -> ret;
        }
    }
}
fn main() {}
"#;
    assert_rejects(src, "L1503");
}

#[test]
fn l1503_non_derived_next_state() {
    // the back-edge next-state is merge-independent (`5 -> acc; -> loop`).
    let src = r#"fn f(n: i32) -> i32 {
    mut acc: i32 <- 0;
    mut i: i32 <- 0;
    loop {
        (i < n) -> {
            -true-> { 5 -> acc; 5 -> i; -> loop; }
            -false-> acc -> ret;
        }
    }
}
fn main() {}
"#;
    assert_rejects(src, "L1503");
}

#[test]
fn l1504_nested_loop_updates_outer() {
    // an inner loop assigning a mut also carried by the enclosing loop (`acc` is
    // assigned in the outer body AND the inner body).
    let src = r#"fn f(n: i32) -> i32 {
    mut i: i32 <- 0;
    mut acc: i32 <- 0;
    loop {
        acc + i -> acc;
        (i < n) -> {
            -true-> {
                loop {
                    (acc < 10) -> {
                        -true-> { acc + 1 -> acc; -> loop; }
                        -false-> acc -> ret;
                    }
                }
                i + 1 -> i;
                -> loop;
            }
            -false-> acc -> ret;
        }
    }
}
fn main() {}
"#;
    assert_rejects(src, "L1504");
}

#[test]
fn l1504_inner_exit_not_ret() {
    // a nested loop whose inner exit does not terminate in `-> ret`.
    let src = r#"fn f(n: i32) -> i32 {
    mut i: i32 <- 0;
    mut total: i32 <- 0;
    loop {
        (i < n) -> {
            -true-> {
                mut j: i32 <- 0;
                loop {
                    (j < 3) -> {
                        -true-> { j + 1 -> j; -> loop; }
                        -false-> j -> total;
                    }
                }
                i + 1 -> i;
                -> loop;
            }
            -false-> total -> ret;
        }
    }
}
fn main() {}
"#;
    assert_rejects(src, "L1504");
}

#[test]
fn l1504_loop_in_effectful_loop() {
    // a nested loop inside a token-carrying loop.
    let src = r#"fn f(n: i32) {
    mut i: i32 <- 0;
    mut j: i32 <- 0;
    loop {
        i -> print;
        (i < n) -> {
            -true-> {
                loop {
                    (j < 3) -> {
                        -true-> { j + 1 -> j; -> loop; }
                        -false-> j -> ret;
                    }
                }
                i + 1 -> i;
                -> loop;
            }
            -false-> -> ret;
        }
    }
}
fn main() {}
"#;
    assert_rejects(src, "L1504");
}

// --- L1601–L1605: map / fold -------------------------------------------------

#[test]
fn l1601_block_arity() {
    // a map block with two params.
    assert_rejects(
        "fn main() { [1, 2] -> map { a, b -> a } -> ys: [i32; 2]; }\n",
        "L1601",
    );
}

#[test]
fn l1602_map_non_array() {
    // map applied to a non-array wire.
    assert_rejects("fn main() { 5 -> map { x -> x + 1 } -> ys; }\n", "L1602");
}

#[test]
fn l1603_fold_shape() {
    // fold wire not a 2-tuple `(init, array)`.
    assert_rejects(
        "fn main() { 5 -> fold { acc, x -> acc + x } -> r; }\n",
        "L1603",
    );
}

#[test]
fn l1604_body_no_value() {
    // a map body with no tail value.
    assert_rejects(
        "fn main() { [1, 2] -> map { x -> x + 1 -> y; } -> ys: [i32; 2]; }\n",
        "L1604",
    );
}

#[test]
fn l1605_body_effectful() {
    // a print inside a map body.
    assert_rejects(
        "fn main() { [1, 2] -> map { x -> x -> print; x } -> ys: [i32; 2]; }\n",
        "L1605",
    );
}

// --- named regression: the `0.0`-seed fold must lower CLEAN ------------------

#[test]
fn fold_zero_seed_lowers_clean() {
    // The §7.2 unification regression: the f32 seed `0.0` must resolve to f32
    // through `acc + px.r`, so the program lowers without error.
    let src = r#"type Pixel { r: f32, g: f32, b: f32 }
fn main() {
    [Pixel { r: 1.0, g: 2.0, b: 3.0 }] -> img: [Pixel; 1];
    (0.0, img) -> fold { acc, px -> acc + px.r } -> total;
    total -> print;
}
"#;
    let po = flow_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
    let ir = flow_lower::lower(src, &po.program).expect("0.0-seed fold must lower clean");
    assert!(flow_ir::validate(&ir).is_empty());
}

// --- named regression: single-branch fanout passes through unpacked ---------

#[test]
fn single_branch_fanout_passes_through() {
    // A single-branch fanout's join is the branch's tail value (no pack; FAN-1).
    let src = r#"fn sq(x: i32) -> i32 { x * x -> ret; }
fn main() {
    6 -> { -> sq -> a; } -> r;
    r -> print;
}
"#;
    let po = flow_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
    let ir = flow_lower::lower(src, &po.program).expect("single-branch fanout lowers");
    assert!(flow_ir::validate(&ir).is_empty());
}

// ===========================================================================
//  Review-finding regressions (named after the finding id)
// ===========================================================================

#[test]
fn lower_1_retk_above_u32_max_rejected() {
    // LOWER-1 / LOWER-RETK-TRUNC: a `ret.k` slot > u32::MAX must be rejected with
    // L1205 (the Session-04 truncation lesson), not silently truncated to slot 0.
    assert_rejects(
        "fn f() -> (i32, i32) { 1 -> ret.1; 2 -> ret.4294967296; }\nfn main() {}\n",
        "L1205",
    );
}

#[test]
fn lower_retk_trunc_noncolliding_rejected() {
    // LOWER-RETK-TRUNC: the worst case — ret.0 + ret.(2^32+1) — used to lower
    // entirely clean (truncated to slot 1); it must now be L1205.
    assert_rejects(
        "fn f() -> (i32, i32) { 10 -> ret.0; 20 -> ret.4294967297; }\nfn main() {}\n",
        "L1205",
    );
}

#[test]
fn lower_memberidx_typing_trunc_rejected() {
    // LOWER-MEMBERIDX-TYPING-TRUNC: a member index > u32::MAX is rejected with
    // L1205 by emission; the typing pass no longer transiently widens against the
    // truncated component.
    assert_rejects(
        "fn f(x: (i32, i32)) -> i32 { x.4294967296 -> ret; }\nfn main() {}\n",
        "L1205",
    );
}

#[test]
fn lower_2_value_producing_effectful_ret_lowers_clean() {
    // LOWER-2: the §6.2 `yes | present | present` row — an effectful fn with a
    // declared return B and a value-producing `<expr> -> ret`. Lowers clean as a
    // `pack(tok, value) -> Dest::Ret { slot: None }` full-tuple writer (LD18).
    let src = "fn f(x: i32) -> i32 { x -> print; x + 1 -> ret; }\nfn main() { 3 -> f -> r; r -> print; }\n";
    let ir = lower_ok(src);
    assert!(flow_ir::validate(&ir).is_empty());
}

#[test]
fn lower_2_value_producing_effectful_fn_body_tail_lowers_clean() {
    // LOWER-2 case 2: the fn-body virtual-ret tail in an effectful B-present fn.
    let src = "fn f(x: i32) -> i32 { x -> print; x + 1 }\nfn main() { 3 -> f -> r; r -> print; }\n";
    let ir = lower_ok(src);
    assert!(flow_ir::validate(&ir).is_empty());
}

#[test]
fn atk_03_effectful_call_in_phi_arm_rejected() {
    // ATK-03: an effectful user-defined call in a Phi-position arm must be L1404
    // (not silently accepted, firing the effect unconditionally on both paths).
    let src = r#"fn log(x: i32) { x -> print; }
fn main() {
    (1 > 0) -> {
        -true-> { 5 -> log; 9 }
        -false-> 7;
    } -> v;
    "done" -> print;
}
"#;
    assert_rejects(src, "L1404");
}

#[test]
fn atk_04_bool_guard_result_ty_recorded() {
    // ATK-04: a bool-guard Phi result must have its ty recorded (and merge tag
    // propagated). Printing it (was L1207), using it as a routing-guard cond (was
    // L1406), and a jump next-state through it (was L1503) must all lower clean.
    // (a) print the bool-guard result.
    let a = "fn main() { (1 > 0) -> { -true-> 1; -false-> 0; } -> y; y -> print; }\n";
    let ir = lower_ok(a);
    assert!(flow_ir::validate(&ir).is_empty());
    // (b) routing-guard cond through a Phi.
    let b = r#"fn f(n: i32) -> i32 {
    mut i: i32 <- 0;
    loop {
        (i < n) -> { -true-> true; -false-> false; } -> {
            -true-> { i + 1 -> i; -> loop; }
            -false-> i -> ret;
        }
    }
}
fn main() { 5 -> f -> r; r -> print; }
"#;
    let irb = lower_ok(b);
    assert!(flow_ir::validate(&irb).is_empty());
}

#[test]
fn atk_08_effect_after_consumed_token_is_l1307() {
    // ATK-08: a print after a ret-write in an effectful fn (token consumed) must
    // be L1307, not the internal L1901 "print in pure fn".
    let src = "fn f(x: i32) -> i32 { x -> print; x -> ret; 5 -> print; }\nfn main() { 3 -> f -> r; r -> print; }\n";
    assert_rejects(src, "L1307");
    // And the ret-terminal loop-exit variant.
    let loop_src = r#"fn main() {
    mut i: i32 <- 3;
    loop {
        i -> print;
        (i > 0) -> {
            -true-> { i - 1 -> i; -> loop; }
            -false-> -> ret;
        }
    }
    5 -> print;
}
"#;
    assert_rejects(loop_src, "L1307");
}

#[test]
fn atk_15_bool_guard_arm_bookkeeping() {
    // ATK-15(a): a default-only bool guard — both poles missing → L1401.
    assert_rejects(
        "fn f(x: bool) -> i32 { x -> { -_-> 5; } -> ret; }\nfn main() {}\n",
        "L1401",
    );
    // ATK-15(b): duplicate `-_->` default arms → L1402.
    assert_rejects(
        "fn f(x: bool) -> i32 { x -> { -true-> 1; -_-> 2; -_-> 3; } -> ret; }\nfn main() {}\n",
        "L1402",
    );
    // ATK-15(c): a default coexisting with both concrete poles (unreachable) → L1402.
    assert_rejects(
        "fn f(x: bool) -> i32 { x -> { -true-> 1; -false-> 2; -_-> 3; } -> ret; }\nfn main() {}\n",
        "L1402",
    );
}

#[test]
fn atk_16_second_effectful_ret_is_l1307() {
    // ATK-16: a second surface ret-write in an effectful fn must be L1307 (each
    // write consumes the final token), not L1201.
    let src = "fn f(x: i32) -> i32 { x -> print; x -> ret; x -> ret; }\nfn main() { 3 -> f -> r; r -> print; }\n";
    let codes = lower_err_codes(src);
    assert!(codes.iter().any(|c| c == "L1307"), "got {codes:?}");
    assert!(!codes.iter().any(|c| c == "L1201"), "got {codes:?}");
}

#[test]
fn atk_17_unit_effectful_fanout_tail_lowers_clean() {
    // ATK-17: a fanout as the fn-body tail of a Unit-output effectful fn must
    // lower clean (the tail is dead code as a value; LD21) — like the `;`-form.
    let tail = "fn main() { 1 -> { -> print; -> print; } }\n";
    let ir = lower_ok(tail);
    assert!(flow_ir::validate(&ir).is_empty());
    let stmt = "fn main() { 1 -> { -> print; -> print; }; }\n";
    let ir2 = lower_ok(stmt);
    assert!(flow_ir::validate(&ir2).is_empty());
}

#[test]
fn atk_18_zero_param_call_is_l1201() {
    // ATK-18: calling a zero-param fn with an argument must be an L1201-family
    // arg-vs-input mismatch, not the nonsensical L1203 "int literal cannot have
    // type `()`".
    let src = "fn g() -> i32 { 5 -> ret; }\nfn main() { 7 -> g -> x; x -> print; }\n";
    let codes = lower_err_codes(src);
    assert!(codes.iter().any(|c| c == "L1201"), "got {codes:?}");
    assert!(!codes.iter().any(|c| c == "L1203"), "got {codes:?}");
}

#[test]
fn atk_19_headless_bind_exit_arm_seeds_scrutinee() {
    // ATK-19: a headless `-> name` binding as a routing-guard exit arm must seed
    // `cur := scrutinee` (§8.3), not misreport (was L1305/L1301). The exit arm
    // `-> c` binds the guard's incoming bool cond, which is then returned.
    let src = r#"fn f(n: i32) -> bool {
    mut i: i32 <- 0;
    loop {
        (i < n) -> {
            -true-> { i + 1 -> i; -> loop; }
            -false-> -> c;
        }
    }
    c -> ret;
}
fn main() { 5 -> f -> r; r -> print; }
"#;
    let ir = lower_ok(src);
    assert!(flow_ir::validate(&ir).is_empty());
}

#[test]
fn atk_05_loop_exit_binding_visible_after_loop() {
    // ATK-05: an exit-arm Bind terminal (`-false-> i -> total`) must bind in the
    // loop's ENCLOSING scope (§8.5 step 4), so the post-loop `total -> ret` reads
    // it (was a false L1101 when bound into the popped loop-body frame).
    let src = r#"fn f(n: i32) -> i32 {
    mut i: i32 <- 0;
    loop {
        (i < n) -> {
            -true-> { i + 1 -> i; -> loop; }
            -false-> i -> total;
        }
    }
    total -> ret;
}
fn main() { 5 -> f -> r; r -> print; }
"#;
    let ir = lower_ok(src);
    assert!(flow_ir::validate(&ir).is_empty());
}

#[test]
fn atk_07_pure_var_tail_returns() {
    // ATK-07(a): a pure fn-body tail that is a bare VARIABLE (`{ x }`) must route
    // through `output()`/`emit_ret_existing` (LD21 + §8.1: bare pre-existing →
    // Output), not pass `Dest::Ret` to the `Var` arm (which ignored it → seal
    // `RetSlotMissing` / L1901).
    let a = "fn f(x: i32) -> i32 { x }\nfn main() { 1 -> f -> r; r -> print; }\n";
    let ir = lower_ok(a);
    assert!(flow_ir::validate(&ir).is_empty());
    // A bare literal tail returns the same way (also a pre-existing wire → Output).
    let lit = "fn g(x: i32) -> i32 { 5 }\nfn main() { 1 -> g -> r; r -> print; }\n";
    let irl = lower_ok(lit);
    assert!(flow_ir::validate(&irl).is_empty());
}

#[test]
fn atk_07_effectful_b_tail_returns() {
    // ATK-07(b): an effectful-with-B fn-body tail (`{ x -> print; x + 1 }`) must
    // lower clean (the `pack(tok, value) -> Dest::Ret` full-tuple writer; LD18).
    let src = "fn f(x: i32) -> i32 { x -> print; x + 1 }\nfn main() { 3 -> f -> r; r -> print; }\n";
    let ir = lower_ok(src);
    assert!(flow_ir::validate(&ir).is_empty());
}

#[test]
fn atk_09_valueless_ret_exit_in_unit_fn_lowers_clean() {
    // ATK-09: a value-less `-> ret` loop-exit in a pure Unit-output fn must lower
    // clean (was a false L1306). §8.5/LP-1: the payload is the merge-state view
    // (the merge object), exiting to `Dest::Fresh(None)` with zero Return writers.
    let src = r#"fn f(n: i32) {
    mut i: i32 <- 0;
    loop {
        (i < n) -> {
            -true-> { i + 1 -> i; -> loop; }
            -false-> -> ret;
        }
    }
}
fn main() { 3 -> f; "done" -> print; }
"#;
    let ir = lower_ok(src);
    assert!(flow_ir::validate(&ir).is_empty());
}

#[test]
fn atk_10_statement_level_bare_ret_is_permitted() {
    // ATK-10: a statement-level bare `-> ret;` is in the catalogue (§8.3), not a
    // false L1301. (a) pure Unit fn: a no-op.
    let a = "fn f(x: i32) { -> ret; }\nfn main() {}\n";
    let ir = lower_ok(a);
    assert!(flow_ir::validate(&ir).is_empty());
    // (b) effectful B-absent fn: writes the current token to Return.
    let b = "fn main() { \"a\" -> print; -> ret; }\n";
    let irb = lower_ok(b);
    assert!(flow_ir::validate(&irb).is_empty());
}

#[test]
fn atk_10_bare_ret_then_effect_is_l1307() {
    // ATK-10 corollary: after a statement-level `-> ret;` consumes the token in an
    // effectful fn, a later effect finds the register consumed → L1307.
    assert_rejects(
        "fn main() { \"a\" -> print; -> ret; \"b\" -> print; }\n",
        "L1307",
    );
}

#[test]
fn atk_11_semicolon_terminated_exit_block_lowers_clean() {
    // ATK-11: an exit arm that is a Block with `tail: None` (a `;`-terminated
    // terminal) must be handled like the same arm without `;` (LD21:
    // statement-vs-tail identical) — the last chain item is peeled as the
    // terminal. (a) `-false-> { -> ret; }`.
    let a = r#"fn f(n: i32) {
    mut i: i32 <- 0;
    loop {
        (i < n) -> {
            -true-> { i + 1 -> i; -> loop; }
            -false-> { -> ret; }
        }
    }
}
fn main() { 3 -> f; "done" -> print; }
"#;
    let ir = lower_ok(a);
    assert!(flow_ir::validate(&ir).is_empty());
    // (b) `-false-> { 99 -> print; -> ret; }` (preceding statement + ret terminal).
    let b = r#"fn f(n: i32) {
    mut i: i32 <- 0;
    loop {
        (i < n) -> {
            -true-> { i + 1 -> i; -> loop; }
            -false-> { 99 -> print; -> ret; }
        }
    }
}
fn main() { 3 -> f; "done" -> print; }
"#;
    let irb = lower_ok(b);
    assert!(flow_ir::validate(&irb).is_empty());
}

#[test]
fn atk_12_pure_call_result_derives_from_merge() {
    // ATK-12: a pure-call result must carry the derives-from-merge tag (§7.3: any
    // derived operand ⇒ derived result), so a guard cond flowing through a pure
    // call passes the L1503 derivation test (was a false L1503).
    let src = r#"fn isneg(x: i32) -> bool { (x < 0) -> ret; }
fn f(n: i32) -> i32 {
    mut i: i32 <- 0;
    loop {
        i - n -> isneg -> c;
        c -> {
            -true-> { i + 1 -> i; -> loop; }
            -false-> i -> ret;
        }
    }
}
fn main() { 5 -> f -> r; r -> print; }
"#;
    let ir = lower_ok(src);
    assert!(flow_ir::validate(&ir).is_empty());
}

#[test]
fn atk_13_headless_routing_guard_is_l1301() {
    // ATK-13: a headless routing guard (`-> { -true-> -> loop; -false-> … }`) has
    // no scrutinee — a user error reported as L1301 HeadlessChain, not the
    // internal L1901 (which must stay unreachable from parse-clean input).
    let src = r#"fn f(n: i32) -> i32 {
    mut i: i32 <- 0;
    loop {
        i + 1 -> i;
        -> {
            -true-> -> loop;
            -false-> i -> ret;
        };
    }
}
fn main() { 5 -> f -> r; r -> print; }
"#;
    let codes = lower_err_codes(src);
    assert!(codes.iter().any(|c| c == "L1301"), "got {codes:?}");
    assert!(!codes.iter().any(|c| c == "L1901"), "got {codes:?}");
}

#[test]
fn atk_14_loop_in_map_body_lowers_clean() {
    // ATK-14: the map/fold-body capture checker must descend into loop bodies,
    // registering body-local loop binds AND exit-arm Bind names as body-locals
    // (was a false L1108). The loop inside the map body is Core-legal (bodies are
    // ordinary token-free fns) and lowers clean.
    let src = r#"fn main() {
    [1, 2, 3] -> map {
        x ->
        mut i: i32 <- 0;
        loop {
            (i < x) -> {
                -true-> { i + 1 -> i; -> loop; }
                -false-> i -> v;
            }
        }
        v
    } -> brr: [i32; 3];
    brr[0] -> print;
}
"#;
    let ir = lower_ok(src);
    assert!(flow_ir::validate(&ir).is_empty());
}
