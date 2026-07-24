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
fn l1009_reserved_collection_builtins() {
    // ADR-0018: `zip`/`enumerate` are builtins resolved by name, so a user `fn`
    // of either name collides exactly like `print` — L1009.
    assert_rejects("fn zip() {}\nfn main() {}\n", "L1009");
    assert_rejects("fn enumerate() {}\nfn main() {}\n", "L1009");
    assert_rejects("fn widen_i64() {}\nfn main() {}\n", "L1009");
    assert_rejects("fn widen_f32() {}\nfn main() {}\n", "L1009");
    assert_rejects("fn widen_f64() {}\nfn main() {}\n", "L1009");
}

#[test]
fn l1009_reserved_time() {
    // plan-time-builtin: `time` is a reserved stage name like `print`/`iota`,
    // so a user fn of that name collides — L1009.
    assert_rejects("fn time() {}\nfn main() {}\n", "L1009");
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
fn l1107_capture_of_loop_carried_name_after_loop() {
    // ADR-0027 review blocker #2: a poisoned (loop-carried) name read as a
    // map-body capture after its loop must report L1107 like the direct read
    // — the capture resolution must not discard the poison bit (before the
    // fix it lowered, and the interp panicked).
    let src = r#"fn main() {
    mut acc: i32 <- 0;
    mut i: i32 <- 0;
    loop {
        (i < 3) -> {
            -true-> { acc + i -> acc; i + 1 -> i; -> loop; }
            -false-> acc -> done;
        }
    }
    [1, 2] -> map { x -> x + acc } -> ys: [i32; 2];
}
"#;
    assert_rejects(src, "L1107");
}

// (The pre-ADR-0027 `l1108_capture_in_body` row moved to tests/captures.rs as
// the positive pin `capture_map_lowers_with_capture_input_product`: reads of
// enclosing bindings are legal captures now — D1.)

#[test]
fn l1108_rebind_of_read_capture_names_the_variable() {
    // ADR-0027 review major #4 (D2b/D3): a chain rebind `-> k` of a name the
    // body has already captured (read) must report the TEACHING L1108 naming
    // `k` — not fall through to emit's generic L1104 ("not `mut`").
    let src = r#"fn main() {
    5 -> k;
    [1, 2] -> map { e -> e + k -> d; e + 1 -> k; d + k } -> ys: [i32; 2];
}
"#;
    let po = flow_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "{:?}", po.diagnostics);
    let ds = match flow_lower::lower(src, &po.program) {
        Ok(_) => panic!("must reject the rebind of a read capture"),
        Err(ds) => ds,
    };
    let d = ds
        .iter()
        .find(|d| d.code.0 == "L1108")
        .unwrap_or_else(|| panic!("expected L1108, got {ds:?}"));
    assert!(d.message.contains("`k`"), "names the variable: {d:?}");
    assert!(
        !ds.iter().any(|d| d.code.0 == "L1104"),
        "not the generic L1104: {ds:?}"
    );
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
fn l1301_unit_as_value() {
    // plan-time-builtin: `()` produces no object — its only use is the
    // wire-LESS head of `() -> time`. In a value position (an operand, a
    // block tail) it is L1301, and the message teaches the one legal use.
    assert_rejects("fn main() { () + 1 -> println; }\n", "L1301");
    let src = "fn f() -> i32 { () }\nfn main() {}\n";
    assert_rejects(src, "L1301");
    let po = flow_syntax::parse(src);
    let ds = flow_lower::lower(src, &po.program).expect_err("`()` tail is not a value");
    let d = ds
        .iter()
        .find(|d| d.code.0 == "L1301")
        .unwrap_or_else(|| panic!("expected L1301, got {ds:?}"));
    assert!(
        d.message.contains("() -> time"),
        "the Unit arm's message names the one legal use: {d:?}"
    );
}

