//! DESIGN §6 test plan item 2/4/5: golden `.ll` insta snapshots (byte-stable,
//! L2), determinism (emit twice → byte-equal), and the `Unsupported` pin on the
//! hand-built nested-loop graph (L3).
//!
//! Micro shapes cover the templates the 10 examples don't isolate cleanly:
//! signed `Div`/`Mod` guards (zero + `MIN/-1`), `Update` memcpy+store, the
//! last-use elision's in-place/keep-copy pins (suggestions #2), the S20
//! proven-`Index` guard elision + the FnAttrs proof refinement, and two
//! sequential loops in one fn (the S12 P0 shape). Select-Phi, index guards,
//! and loop CFG are additionally covered by the `abs` / `fir` / `sum_to_n`
//! example snapshots.

use flow_backend_llvm::{EmitError, emit};
use flow_ir::{CategoryIr, Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value};

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

const EXAMPLES: &[&str] = &[
    "abs",
    "calc",
    "fanout",
    "fir",
    "pipeline",
    "sepia",
    "seq_demo",
    "sum_to_n",
    "vector_add",
    "zip_demo",
];

fn lower_src(src: &str) -> CategoryIr {
    let po = flow_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
    flow_lower::lower(src, &po.program).unwrap_or_else(|d| panic!("lower: {d:?}"))
}