#[test]
fn l1302_time_with_a_wire() {
    // plan-time-builtin: `time` is the one stage that takes no wire; feeding it
    // one is L1302 ("`time` takes no value: write `() -> time`").
    assert_rejects("fn main() { 5 -> time -> t; t -> println; }\n", "L1302");
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

#[test]
fn l1605_body_time() {
    // plan-time-builtin: `time` is an effect site exactly like `print`, so a
    // clock read inside a map body is the same L1605 (bodies are token-free).
    assert_rejects(
        "fn main() { [1, 2] -> map { x -> () -> time -> t; x } -> ys: [i32; 2]; }\n",
        "L1605",
    );
}

// --- L1606–L1610: zip / enumerate (ADR-0018) --------------------------------

#[test]
fn l1606_zip_non_tuple_scalar() {
    // zip source is a scalar, not a 2-tuple.
    assert_rejects("fn main() { 5 -> zip -> x; }\n", "L1606");
}

#[test]
fn l1606_zip_non_tuple_arity3() {
    // zip source is a 3-tuple, not a 2-tuple.
    assert_rejects("fn main() { (1, 2, 3) -> zip -> x; }\n", "L1606");
}

#[test]
fn l1607_zip_component_not_array() {
    // component 0 of the 2-tuple is a scalar, not an array.
    assert_rejects(
        "fn main() { [1, 2, 3] -> a: [i32; 3]; (5, a) -> zip -> x; }\n",
        "L1607",
    );
}

#[test]
fn l1608_zip_size_mismatch() {
    // the two arrays differ in length.
    assert_rejects(
        "fn main() { [1, 2] -> a: [i32; 2]; [3, 4, 5] -> b: [i32; 3]; (a, b) -> zip -> x; }\n",
        "L1608",
    );
}

#[test]
fn l1609_enumerate_non_array() {
    // enumerate applied to a scalar wire.
    assert_rejects("fn main() { 5 -> enumerate -> x; }\n", "L1609");
}

#[test]
fn l1610_enumerate_oversize() {
    // an array whose length exceeds i32::MAX cannot be enumerated (the index
    // `i32` could not name every element).
    assert_rejects(
        "fn f(a: [i32; 2147483648]) -> [(i32, i32); 2147483648] { a -> enumerate -> ret; }\nfn main() {}\n",
        "L1610",
    );
}

// --- L1611: seq statement block (ADR-0019 / WP2) ----------------------------

#[test]
fn l1611_seq_continues_no_tail() {
    // A seq whose chain continues (a following `-> g` stage) but that has no
    // tail value → L1611 (no more silent pack-of-tails; ADR-0019 pin c).
    assert_rejects(
        "fn g(x: i32) -> i32 { x -> ret; }\nfn main() { 5 -> seq { 3 -> a; } -> g -> r; r -> println; }\n",
        "L1611",
    );
}

#[test]
fn l1611_seq_return_position_no_tail() {
    // A seq in return position with no tail value → L1611 (the return demands a
    // value the tail-less seq cannot supply).
    assert_rejects(
        "fn f(x: i32) -> i32 { x -> seq { x -> a; } }\nfn main() {}\n",
        "L1611",
    );
}

#[test]
fn l1611_effectful_seq_return_position_no_tail() {
    // WP2 fixer regression (finding F5/F6): an EFFECTFUL fn whose body-tail chain
    // ends in a tail-less `seq` in return position must draw L1611 — not fall
    // through to the pre-existing L1306. The effectful-B-present tail lowers under
    // `ChainCtx::RetValue` so `emit_seq_block` sees return position (ADR-0019
    // pin c; DESIGN §8.10). The pure analogue is `l1611_seq_return_position_no_tail`.
    assert_rejects(
        "fn f(x: i32) -> i32 { \"hi\" -> println; x -> seq { 3 -> a; } }\nfn main() { 5 -> f -> r; r -> println; }\n",
        "L1611",
    );
    // A tail-less seq that is itself the whole (effectful) body-tail: same code.
    assert_rejects(
        "fn f(x: i32) -> i32 { x -> seq { \"hi\" -> println; } }\nfn main() { 5 -> f -> r; r -> println; }\n",
        "L1611",
    );
}

#[test]
fn seq_return_position_valued_effectful_lowers_clean() {
    // WP2 fixer regression: the RetValue fix must NOT break a VALUED seq in an
    // effectful fn's return position — the tail value is still handed back and
    // packed with the token (prints "hi" then returns x*2).
    let _ = lower_ok(
        "fn f(x: i32) -> i32 { \"hi\" -> println; x -> seq { x * 2 -> a; a } }\nfn main() { 5 -> f -> r; r -> println; }\n",
    );
}

#[test]
fn l1404_effectful_seq_in_phi_arm() {
    // WP2 fixer regression (finding F1/F2): an effectful stage inside a `seq`
    // inside a Phi-position guard arm must draw L1404 — the phi-arm scan descends
    // into the seq body (ADR-0019 §8.10). Without the descent the effect lowered
    // UNCONDITIONALLY (hoisted out of the Phi): a validate-clean miscompile.
    assert_rejects(
        "fn f(b: bool, x: i32) -> i32 { b -> { -true-> { x -> seq { x -> println; x -> a; a } } -false-> x; } -> ret; }\nfn main() {}\n",
        "L1404",
    );
}

#[test]
fn l1404_effectful_fanout_in_phi_arm() {
    // Sibling of the above (finding F1's Fanout analog): the phi-arm scan descends
    // into fanout branches too — an effect in a fanout branch inside a Phi arm is
    // the same unconditional-effect hazard, L1404.
    assert_rejects(
        "fn f(b: bool, x: i32) -> i32 { b -> { -true-> { x -> { -> println; -> a; } a } -false-> x; } -> ret; }\nfn main() {}\n",
        "L1404",
    );
}

// (The pre-ADR-0027 `l1108_capture_in_seq_in_map_body` row moved to
// tests/captures.rs as the positive pin `capture_in_seq_in_map_body_lowers`:
// the seq-descent read of enclosing `k` is a legal capture now — D1.)

#[test]
fn l1108_indexed_bind_captures_enclosing_local_in_map_body() {
    // Sibling of the (now positive) seq-in-body case — see
    // `capture_in_seq_in_map_body_lowers` in tests/captures.rs — for the
    // ADR-0021 sugar: an
    // indexed bind `c[i] <- v` whose target `c` is an enclosing local (not a
    // body-local) captures it — must draw L1108, not the misleading L1101
    // "unresolved name". The map/fold-body capture check (typing.rs
    // `capture_stmt`) must NOT treat the indexed-bind target as a fresh local.
    assert_rejects(
        "fn main() { mut c: [i32; 3] <- [0,0,0]; [1,2,3] -> map { e -> e -> seq { c[0] <- e; e } } -> r; r[0] -> println; }\n",
        "L1108",
    );
    // The index expression is capture-checked too — but a READ of enclosing
    // `k` in the index is a legal capture now (ADR-0027 D1): the L1108 here
    // comes from the WRITE `c[k] <- e` targeting enclosing `c` (an indexed
    // bind is a rebind, never a fresh shadow — ADR-0021).
    assert_rejects(
        "fn main() { 2 -> k; mut c: [i32; 3] <- [0,0,0]; [1,2,3] -> map { e -> e -> seq { c[k] <- e; e } } -> r; r[0] -> println; }\n",
        "L1108",
    );
}

#[test]
fn indexed_update_of_body_local_in_map_body_lowers_clean() {
    // Inverse control for the fix above: an indexed bind whose target is a
    // *body-local* (declared inside the map body) is not a capture — it lowers
    // clean (ADR-0021 §1 "legal in map/fold bodies").
    let _ = lower_ok(
        "fn main() { [1,2,3] -> map { e -> e -> seq { mut d: [i32; 2] <- [0,0]; d[0] <- e; d[0] } } -> r; r[0] -> println; }\n",
    );
}

#[test]
fn effectful_seq_in_fanout_join_rejected() {
    // ADR-0019 pin e / plan WP2 item 5: an effectful `seq` branch inside a
    // *Plain* (parallel) fanout that joins is rejected by lower's existing
    // L1305 — exactly as a bare effectful branch is (the effectful seq produces
    // no value for the join). Parity with `print_branch_join` below: the seq
    // opens no effect escape hatch.
    assert_rejects(
        "fn main() { 5 -> { -> seq { -> println }; -> println; } -> x; x -> println; }\n",
        "L1305",
    );
    // The bare-print branch it must match, unchanged (plan matrix).
    assert_rejects(
        "fn main() { 5 -> { -> println; -> println; } -> x; x -> println; }\n",
        "L1305",
    );
}

#[test]
fn empty_seq_lowers_clean() {
    // `seq { }` in statement position: no value, no continuation — lowers clean.
    let _ = lower_ok("fn main() { 5 -> seq { } }\n");
}

#[test]
fn seq_bindings_escape_to_enclosing_scope() {
    // ADR-0019 pin b: a binding made inside `seq` lives in the enclosing scope,
    // so `a` is visible after the seq (else `a -> println` would be L1101). The
    // `fanout.flow` idiom, now for seq.
    let _ = lower_ok("fn main() { 5 -> seq { 7 -> a; } a -> println; }\n");
}

#[test]
fn seq_headless_statements_seed_from_input() {
    // ADR-0019 pin a / compat pin 3: the old bare-chain branch form — headless
    // chain *statements* — seeds each from the seq input and lowers clean (each
    // `-> println` prints the seq input `42`, ordered by the token thread).
    let _ = lower_ok("fn main() { 42 -> seq { -> println; -> println; } }\n");
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

// --- ADR-0021: array element update `c[i] <- v` -----------------------------
// The indexed bind is a *rebind* (wiring point a). No new L-codes: a non-array
// target reuses L1204 (index on non-array), an element-ty clash reuses L1201/
// L1203 (width-unification), a non-`mut` target reuses L1104, and an assignment
// in a Phi arm reuses L1408 (wiring point c).

#[test]
fn array_update_non_mut_is_l1104() {
    // `c` bound non-`mut`; `c[0] <- 9` is an immutable rebind → L1104.
    let src = r#"fn main() {
    [0, 0, 0, 0] -> c: [i32; 4];
    c[0] <- 9;
    c[0] -> println;
}
"#;
    assert_rejects(src, "L1104");
}

#[test]
fn array_update_unbound_is_l1101() {
    // `d` never bound; `d[0] <- 9` targets an unknown name → L1101.
    let src = r#"fn main() {
    d[0] <- 9;
}
"#;
    assert_rejects(src, "L1101");
}

#[test]
fn array_update_non_array_target_is_l1204() {
    // `c` is a scalar; `c[0] <- 9` is an index-write on a non-array → L1204.
    let src = r#"fn main() {
    mut c: i32 <- 5;
    c[0] <- 9;
    c -> println;
}
"#;
    assert_rejects(src, "L1204");
}

#[test]
fn array_update_elem_ty_clash_is_l1201() {
    // A concrete element-ty clash (`bool` written into an `i32` array) is caught
    // by width-unification → L1201.
    let src = r#"fn main() {
    mut c: [i32; 4] <- [0, 0, 0, 0];
    c[0] <- true;
    c[0] -> println;
}
"#;
    assert_rejects(src, "L1201");
}

#[test]
fn array_update_in_phi_arm_is_l1408() {
    // `c[0] <- 1` inside a Phi-position arm rebinds an enclosing `mut`
    // unconditionally → L1408 (wiring point c: scan_stmt records the indexed
    // bind's target).
    let src = r#"fn main() {
    mut c: [i32; 4] <- [0, 0, 0, 0];
    5 -> x: i32;
    (x > 0) -> {
        -true-> { c[0] <- 1; x }
        -false-> x
    } -> r: i32;
    r -> println;
}
"#;
    assert_rejects(src, "L1408");
}