fn build_example(name: &str) -> CategoryIr {
    let path = format!(
        "{}/../../../examples/{}.flow",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    let src = std::fs::read_to_string(&path).unwrap();
    lower_src(&src)
}

#[test]
fn golden_examples() {
    for name in EXAMPLES {
        let ir = build_example(name);
        let ll = emit(&ir).unwrap();
        insta::assert_snapshot!(format!("example_{name}"), ll);
    }
}

// --- micro shapes ---------------------------------------------------------

/// Signed `Div`/`Mod`: zero-guard trap block + `MIN/-1` guard (Div ⇒ MIN, Mod ⇒
/// 0), plus wrapping `add` (no nsw).
#[test]
fn golden_micro_arith() {
    let src = r#"
fn f(a: i32, b: i32) -> i32 {
    a / b -> q;
    a % b -> r;
    q + r -> ret;
}
fn main() {
    (7, 3) -> f -> v;
    v -> println;
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    insta::assert_snapshot!("micro_arith", ll);
}

/// `Update` (ADR-0021): index guard + `llvm.memcpy` + dynamic GEP store. Built
/// via `IrBuilder` (surface rebind sugar is a later WP).
#[test]
fn golden_micro_update() {
    let mut b = IrBuilder::new();
    let arr_ty = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: 4,
    };
    let f = b
        .declare(FuncKind::Named, "main", arr_ty.clone(), arr_ty, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let a = fb.input();
        let i = fb.constant(Value::I32(2), L).unwrap();
        let v = fb.constant(Value::I32(99), L).unwrap();
        fb.update(a, i, v, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let ll = emit(&ir).unwrap();
    insta::assert_snapshot!("micro_update", ll);
}

#[test]
fn widen_emits_conversion_instructions() {
    let input = Ty::Tuple(vec![Ty::i32(), Ty::f32()]);
    let output = Ty::Tuple(vec![Ty::i64(), Ty::f32(), Ty::f64(), Ty::f64()]);
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "widen", input, output, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let arg = fb.input();
        let i = fb.proj(arg, 0, Dest::Fresh(None), L).unwrap();
        let f32 = fb.proj(arg, 1, Dest::Fresh(None), L).unwrap();
        let i64 = fb.widen(i, Ty::i64(), Dest::Fresh(None), L).unwrap();
        let i_f32 = fb.widen(i, Ty::f32(), Dest::Fresh(None), L).unwrap();
        let i_f64 = fb.widen(i, Ty::f64(), Dest::Fresh(None), L).unwrap();
        let f_f64 = fb.widen(f32, Ty::f64(), Dest::Fresh(None), L).unwrap();
        fb.pack(&[i64, i_f32, i_f64, f_f64], Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ll = emit(&b.seal(f).unwrap()).unwrap();
    assert_eq!(ll.matches(" = sext i32 ").count(), 1, "{ll}");
    assert_eq!(ll.matches(" = sitofp i32 ").count(), 2, "{ll}");
    assert_eq!(ll.matches(" = fpext float ").count(), 1, "{ll}");
}

// --- last-use Update elision (suggestions #2; plan-last-use §2 rule 4) -----

/// The matmul4-class loop form — a loop-carried array rebuilt per iteration by
/// `c[t] <- v` — elides the whole-array memcpy: the plan proves the update's
/// source (the merge's Proj view) `dead_after` the update (decide-cone reads
/// and the exit-route pin rank before the advance-cone update; ¬escapes,
/// ¬carried), so the element store lands in the carried storage in place and
/// the Update target has no separate array alloca. Other semantically owed
/// array transfers use `llvm.memcpy` per WP3b and are not part of this count.
#[test]
fn golden_update_inplace_carried_loop() {
    let src = r#"
fn build(n: i32) -> [i32; 4] {
    mut c: [i32; 4] <- [0, 0, 0, 0];
    mut t: i32 <- 0;
    loop {
        (t < 4) -> {
            -true-> {
                t + 1 -> v;
                c[t] <- v;
                t + 1 -> t;
                -> loop;
            }
            -false-> c -> ret;
        }
    }
}
fn main() {
    0 -> build -> r;
    r[2] -> println;
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    let build = ll
        .split("define internal [4 x i32] @fn")
        .nth(1)
        .and_then(|s| s.split("\n}\n").next())
        .expect("build fn");
    assert_eq!(
        build.matches("alloca [4 x i32]").count(),
        3,
        "the carried Update target shares its source slot (no fourth array alloca):\n{build}"
    );
    insta::assert_snapshot!("update_inplace_carried_loop", ll);
}

/// The elision's negative pins: when the plan cannot prove the source dead,
/// the fresh-alloca copy stays.
///
/// 1. Escape: the pre-update array is packed into the returned product (rule
///    2's escape through `Pair` fields) — the update keeps its memcpy.
/// 2. By-ref source: a Named fn's bare-Array parameter arrives as `ptr`
///    (suggestions #8) — ptr-resident, borrowed caller memory, never written
///    in place even though the rebind is its only non-escape use. The `ptr`
///    signature assertion pins that the by-ref shape is actually exercised.
#[test]
fn golden_update_memcpy_kept_when_not_dead() {
    // Escape via a Pair field of the output product.
    let escape = r#"
fn f(n: i32) -> ([i32; 4], [i32; 4]) {
    mut a: [i32; 4] <- [1, 2, 3, 4];
    a -> orig;
    a[1] <- 9;
    (orig, a) -> ret;
}
fn main() {
    0 -> f -> r;
    r.0[1] -> println;
    r.1[1] -> println;
}
"#;
    let ll = emit(&lower_src(escape)).unwrap();
    let f = ll
        .split("define internal { [4 x i32], [4 x i32] } @fn")
        .nth(1)
        .and_then(|s| s.split("\n}\n").next())
        .expect("escape f");
    assert_eq!(
        f.matches("call void @llvm.memcpy").count(),
        3,
        "the two owed return-field copies plus the escaping Update copy stay:\n{f}"
    );

    // Borrowed by-ref source (ptr-resident): the in-place write would land in
    // caller memory — vetoed, memcpy stays.
    let byref = r#"
fn f(a: [i32; 4]) -> [i32; 4] {
    mut c: [i32; 4] <- a;
    c[1] <- 99;
    c -> ret;
}
fn main() {
    [1, 2, 3, 4] -> f -> r;
    r[1] -> println;
}
"#;
    let ll = emit(&lower_src(byref)).unwrap();
    let fdef = ll
        .lines()
        .find(|l| l.starts_with("define internal [4 x i32] @fn"))
        .expect("f's define line");
    assert!(
        fdef.contains("(ptr"),
        "f's array parameter arrives by reference (the ptr-resident shape):\n{fdef}"
    );
    assert_eq!(
        ll.matches("call void @llvm.memcpy").count(),
        2,
        "the borrowed Update copy and owed output copy stay:\n{ll}"
    );
}

/// Two sequential canonical loops in one fn (S12 P0 shape) — each its own
/// guard-first quartet, exit attribution by route-feeder membership.
#[test]
fn golden_micro_two_loops() {
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
    let ll = emit(&lower_src(src)).unwrap();
    insta::assert_snapshot!("micro_two_loops", ll);
}

// --- ADR-0027 captures ----------------------------------------------------

/// A source-level capturing map: the body reads the enclosing `scale`, lowered
/// to a hidden leading input component — the body fn's input is `(cap, elem)`
/// and the map loop's call passes the assembled product (the capture is a real
/// leading argument, broadcast to every element). One map loop, no other
/// counted loop.
#[test]
fn golden_capture_map() {
    let src = r#"
fn main() {
    3 -> scale;
    [1, 2, 3] -> a;
    a -> map { x -> x * scale } -> b;
    b[1] -> println;
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    assert_eq!(
        ll.matches("icmp uge").count(),
        1,
        "exactly one counted (map) loop:\n{ll}"
    );
    let call = ll
        .lines()
        .find(|l| l.contains(" = call i32 @fn"))
        .expect("map body call site");
    assert!(
        call.contains("{ i32, i32 }"),
        "the body call passes the (capture, elem) product: {call}"
    );
    insta::assert_snapshot!("capture_map", ll);
}

/// A source-level capturing fold: `acc + x * scale` reads the enclosing
/// `scale` — the step fn's input is `(cap, acc, elem)` and the fold loop's
/// call passes all three (capture leading, per ADR-0027).
#[test]
fn golden_capture_fold() {
    let src = r#"
fn main() {
    3 -> scale;
    [1, 2, 3] -> a;
    (0, a) -> fold { acc, x -> acc + x * scale } -> total;
    total -> println;
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    assert_eq!(
        ll.matches("icmp uge").count(),
        1,
        "exactly one counted (fold) loop:\n{ll}"
    );
    let call = ll
        .lines()
        .find(|l| l.contains(" = call i32 @fn"))
        .expect("fold body call site");
    assert!(
        call.contains("{ i32, i32, i32 }"),
        "the step call passes the (capture, acc, elem) product: {call}"
    );
    insta::assert_snapshot!("capture_fold", ll);
}

/// The ADR-0021 motivating program in its natural ADR-0027 form: the
/// one-kernel matmul — a map over `enumerate` whose body is a fold over the
/// captured `a`/`b` (the body also captures the loop-derived `i`/`j`). ONE
/// elementwise map loop around one fold loop; the captured arrays reach the
/// inner body's `Index` operands as leading body-input components — **by
/// reference** (suggestions #6): the first-k Array components of each body
/// input lower to `ptr`, so no array bytes cross the per-element/per-step
/// call boundary (the 64 KB-per-call memcpy measured at S19).
#[test]
fn golden_capture_one_kernel_matmul() {
    // `a` doubles as the enumerate seed (only its length matters). N=4.
    let src = r#"
fn matmul(a: [f32; 16], b: [f32; 16], ks: [i32; 4]) -> [f32; 16] {
    a -> enumerate -> map { p ->
        p.0 / 4 -> i;
        p.0 % 4 -> j;
        (0.0, ks) -> fold { acc, k -> acc + a[i * 4 + k] * b[k * 4 + j] } -> cell;
        cell
    } -> c;
    c -> ret;
}
fn main() {
    [ -37.0, -30.0, -23.0, -16.0, -9.0, -2.0, 5.0, 12.0,
      19.0, 26.0, 33.0, 40.0, 47.0, -47.0, -40.0, -33.0] -> a: [f32; 16];
    [7.0, 14.0, 21.0, 28.0, 35.0, 42.0, 49.0, -45.0,
     -38.0, -31.0, -24.0, -17.0, -10.0, -3.0, 4.0, 11.0] -> b: [f32; 16];
    [0, 1, 2, 3] -> ks: [i32; 4];
    (a, b, ks) -> matmul -> c;
    c[0] -> println;
    c[15] -> println;
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    // Three counted loops total: enumerate + ONE map (in `matmul`) + the fold
    // (inside the map body fn) — the map is one loop, not per-element calls.
    assert_eq!(
        ll.matches("icmp uge").count(),
        3,
        "enumerate + ONE map loop + ONE fold loop:\n{ll}"
    );
    // Exactly two body call sites: the map body per element (in the map loop)
    // and the fold body per step (inside the map body) — no other per-element
    // call structure. Both pass their array captures as `ptr` fields.
    assert_eq!(
        ll.matches(" = call float @fn").count(),
        2,
        "one map-body call site + one fold-body call site:\n{ll}"
    );
    assert!(
        ll.contains(" = call float @fn") && ll.contains("({ ptr, ptr, ptr, { i32, float } } %"),
        "the map-body call passes the three array captures (ks, a, b) by reference:\n{ll}"
    );
    assert!(
        ll.contains("({ ptr, i32, ptr, i32, float, i32 } %"),
        "the fold-step call passes the captured a/b by reference (i/j by value):\n{ll}"
    );
    // The fold body (the only fn with `fmul` — the dot-product step): its
    // input product carries the captured `a`/`b` as `ptr`; each capture Proj
    // lands in an `alloca ptr`, and `a[i*4+k]` / `b[k*4+j]` index through the
    // forwarded pointers with dynamic (i64) GEPs — no inline array copies in
    // the body-input product.
    let fold_body = ll
        .split("define internal ")
        .skip(1)
        .filter_map(|s| s.split("\n}\n").next())
        .find(|s| s.contains("fmul"))
        .expect("the fold body fn");
    assert!(
        fold_body.contains("{ ptr, i32, ptr, i32, float, i32 } %arg"),
        "the fold body's array captures arrive by reference:\n{fold_body}"
    );
    assert_eq!(
        fold_body.matches("alloca ptr").count(),
        2,
        "each captured-array Proj lands in an `alloca ptr` slot:\n{fold_body}"
    );
    assert_eq!(
        fold_body
            .matches("getelementptr [16 x float], ptr %t")
            .count(),
        2,
        "a[i*4+k] and b[k*4+j] index the captured arrays through the forwarded pointers:\n{fold_body}"
    );
    insta::assert_snapshot!("capture_one_kernel_matmul", ll);
}

#[test]
fn golden_parallel_matmul_cap() {
    let src = r#"
fn main() {
    64 -> iota -> a;
    64 -> iota -> b;
    4 -> iota -> ks;
    16 -> iota -> cells;
    cells -> map { cell_id ->
        cell_id / 4 -> i;
        cell_id % 4 -> j;
        (0, ks) -> fold { acc, k ->
            acc + a[i * 4 + k] * b[k * 4 + j] -> partial;
            a[100] -> guarded;
            partial + guarded
        } -> value;
        value
    } -> c;
    c[0] -> println;
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    assert!(ll.contains("%Frame = type {"), "parallel frame:\n{ll}");
    assert!(
        ll.contains("define internal void @task0(i64 %lo, i64 %hi, ptr %frame)"),
        "parallel task functions:\n{ll}"
    );
    assert!(
        ll.contains("@ckpt0_entries = private unnamed_addr constant"),
        "packed checkpoint entries:\n{ll}"
    );
    let fold_body = ll
        .split("define internal ")
        .skip(1)
        .filter_map(|s| s.split("\n}\n").next())
        .find(|s| {
            s.contains("getelementptr [64 x i32]") && s.contains("call void @flow_par_trap(i64 ")
        })
        .expect("fold body");
    assert!(
        fold_body.contains("call void @flow_par_trap(i64 "),
        "fold body guards speculate into the parallel trap flag:\n{fold_body}"
    );
    assert!(
        !fold_body.contains("call void @flow_trap(i32"),
        "parallel fold body must not directly trap:\n{fold_body}"
    );
    insta::assert_snapshot!("parallel_matmul_cap", ll);
}

/// S24 review-find pin: a checkpoint INSIDE an effectful loop also fires
/// BEFORE the loop is entered. The loop's seed/entry glue reads task-produced
/// frame slots, so `flow_par_wait`+`flow_par_check` must precede the loop CFG
/// in the host body — the per-iteration hook alone would let the first entry
/// read race a still-running task.
#[test]
fn parallel_effectful_loop_waits_before_entry() {
    let src = r#"
fn main() {
    64 -> iota -> t;
    t -> map { x -> (x * 7) % 5 } -> a;
    (0, a) -> fold { acc, x -> acc + x } -> s0;
    s0 % 10 -> s;
    mut i: i32 <- s;
    loop {
        (i > 0) -> {
            -true-> { i -> println; i - 1 -> i; -> loop; }
            -false-> i -> done;
        }
    }
    done -> println;
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    let host = ll
        .split("define internal void @flow_main(")
        .nth(1)
        .expect("host fn");
    let wait = host
        .find("call void @flow_par_wait")
        .expect("host emits a checkpoint wait");
    let first_label = host.find("\nbb").expect("the loop CFG's first label");
    assert!(
        wait < first_label,
        "the in-loop checkpoint's wait+check must precede the loop CFG:\n{host}"
    );
    assert!(
        host.matches("call void @flow_par_wait").count() >= 2,
        "both the pre-loop and per-iteration checkpoint hooks exist:\n{host}"
    );
}

#[test]
fn parallel_scalar_guard_publishes_watermark() {
    let src = r#"
fn main() {
    4 -> iota -> xs;
    xs[1] -> divisor;
    8 / divisor -> value;
    value -> println;
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    assert!(
        ll.contains("call void @flow_par_trap(i64 "),
        "scalar divide speculates:\n{ll}"
    );
    assert!(
        ll.contains("call void @flow_par_watermark(i64 "),
        "scalar guard publishes its decided watermark:\n{ll}"
    );
}

/// S20 #6/#8 regression: capture/product staging may forward an array address,
/// but must never materialize the large array as a first-class SSA value.
#[test]
fn capture_array_staging_never_loads_whole() {
    let src = r#"
fn main() {
    32 -> iota -> a;
    32 -> iota -> ids;
    ids -> map { i -> a[i] } -> b;
    b[31] -> println;
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    assert_eq!(
        ll.matches("load [32 x").count(),
        0,
        "arrays larger than the small-class boundary never load whole:\n{ll}"
    );
}

// --- proven-Index guard elision (S20 `bounds_proof`; the vectorization unlock) ---

/// A proven `Index` — the plan shows its index statically inside `[0, n)` —
/// emits NO bounds guard (the trap is dead); everything unproven keeps the
/// two-sided guard + trap byte-identical. The map body below is the
/// matmul-cell class: an enumerate'd map whose body indexes a captured array
/// with the affine `h * 2 + r` (`h = p.0 / 2 ∈ [0, 1]`, `r = p.0 % 2 ∈ [0,
/// 1]` ⇒ `h*2+r ∈ [0, 3] < 4`). Its `Index` guard is gone (the index-guard
/// shape is the only `icmp slt`/`icmp sge` source here; the signed `Div`/`Mod`
/// zero/`MIN/-1` guards are `icmp eq` and stay). The unproven sibling — `cell`
/// indexing by its unknown i32 parameter — keeps exactly one guard.
#[test]
fn golden_proven_index_guard_elision() {
    let src = r#"
fn cell(a: [f64; 4], i: i32) -> f64 {
    a[i] -> r;
    r -> ret;
}
fn main() {
    [10.0, 20.0, 30.0, 40.0] -> a: [f64; 4];
    a -> enumerate -> map { p ->
        p.0 / 2 -> h;
        p.0 % 2 -> r;
        a[h * 2 + r] -> x;
        x * 2.0 -> y;
        y
    } -> b;
    (b, 2) -> cell -> r2;
    b[1] -> fst;
    r2 + fst -> t;
    t -> println;
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    let fns: Vec<&str> = ll
        .split("define internal ")
        .skip(1)
        .filter_map(|s| s.split("\n}\n").next())
        .collect();
    // The affine map body (the only fn with `fmul`): the proven Index's guard
    // is elided — no `icmp slt`/`icmp sge`/`or`-trap chain, just the GEP+load
    // (the div/mod `icmp eq` guards are a different op's and stay).
    let map_body = fns
        .iter()
        .find(|s| s.contains("fmul"))
        .expect("the affine map body fn");
    assert_eq!(
        map_body.matches("icmp slt").count() + map_body.matches("icmp sge").count(),
        0,
        "the proven Index emits no bounds guard:\n{map_body}"
    );
    assert_eq!(
        map_body.matches("getelementptr [4 x double]").count(),
        1,
        "the elided Index still emits its GEP+load:\n{map_body}"
    );
    // The unproven sibling (index by the unknown parameter): today's full
    // guard, byte-identical — one slt/sge/or chain + one trap call.
    let cell = fns
        .iter()
        .find(|s| s.contains("{ ptr, i32 } %arg"))
        .expect("the cell fn");
    assert_eq!(
        cell.matches("icmp slt").count(),
        1,
        "an unproven Index keeps the lower-bound compare:\n{cell}"
    );
    assert_eq!(
        cell.matches("icmp sge").count(),
        1,
        "an unproven Index keeps the upper-bound compare:\n{cell}"
    );
    assert_eq!(
        cell.matches("call void @flow_trap(i32 1)").count(),
        1,
        "an unproven Index keeps the index_oob trap:\n{cell}"
    );
    insta::assert_snapshot!("proven_index_guard_elision", ll);
}

/// The FnAttrs refinement: a fn whose only trap-looking ops are PROVEN
/// `Index`es cannot trap (the guards are elided), so it joins the clean set —
/// `readonly nounwind willreturn`, and a bare by-ref `ptr` parameter
/// additionally carries `noalias nocapture readonly`. Before the refinement
/// both fns below emitted attribute-free (any `Index` counted as
/// trap-capable); the map body also pins the zero-`icmp` cell shape.
#[test]
fn golden_proven_index_fn_clean_attrs() {
    let src = r#"
fn sum4(a: [f64; 4]) -> f64 {
    a[0] -> x;
    a[1] -> y;
    a[2] -> z;
    a[3] -> w;
    x + y -> s1;
    z + w -> s2;
    s1 + s2 -> ret;
}
fn main() {
    [10.0, 20.0, 30.0, 40.0] -> a: [f64; 4];
    a -> enumerate -> map { p -> a[p.0] } -> b;
    b -> sum4 -> s;
    s -> println;
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    let fns: Vec<&str> = ll
        .split("define internal ")
        .skip(1)
        .filter_map(|s| s.split("\n}\n").next())
        .collect();
    // The cell body (enumerate `.0` index ⇒ proven): no guard icmps at all,
    // and it is attribute-clean.
    let cell_body = fns
        .iter()
        .find(|s| s.contains("{ ptr, { i32, double } } %arg"))
        .expect("the map body fn");
    assert_eq!(
        cell_body.matches("icmp").count(),
        0,
        "the proven cell body has no guard instructions:\n{cell_body}"
    );
    assert!(
        cell_body
            .lines()
            .next()
            .unwrap()
            .contains("readonly nounwind willreturn"),
        "the cell body is attribute-clean:\n{cell_body}"
    );
    // sum4 (four constant indices, all proven): clean, and its bare-Array
    // by-ref parameter carries the full `noalias nocapture readonly` set.
    let sum4 = fns
        .iter()
        .find(|s| s.contains("ptr noalias nocapture readonly %arg"))
        .expect("sum4's by-ref ptr param carries the clean param attrs");
    assert!(
        sum4.lines()
            .next()
            .unwrap()
            .contains("readonly nounwind willreturn"),
        "sum4 is attribute-clean:\n{sum4}"
    );
    assert_eq!(
        sum4.matches("icmp").count(),
        0,
        "sum4's constant indices emit no guards:\n{sum4}"
    );
    insta::assert_snapshot!("proven_index_fn_clean_attrs", ll);
}

// --- determinism (L2) -----------------------------------------------------

#[test]
fn determinism_emit_twice_byte_equal() {
    for name in ["sum_to_n", "sepia", "vector_add"] {
        let a = emit(&build_example(name)).unwrap();
        let b = emit(&build_example(name)).unwrap();
        assert_eq!(a, b, "{name}: emit is not byte-deterministic");
    }
}

// --- Unsupported pin (L3) -------------------------------------------------

/// A multi-merge nested loop (two loops cross-fed into one SCC): not the
/// canonical quartet. Shape copied from `flow-rewrite/tests/identity.rs`.
fn multi_merge_nested_loop() -> CategoryIr {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "nested", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let c0 = fb.constant(Value::I32(0), L).unwrap();
        let c1 = fb.constant(Value::I32(0), L).unwrap();
        let outer = fb.begin_loop(c0, L).unwrap();
        let om = fb.merge_of(&outer);
        let inner = fb.begin_loop(c1, L).unwrap();
        let im = fb.merge_of(&inner);
        let ct = fb.constant(Value::Bool(true), L).unwrap();
        let cf = fb.constant(Value::Bool(false), L).unwrap();
        let inext = fb
            .binop(Operation::Add, om, im, Dest::Fresh(Some("inext".into())), L)
            .unwrap();
        fb.loop_back(&inner, inext, ct, L).unwrap();
        let onext = fb
            .binop(Operation::Add, im, om, Dest::Fresh(Some("onext".into())), L)
            .unwrap();
        fb.loop_back(&outer, onext, ct, L).unwrap();
        fb.loop_exit(&inner, im, cf, Dest::Fresh(Some("iout".into())), L)
            .unwrap();
        fb.end_loop(inner).unwrap();
        fb.loop_exit(&outer, om, cf, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(outer).unwrap();
        fb.finish().unwrap();
    }
    b.seal(f).expect("fused nested loops seal")
}

#[test]
fn nested_loop_is_unsupported() {
    let ir = multi_merge_nested_loop();
    let e = emit(&ir).unwrap_err();
    assert!(
        matches!(e, EmitError::Unsupported { ref feature, .. } if feature == "nested loops"),
        "expected Unsupported {{ nested loops }}, got {e:?}"
    );
}

/// S13 orchestrator-review regression: an exit-only computed payload
/// (`acc + 12345` consumed only by the exit route) belongs to the decide cone
/// but lives outside the loop SCC. The walk must not re-emit it after the loop
/// (dead recompute for values; a DOUBLE side effect for an exit-arm Print) —
/// the constant appears exactly once in the module.
#[test]
fn exit_only_payload_emitted_once() {
    let src = r#"
fn f(n: i32) -> i32 {
    mut i: i32 <- 0;
    mut acc: i32 <- 0;
    loop {
        (i < n) -> {
            -true-> { acc + 2 -> acc; i + 1 -> i; -> loop; }
            -false-> acc + 12345 -> ret;
        }
    }
}
fn main() { 4 -> f -> r; r -> println; }
"#;
    let ll = emit(&lower_src(src)).unwrap();
    assert_eq!(
        ll.matches("12345").count(),
        1,
        "exit-only payload must be emitted exactly once (driver-owned):\n{ll}"
    );
}