#[test]
fn lone_indexed_update_loop_not_l1502() {
    // A loop whose ONLY enclosing-mut mutation is `c[0] <- …` must NOT be
    // L1502-rejected (wiring point b: collect_assigns_stmt records the indexed
    // bind's target, so the mut array joins the carried set). It lowers clean.
    let src = r#"fn main() {
    mut c: [i32; 4] <- [1, 0, 0, 0];
    loop {
        c[0] <- c[0] + 1;
        (c[0] < 4) -> {
            -true-> -> loop;
            -false-> c -> result;
        }
    }
    result[0] -> println;
}
"#;
    let codes = lower_err_codes(src);
    assert!(
        !codes.iter().any(|c| c == "L1502"),
        "L1502 must not fire for a lone indexed-update loop; got {codes:?}"
    );
    // In fact it lowers clean.
    let _ = lower_ok(src);
}

// --- L1612–L1613: iota / fill (ADR-0031 pipeline form) -----------------------

#[test]
fn l1612_iota_tuple_wire_rejects() {
    // The old call-arity case in arrow clothes: a tuple is not a count.
    assert_rejects("fn main() { (4, 5) -> iota -> t; }\n", "L1612");
}

#[test]
fn l1612_iota_non_literal_count_rejects() {
    // An array wire is not a static count (a runtime size is ADR-0023).
    assert_rejects(
        "fn main() { 4 -> iota -> t; t -> k; k -> iota -> u; }\n",
        "L1612",
    );
}

#[test]
fn l1612_iota_zero_rejects_and_oversize_is_width_owned() {
    assert_rejects("fn main() { 0 -> iota -> t; }\n", "L1612");
    // ADR-0031: an oversize literal never reaches iota — the literal-width
    // system rejects it first (L1202). The IR-level oversize twin
    // (`EnumerateIndexOverflow`) is pinned in flow-ir's builder tests.
    assert_rejects("fn main() { 2147483648 -> iota -> t; }\n", "L1202");
}

#[test]
fn l1613_fill_wire_shape_rejects() {
    // Not a 2-tuple: a scalar and a 3-tuple both miss the (value, count) shape.
    assert_rejects("fn main() { 1.0 -> fill -> s; }\n", "L1613");
    assert_rejects("fn main() { (1.0, 2, 3) -> fill -> s; }\n", "L1613");
}

#[test]
fn l1613_fill_zero_rejects_and_oversize_is_width_owned() {
    assert_rejects("fn main() { (1.0, 0) -> fill -> s; }\n", "L1613");
    // Same width-system ownership as iota (L1202 fires on the literal).
    assert_rejects("fn main() { (1.0, 2147483648) -> fill -> s; }\n", "L1202");
}

#[test]
fn iota_bound_literal_count_lowers() {
    // ADR-0031 consequence: a NAME bound to a literal is the same Constant
    // object — still static. Strictly more expressive than the old AST check.
    lower_ok("fn main() { 4 -> n; n -> iota -> t; t[2] -> println; }\n");
}

// --- L1614: explicit numeric widening (ADR-0029) ----------------------------

#[test]
fn l1614_invalid_widen_sources_reject() {
    assert_rejects("fn main() { 5 -> x: i64; x -> widen_f64 -> y; }\n", "L1614");
    assert_rejects("fn main() { [1, 2] -> widen_i64 -> y; }\n", "L1614");
    assert_rejects("fn main() { 1.0 -> widen_f32 -> y; }\n", "L1614");
}

#[test]
fn l1614_message_names_the_legal_lattice() {
    let src = "fn main() { 5 -> x: i64; x -> widen_f64 -> y; }\n";
    let po = flow_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "{:?}", po.diagnostics);
    let ds = flow_lower::lower(src, &po.program).unwrap_err();
    let d = ds
        .iter()
        .find(|d| d.code.0 == "L1614")
        .unwrap_or_else(|| panic!("expected L1614, got {ds:?}"));
    for edge in ["i32→i64", "i32→f32", "i32→f64", "f32→f64"] {
        assert!(d.message.contains(edge), "message omits {edge}: {d:?}");
    }
}
