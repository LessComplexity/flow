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

use flow_backend_llvm::{EmitError, EmitOpts, emit, emit_with_opts};
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

const TILE_MATMUL_SRC: &str = r#"
fn matmul(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    16 -> iota -> cells;
    4 -> iota -> ks;
    cells -> map { cell ->
        cell / 4 -> i;
        cell % 4 -> j;
        (0.0, ks) -> fold { acc, k -> acc + a[i * 4 + k] * b[k * 4 + j] }
    } -> c;
    c -> ret;
}
fn main() {
    [ -37.0, -30.0, -23.0, -16.0, -9.0, -2.0, 5.0, 12.0,
      19.0, 26.0, 33.0, 40.0, 47.0, -47.0, -40.0, -33.0] -> a: [f32; 16];
    [7.0, 14.0, 21.0, 28.0, 35.0, 42.0, 49.0, -45.0,
     -38.0, -31.0, -24.0, -17.0, -10.0, -3.0, 4.0, 11.0] -> b: [f32; 16];
    (a, b) -> matmul -> c;
    c[0] -> println;
    c[15] -> println;
}
"#;

const TILE_MATMUL_F64_SRC: &str = r#"
fn matmul(a: [f64; 30], b: [f64; 100]) -> [f64; 120] {
    120 -> iota -> cells;
    5 -> iota -> ks;
    cells -> map { cell ->
        cell / 20 -> i;
        cell % 20 -> j;
        (0.0, ks) -> fold { acc, k -> acc + a[i * 5 + k] * b[k * 20 + j] }
    } -> c;
    c -> ret;
}
fn main() {
    30 -> iota -> ais;
    ais -> map { x -> (x * 5 + 7) % 67 - 33 -> widen_f64 } -> a;
    100 -> iota -> bis;
    bis -> map { x -> (x * 7 + 11) % 101 - 50 -> widen_f64 } -> b;
    (a, b) -> matmul -> c;
    c[0] -> println;
    c[119] -> println;
}
"#;

const TILE_FIR_SRC: &str = r#"
fn fir(w: [f32; 4], x: [f32; 19]) -> [f32; 16] {
    16 -> iota -> ts;
    4 -> iota -> kr;
    ts -> map { t ->
        (0.0, kr) -> fold { acc, k -> acc + w[k] * x[t + k] }
    } -> y;
    y -> ret;
}
fn main() {
    [1.0, -2.0, 3.0, -4.0] -> w: [f32; 4];
    [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0,
     11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0] -> x: [f32; 19];
    (w, x) -> fir -> y;
    y[0] -> println;
    y[15] -> println;
}
"#;

/// A K=300 packed matmul — deep enough that the KC k-panel nest applies under
/// the `generic` profile (kc = 128) and shallow enough that it does NOT under
/// `apple-m` (kc = 4096). Shared by the KC golden and the profile-gate test.
const KC_SRC: &str = r#"
fn matmul(a: [f32; 2400], b: [f32; 9600]) -> [f32; 256] {
    256 -> iota -> cells;
    300 -> iota -> ks;
    cells -> map { cell ->
        cell / 32 -> i;
        cell % 32 -> j;
        (0.0, ks) -> fold { acc, k -> acc + a[i * 300 + k] * b[k * 32 + j] }
    } -> c;
    c -> ret;
}
fn main() {
    2400 -> iota -> ta;
    ta -> map { t -> (t * 7 + 13) % 101 - 50 -> widen_f32 } -> a;
    9600 -> iota -> tb;
    tb -> map { t -> (t * 7 + 57) % 101 - 50 -> widen_f32 } -> b;
    (a, b) -> matmul -> c;
    c[0] -> println;
    c[255] -> println;
}
"#;

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

fn flow_main(ll: &str) -> &str {
    let marker = ll.find("@flow_main(").expect("flow_main");
    let start = ll[..marker]
        .rfind("define internal ")
        .expect("flow_main def");
    let end = start + ll[start..].find("\n}\n").expect("flow_main end") + 3;
    &ll[start..end]
}

fn function_containing<'a>(ll: &'a str, needle: &str) -> &'a str {
    let marker = ll
        .find(needle)
        .unwrap_or_else(|| panic!("missing {needle}"));
    let start = if ll[marker..].starts_with("define internal ") {
        marker
    } else {
        ll[..marker]
            .rfind("define internal ")
            .expect("function start")
    };
    let end = start + ll[start..].find("\n}\n").expect("function end") + 3;
    &ll[start..end]
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
fn golden_tile_map_shapes() {
    let ir = flow_rewrite::rewrite(lower_src(TILE_MATMUL_SRC)).ir;
    let tiled = emit(&ir).unwrap();
    let tiled_fn = function_containing(&tiled, " = alloca [64 x float]");
    // Rows run in TI=4 register blocks, so the accumulator is one flat
    // entry-block scratch of TI×TJ elems (subrow r at r*16 + lane).
    assert!(
        tiled_fn
            .lines()
            .any(|line| line.trim_start().starts_with("%s")
                && line.contains(" = alloca [64 x float]")),
        "tile accumulator must be one flat [TI=4 x TJ=16] entry-block scratch alloca:\n{tiled_fn}"
    );
    assert!(
        !tiled_fn.contains(" = call float @fn"),
        "tiled map must not call its per-cell body:\n{tiled_fn}"
    );
    // Row bounds: the row lo plus the interior full-window row end — the i
    // loop splits into boundary head rows / TI-blocked full-window interior /
    // tail rows.
    assert!(
        tiled_fn.contains(" = udiv i64 %lo, 4")
            && tiled_fn.contains(" = add i64 %hi, 3")
            && tiled_fn.contains(" = add i64 %lo, 3")
            && tiled_fn.contains(" = udiv i64 %hi, 4"),
        "tile nest must contain the row bounds, incl. the interior full-window end:\n{tiled_fn}"
    );
    // plan-s30: the jt-outer main body is the vector-accumulator path — its
    // TI=4 seed/lane/store loops are replaced by `<TJ x float>` phis, so NO
    // lane loop is bounded by the constant TJ=16 any more. The loops that
    // remain (remainder tile, boundary rows) are all runtime bounded.
    assert_eq!(
        tiled_fn
            .lines()
            .filter(|line| {
                let line = line.trim();
                line.contains(" = icmp uge i64 %t") && line.ends_with(", 16")
            })
            .count(),
        0,
        "the main tile must have no constant-TJ lane loop left:\n{tiled_fn}"
    );
    // The main tile's accumulator state lives only in SSA: 4 header phis + 4
    // exit phis, one contiguous b vector load per k step (×2 unrolled + the
    // odd tail) and one vector store per subrow — no accumulator load, store
    // or GEP anywhere in the k loop.
    assert_eq!(
        (
            tiled_fn.matches("phi <16 x float>").count(),
            tiled_fn.matches("load <16 x float>").count(),
            tiled_fn.matches("store <16 x float>").count(),
        ),
        (8, 3, 4),
        "main tile: 4 header + 4 exit phis, 3 b loads, 4 subrow stores:\n{tiled_fn}"
    );
    // Composition rule 3: vector memory ops carry the ELEMENT alignment, never
    // the `<16 x float>` ABI's 64 — `j0` offsets are arbitrary.
    assert!(
        tiled_fn
            .lines()
            .filter(|line| line.contains("<16 x float>") && line.contains(", ptr "))
            .all(|line| line.trim_end().ends_with(", align 4")),
        "vector accumulator memory must be element-aligned:\n{tiled_fn}"
    );
    // The acc scratch is now addressed only by the tiles that kept the memory
    // form (remainder + boundary rows); it was 44 GEPs before S30.
    assert_eq!(
        tiled_fn
            .matches("getelementptr [64 x float], ptr %s0")
            .count(),
        24,
        "only the non-vector tiles may address the acc scratch:\n{tiled_fn}"
    );
    // The outer main and remainder bodies each compute their packed-panel
    // base once, then reuse it across head/interior/tail row groups.
    assert_eq!(
        (
            tiled_fn
                .lines()
                .filter(|line| line.contains(" = udiv i64 ") && line.trim_end().ends_with(", 16"))
                .count(),
            tiled_fn
                .lines()
                .filter(|line| line.contains(" = mul i64 ") && line.trim_end().ends_with(", 64"))
                .count(),
        ),
        (2, 2),
        "jt-outer kernel must compute one panel base in each main/remainder body:\n{tiled_fn}"
    );
    // One runtime `tj = min(remaining, TJ)` select belongs to the outer
    // remainder tile. Boundary rows retain signed jw clipping in both outer
    // bodies.
    assert_eq!(
        tiled_fn
            .lines()
            .filter(|line| {
                let line = line.trim();
                line.contains(" = select i1 ") && line.ends_with(", i64 16")
            })
            .count(),
        1,
        "jt-outer remainder must clip once to tj = min(remaining, TJ):\n{tiled_fn}"
    );
    assert_eq!(
        (
            tiled_fn.matches(" = icmp slt i64 ").count(),
            tiled_fn.matches(" = icmp sgt i64 ").count(),
        ),
        (4, 4),
        "head/tail rows must retain signed jw clipping in both jt bodies:\n{tiled_fn}"
    );
    assert!(
        tiled.contains(" = alloca [64 x float], align 64")
            && tiled.contains("store float zeroinitializer")
            && tiled.contains("getelementptr [64 x float], ptr %packed"),
        "module must zero-pad and address the packed b buffer:\n{tiled}"
    );
    assert!(
        tiled_fn.contains("call void @llvm.prefetch.p0(")
            && tiled_fn
                .lines()
                .any(|line| line.contains(" = add i64 ") && line.ends_with(", 2")),
        "tile nest must prefetch and unroll k:\n{tiled_fn}"
    );
    insta::assert_snapshot!("tile_nest_shape", tiled_fn);

    let untiled = emit_with_opts(
        &ir,
        &EmitOpts {
            tiling: false,
            ..EmitOpts::default()
        },
    )
    .unwrap();
    let untiled_fn = function_containing(&untiled, " = call float @fn");
    assert!(
        !untiled_fn
            .lines()
            .any(|line| line.trim_start().starts_with("%s")
                && line.contains(" = alloca [16 x float]")),
        "untiled map must not allocate the tile accumulator:\n{untiled_fn}"
    );
    assert!(
        untiled_fn.contains(" = call float @fn"),
        "untiled map must retain its per-cell body call:\n{untiled_fn}"
    );
    insta::assert_snapshot!("untiled_map_shape", untiled_fn);

    let f64_ir = flow_rewrite::rewrite(lower_src(TILE_MATMUL_F64_SRC)).ir;
    let f64 = emit(&f64_ir).unwrap();
    let f64_fn = function_containing(&f64, " = alloca [32 x double]");
    assert!(
        f64_fn.contains(" = alloca [32 x double]")
            && f64.contains(" = alloca [120 x double], align 64")
            && f64.contains("store double zeroinitializer")
            && f64.contains("getelementptr [120 x double], ptr %packed")
            && f64_fn
                .lines()
                .any(|line| line.contains(" = add i64 ") && line.ends_with(", 8"))
            && f64_fn.contains("call void @llvm.prefetch.p0("),
        "f64 tile nest must use TJ=8 throughout packing and the kernel:\n{f64_fn}"
    );
    assert_eq!(
        (
            f64_fn
                .lines()
                .filter(|line| line.contains(" = udiv i64 ") && line.trim_end().ends_with(", 8"))
                .count(),
            f64_fn
                .lines()
                .filter(|line| line.contains(" = mul i64 ") && line.trim_end().ends_with(", 40"))
                .count(),
        ),
        (2, 2),
        "f64 jt-outer kernel must compute one panel base in each main/remainder body:\n{f64_fn}"
    );
    insta::assert_snapshot!("tile_nest_shape_f64", f64_fn);
}

/// S29 KC rung golden: a packed site with K=300 > TILE_KC=128 takes the KC
/// nest — jb blocks of NC=512 lanes outer, k-panels of 128 next (the peeled
/// kc==0 panel, the [128, K) loop with the runtime-short `k_hi = min(kc+128,
/// K)` last panel), the existing head/interior/tail i regions innermost, and
/// the a-panel pack per (i-block, kc). The acc discipline pins: seed splat
/// only in the kc==0 sweep (12 subrow seed loops: interior main+remainder
/// trios ×4 subrows + head/tail boundary trios ×1), an `out` spill at EVERY
/// panel end (2 sweeps × 12 stores) and a reload only in the post-kc0 sweep
/// (12 loads). Sites with K ≤ TILE_KC keep the jt-outer nest byte-for-byte
/// (the tile_nest_shape / tile_nest_shape_f64 goldens above are unmoved).
#[test]
fn golden_tile_map_shape_kc() {
    let src = KC_SRC;
    let ir = flow_rewrite::rewrite(lower_src(src)).ir;
    // The KC nest is a default-OFF performance tailor (EmitOpts::kc_nest —
    // measured a 3x loss locally at 1024 f32, S29); opt in to pin its shape.
    let tiled = emit_with_opts(
        &ir,
        &EmitOpts {
            kc_nest: true,
            ..EmitOpts::default()
        },
    )
    .unwrap();
    let tiled_fn = function_containing(&tiled, " = alloca [512 x float], align 64");
    // The KC accumulator: one [TI=4 x TJ=16] entry-block scratch, the same
    // width as the jt-outer nest's — partial sums park in `out` at every panel
    // end (the (jc, kc, ic) order runs other i-blocks in between), so only the
    // j-tile being computed is ever live.
    assert!(
        tiled_fn
            .lines()
            .any(|line| line.trim_start().starts_with("%s")
                && line.contains(" = alloca [64 x float]")),
        "KC acc must be one [TI=4 x TJ=16] entry-block scratch alloca:\n{tiled_fn}"
    );
    // The a-panel pack scratch: [TI=4 x TILE_KC=128], 64-aligned.
    assert!(
        tiled_fn
            .lines()
            .any(|line| line.trim_start().starts_with("%s")
                && line.contains(" = alloca [512 x float], align 64")),
        "KC a-panel pack must be an align-64 [TI=4 x KC=128] scratch alloca:\n{tiled_fn}"
    );
    assert!(
        !tiled_fn.contains(" = call float @fn"),
        "tiled map must not call its per-cell body:\n{tiled_fn}"
    );
    // The jb loop (NC=512 lane blocks): the block-end add and the loop step.
    assert!(
        tiled_fn
            .lines()
            .filter(|line| line.contains(" = add i64 ") && line.trim_end().ends_with(", 512"))
            .count()
            >= 2,
        "the jb block must step NC=512 (block end + loop step):\n{tiled_fn}"
    );
    // The jb block end clips jb_end = min(jb0 + NC, C=32): 1 select, plus
    // the four boundary-row jw_hi clips (head/tail × the two kc sweeps)
    // sharing the C literal.
    assert_eq!(
        tiled_fn
            .lines()
            .filter(|line| {
                let line = line.trim();
                line.contains(" = select i1 ") && line.contains(", i64 32, i64 ")
            })
            .count(),
        1 + 4,
        "the jb block end must clip jb_end = min(jb0 + NC, C):\n{tiled_fn}"
    );
    // The kc loop init with the literal TILE_KC=128 and the runtime-short
    // last panel's k_hi = min(kc + 128, K=300) select.
    assert_eq!(
        tiled_fn
            .lines()
            .filter(|line| line.trim_start().starts_with("store i64 128, ptr"))
            .count(),
        1,
        "the kc loop must init at TILE_KC=128:\n{tiled_fn}"
    );
    assert_eq!(
        tiled_fn
            .lines()
            .filter(|line| {
                let line = line.trim();
                line.contains(" = select i1 ") && line.contains(", i64 300, i64 ")
            })
            .count(),
        1,
        "the last kc panel must clip k_hi = min(kc + TILE_KC, K):\n{tiled_fn}"
    );
    // The acc discipline: seed splat only in the peeled kc==0 sweep — 8 scalar
    // subrow seed loops (remainder + boundary trios) now that the two main
    // trios splat into a vector instead; an out spill at every panel end and a
    // reload only in the post-kc0 sweep — 32 out-array GEPs total (the main
    // trios' 12 scalar lane-loop GEPs collapse to 4 vector ones per sweep).
    assert_eq!(
        tiled_fn.matches("store float 0x0000000000000000").count(),
        8,
        "seed splat belongs to the kc==0 sweep only:\n{tiled_fn}"
    );
    assert_eq!(
        tiled_fn.matches("getelementptr [256 x float]").count(),
        32,
        "2 spills + 1 reload per (i-block, j-tile) across the two sweeps:\n{tiled_fn}"
    );
    // plan-s30, the point of the rung: the KC k loop carries its accumulators
    // in phis, so nothing of the acc tile round-trips memory between panels.
    // Both main trios (the peeled kc==0 sweep and the [128, K) sweep) get 4
    // header + 4 exit phis; the 10 vector loads are 3 packed-b loads per trio
    // plus the post-kc0 sweep's 4 partial-sum reloads; the 8 vector stores are
    // the two sweeps' panel-end parks.
    assert_eq!(
        (
            tiled_fn.matches("phi <16 x float>").count(),
            tiled_fn.matches("load <16 x float>").count(),
            tiled_fn.matches("store <16 x float>").count(),
        ),
        (16, 10, 8),
        "KC main tiles must carry vector accumulators across the k loop:\n{tiled_fn}"
    );
    assert!(
        tiled_fn
            .lines()
            .filter(|line| line.contains("<16 x float>") && line.contains(", ptr "))
            .all(|line| line.trim_end().ends_with(", align 4")),
        "vector accumulator memory must be element-aligned:\n{tiled_fn}"
    );
    // The acc scratch is addressed only by the tiles that kept the memory form
    // (remainder + boundary rows); it was 88 GEPs before S30.
    assert_eq!(
        tiled_fn
            .matches("getelementptr [64 x float], ptr %s0")
            .count(),
        48,
        "only the non-vector tiles may address the acc scratch:\n{tiled_fn}"
    );
    // The kernel keeps the packed-b panel addressing, the ×2 k unroll, and
    // the next-k-line prefetch.
    assert!(
        tiled_fn.contains("call void @llvm.prefetch.p0(")
            && tiled_fn
                .lines()
                .any(|line| line.contains(" = add i64 ") && line.ends_with(", 2"))
            && tiled_fn.contains("getelementptr [9600 x float], ptr %packed"),
        "KC kernel must keep the packed panel, k unroll, and prefetch:\n{tiled_fn}"
    );
    insta::assert_snapshot!("tile_nest_shape_kc", tiled_fn);
}

#[test]
fn tile_contract_flags_are_opt_in() {
    let ir = flow_rewrite::rewrite(lower_src(TILE_MATMUL_SRC)).ir;
    let plain = emit_with_opts(&ir, &EmitOpts::default()).unwrap();
    let contracted = emit_with_opts(
        &ir,
        &EmitOpts {
            contract: true,
            ..EmitOpts::default()
        },
    )
    .unwrap();

    assert!(!plain.contains("fmul contract"));
    assert!(!plain.contains("fadd contract"));
    assert!(contracted.contains("fmul contract float"));
    assert!(contracted.contains("fadd contract float"));
}

#[test]
fn golden_tile_map_shape_1d() {
    let ir = flow_rewrite::rewrite(lower_src(TILE_FIR_SRC)).ir;
    let tiled = emit(&ir).unwrap();
    let tiled_fn = function_containing(&tiled, " = alloca [64 x float]");
    // The S28 window rung: TI=4 register blocks over the lane axis, so the
    // accumulator is one flat entry-block scratch of TI×TJ elems (subrow r at
    // r*16 + lane) — the rung-2 placement, one row deep.
    assert!(
        tiled_fn
            .lines()
            .any(|line| line.trim_start().starts_with("%s")
                && line.contains(" = alloca [64 x float]")),
        "FIR tile accumulator must be one flat [TI=4 x TJ=16] entry-block scratch alloca:\n{tiled_fn}"
    );
    assert!(
        !tiled_fn.contains(" = call float @fn"),
        "tiled FIR map must not call its per-sample body:\n{tiled_fn}"
    );
    // Full blocks step TI·TJ = 64 lanes (`jb + 64 <= hi`), entered from the
    // task window lo — no row loop, no [0, C) clip (rows == 1 makes the task
    // range the whole window).
    assert!(
        !tiled_fn.contains(" = udiv i64 %lo, 16") && !tiled_fn.contains(" = add i64 %hi, 15"),
        "window nest must drop the collapsed row loop:\n{tiled_fn}"
    );
    assert_eq!(
        tiled_fn
            .lines()
            .filter(|line| line.contains(" = add i64 ") && line.trim_end().ends_with(", 64"))
            .count(),
        1,
        "full blocks must step TI·TJ = 64:\n{tiled_fn}"
    );
    // The block body: per-subrow seed splat (4), the ×2-unrolled k body (K=4
    // even: two steps of 4 subrow lane loops), the single-k tail step (4),
    // and per-subrow stores (4) — every lane loop constant-bounded by TJ.
    // The TI=1 remainder adds one constant-TJ main trio (seed + k-lane +
    // store = 3).
    assert_eq!(
        tiled_fn
            .lines()
            .filter(|line| {
                let line = line.trim();
                line.contains(" = icmp uge i64 %t") && line.ends_with(", 16")
            })
            .count(),
        4 + 2 * 4 + 4 + 4 + 3,
        "block + remainder-main lane loops must be bounded by the constant TJ=16:\n{tiled_fn}"
    );
    // ONE runtime `tj = min(remaining, TJ)` select in the whole nest — the
    // TI=1 remainder tile. Full blocks never select.
    assert_eq!(
        tiled_fn
            .lines()
            .filter(|line| {
                let line = line.trim();
                line.contains(" = select i1 ") && line.ends_with(", i64 16")
            })
            .count(),
        1,
        "only the TI=1 remainder tile may clip to tj = min(remaining, TJ):\n{tiled_fn}"
    );
    // The shared read: ONE scalar w[k] load per emitted k step (three in the
    // block path — the ×2 pair plus the tail step — two in the TI=1
    // remainder), never one per (k, subrow): 5 w-load GEPs feed 14 FMA lane
    // loops (3 block steps × TI=4 subrows + 2 remainder k loops).
    assert_eq!(
        tiled_fn.matches("getelementptr [4 x float]").count(),
        5,
        "one scalar a load per k step, shared across the TI subrows:\n{tiled_fn}"
    );
    assert_eq!(
        tiled_fn.matches(" = fmul float ").count(),
        14,
        "3 block k-steps x TI=4 subrows + 2 remainder k-loops of FMAs:\n{tiled_fn}"
    );
    // The block k loop is ×2-unrolled (K=4 even) in the trio's shape: one
    // `kk + 2` step. The TI=1 remainder keeps the plain single-k loop (two
    // `k >= 4` heads — main tile and remainder tile).
    assert_eq!(
        tiled_fn
            .lines()
            .filter(|line| line.contains(" = add i64 ") && line.ends_with(", 2"))
            .count(),
        1,
        "the block k loop must unroll x2:\n{tiled_fn}"
    );
    assert_eq!(
        tiled_fn
            .lines()
            .filter(|line| {
                let line = line.trim();
                line.contains(" = icmp uge i64 %t") && line.ends_with(", 4")
            })
            .count(),
        2,
        "the TI=1 remainder must keep the plain single-k loop:\n{tiled_fn}"
    );
    assert_eq!(
        tiled_fn.matches(" = icmp ule i64 ").count(),
        2,
        "the full-block guard plus the TI=1 main-tile guard:\n{tiled_fn}"
    );
    insta::assert_snapshot!("tile_nest_shape_1d", tiled_fn);
}

const TILE_CONV_SRC: &str = r#"
fn main() {
    324 -> iota -> ti;
    ti -> map { t -> (t * 7 + 13) % 101 - 50 -> widen_f32 } -> img;
    9 -> iota -> kr;
    kr -> map { t -> (t * 5 + 3) % 31 - 15 -> widen_f32 } -> w;
    256 -> iota -> ts;
    ts -> map { t ->
        t / 16 -> i;
        t % 16 -> j;
        (0.0, kr) -> fold { acc, k -> acc + w[k] * img[(i + k / 3) * 18 + j + k % 3] }
    } -> y;
    y[0] -> println;
    y[255] -> println;
}
"#;

#[test]
fn golden_tile_map_shape_conv() {
    let ir = flow_rewrite::rewrite(lower_src(TILE_CONV_SRC)).ir;
    let tiled = emit(&ir).unwrap();
    let tiled_fn = function_containing(&tiled, " = alloca [16 x float]");
    // The S28 conv rung: the k-split record cashed as an unrolled (kq, kr) tap
    // nest. S31 (plan-s31-deduced-blocking item 2) moved the constant-TJ MAIN
    // tile onto `<TJ x elem>` SSA values; the runtime-`tj` remainder still uses
    // the flat [TJ] scratch, so the alloca remains — for the remainder alone.
    assert!(
        tiled_fn
            .lines()
            .any(|line| line.trim_start().starts_with("%s")
                && line.contains(" = alloca [16 x float]")),
        "the remainder tile keeps the [TJ=16] scratch alloca:\n{tiled_fn}"
    );
    // THE property this rung buys: the main tile touches no accumulator memory
    // at all. Every `[16 x float]` GEP is an accumulator access, and 11 of the
    // 22 (one seed + 9 taps + one store, per tile body) are gone with the main
    // body's — what remains is exactly the remainder tile's.
    assert_eq!(
        tiled_fn.matches("getelementptr [16 x float]").count(),
        11,
        "only the remainder tile may address accumulator memory:\n{tiled_fn}"
    );
    // Conv has no runtime k loop — the taps are unrolled at emission — so the
    // main tile's accumulator is a straight SSA chain, not even a phi: one
    // splat of the seed, then one fmul/fadd pair per tap.
    assert_eq!(
        tiled_fn.matches(" = fmul <16 x float> ").count(),
        9,
        "9 unrolled taps on vector accumulators in the main tile:\n{tiled_fn}"
    );
    assert_eq!(
        tiled_fn.matches(" = fadd <16 x float> ").count(),
        9,
        "9 vector accumulations in the main tile:\n{tiled_fn}"
    );
    assert_eq!(
        tiled_fn.matches(" = load <16 x float>, ").count(),
        9,
        "one contiguous b vector load per tap:\n{tiled_fn}"
    );
    assert_eq!(
        tiled_fn.matches("store <16 x float> ").count(),
        1,
        "the main tile stores its accumulator once:\n{tiled_fn}"
    );
    assert_eq!(
        tiled_fn.matches(" = insertelement <16 x float> ").count(),
        10,
        "the seed splat plus one w[k] splat per tap:\n{tiled_fn}"
    );
    // Element alignment, never the vector type's ABI alignment — j0 is
    // arbitrary and <16 x float> would claim 64 (S30 composition rule 3).
    assert!(
        !tiled_fn.contains("<16 x float>, ptr %t") || tiled_fn.contains(", align 4"),
        "vector accesses must carry the element alignment:\n{tiled_fn}"
    );
    assert!(
        !tiled_fn.contains(" = call float @fn"),
        "tiled conv map must not call its per-cell body:\n{tiled_fn}"
    );
    // The S27c priced refusal, cashed: the fold body's k/3, k%3 become
    // compile-time tap offsets — ZERO div/mod in the tile nest.
    assert!(
        !tiled_fn.contains("sdiv") && !tiled_fn.contains("srem"),
        "conv tile nest must contain no sdiv/srem (the taps constant-fold):\n{tiled_fn}"
    );
    // K=9 taps fully unrolled per j-tile: 9 constant-index w loads and 9 FMA
    // lane loops in each of the main and remainder tile bodies.
    assert_eq!(
        tiled_fn.matches("getelementptr [9 x float]").count(),
        18,
        "9 constant-index w[k] loads per tile body (main + remainder):\n{tiled_fn}"
    );
    assert_eq!(
        tiled_fn.matches(" = fmul float ").count(),
        9,
        "the scalar tap FMAs are now the REMAINDER tile's alone:\n{tiled_fn}"
    );
    // The tap offsets (cq·kq + cr·kr) fold to constants: offsets 18..38 by
    // kq row appear once per tile body (offset 0 needs no add, 1/2 collide
    // with lane arithmetic, so pin the unambiguous row offsets).
    for off in ["18", "19", "20", "36", "37", "38"] {
        assert_eq!(
            tiled_fn
                .lines()
                .filter(|line| {
                    line.contains(" = add i64 ") && line.trim_end().ends_with(&format!(", {off}"))
                })
                .count(),
            2,
            "tap offset {off} must appear as a constant add in each tile body:\n{tiled_fn}"
        );
    }
    // Constant-TJ lane loops everywhere on the main path: seed + 9 taps +
    // store = 11 per main tile body; the remainder tile body alone is
    // bounded by the runtime `tj` (one select in the whole nest).
    // The main tile has NO lane loops left — seed, taps and store are all one
    // vector operation each. (Was 11: seed + 9 taps + store.)
    assert_eq!(
        tiled_fn
            .lines()
            .filter(|line| {
                let line = line.trim();
                line.contains(" = icmp uge i64 %t") && line.ends_with(", 16")
            })
            .count(),
        0,
        "the main tile must have no constant-TJ lane loops left:\n{tiled_fn}"
    );
    assert_eq!(
        tiled_fn
            .lines()
            .filter(|line| {
                let line = line.trim();
                line.contains(" = select i1 ") && line.ends_with(", i64 16")
            })
            .count(),
        1,
        "only the remainder tile may clip to tj = min(remaining, TJ):\n{tiled_fn}"
    );
    insta::assert_snapshot!("tile_nest_shape_conv", tiled_fn);
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

/// A map over `n` cells: two `[n x i32]` frame fields, so `n` alone decides
/// whether the entry frame crosses the heap-lowering threshold.
fn heap_src(n: u32) -> String {
    format!(
        "\nfn main() {{\n    {n} -> iota -> t;\n    \
         t -> map {{ x -> (x * 7 + 13) % 101 - 50 }} -> a;\n    \
         a[{}] -> println;\n}}\n",
        n - 1
    )
}

/// plan-s29 emission item 4 (heap lowering). The parallel entry packs every
/// array into ONE `%Frame`, so `%Frame` is the block that blows the stack —
/// macOS caps the main thread at 64 MB hard, and 2048² f32 ×3 plus the packed
/// panel is ~67 MB. At or above `func.rs:HEAP_MIN_BYTES` (256 KB) the frame
/// becomes a `flow_rt_alloc` arena block; every field access stays the same
/// `getelementptr %Frame, ptr %frame, …` because an `alloca` result and a
/// `flow_rt_alloc` result are both just a `ptr`. `flow_main` then drops
/// exactly one `flow_rt_free_all`, AFTER `flow_par_finish` — composition rule
/// 4: no task can still be reading arena memory past the join.
///
/// The size operand is pinned deliberately. It is the emitter's own
/// struct-layout arithmetic (`func.rs:llt_bytes`), verified against LLVM's
/// `ptrtoint (ptr getelementptr (%Frame, ptr null, i32 1) to i64)`, and an
/// under-count is a silent heap overflow rather than a loud failure.
///
/// The n=64 twin is the negative control: below the threshold NOTHING moves,
/// down to the declaration block — which is what keeps every other golden in
/// this file byte-identical.
#[test]
fn golden_heap_lowered_frame() {
    let big = emit(&lower_src(&heap_src(100_000))).unwrap();
    assert!(
        big.contains("declare ptr @flow_rt_alloc(i64, i64)\ndeclare void @flow_rt_free_all()\n"),
        "the arena ABI is declared:\n{big}"
    );
    assert!(
        big.contains("%Frame = type { [100000 x i32], [100000 x i32], { ptr, i32 }, i32, i32 }"),
        "both arrays are frame fields:\n{big}"
    );
    assert!(
        !big.contains("alloca %Frame"),
        "a 780 KB frame must not be a stack block:\n{big}"
    );
    assert!(
        big.contains("  %frame = call ptr @flow_rt_alloc(i64 800024, i64 8)\n"),
        "the frame is one arena block at LLVM's own sizeof(%Frame):\n{big}"
    );
    let host = flow_main(&big);
    assert_eq!(
        host.matches("call void @flow_rt_free_all()").count(),
        1,
        "exactly one teardown:\n{host}"
    );
    assert!(
        host.find("call void @flow_par_finish").expect("host joins")
            < host
                .find("call void @flow_rt_free_all")
                .expect("host tears down"),
        "the teardown follows the join:\n{host}"
    );

    let small = emit(&lower_src(&heap_src(64))).unwrap();
    assert!(
        small.contains("  %frame = alloca %Frame\n"),
        "below the threshold the frame stays on the stack:\n{small}"
    );
    assert!(
        !small.contains("flow_rt_"),
        "a program that heap-allocates nothing gains no declaration:\n{small}"
    );
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

/// plan-time-builtin: the `time` extern is declared, a bracketed program emits
/// exactly two `flow_time_ms` calls on the host spine in chain order, and each
/// carries its checkpoint. The S29 fence is the point of the pin: each read
/// waits for every task written entirely BEFORE it in the source, so `t0`
/// fences the generation above the bracket and `t1` fences that PLUS the
/// bracketed kernel — `t1 - t0` is the work written between them, and the
/// generation is excluded (the S28 gen-boundary finding). Keyed on source
/// order because the dataflow graph gives none: pure work is unordered against
/// a clock read, so topo order is free to put all of it on either side, and
/// does. The `fsub` after the second read is the second fence: a `TimeMs`
/// result's consumer cone stays on the host spine, so it can never race the
/// host's write of the clock value.
/// plan-time-builtin rule 1, the loop case: a clock read inside a loop body
/// runs once per ITERATION. Four of lower's six effect detectors keyed on
/// `print` alone when `time` landed, and the two that did not learn it —
/// `emit.rs:effect_chain` (via `loop_body_has_effect`) and `scan_phi_arm` —
/// classify a loop body as pure, which hoists the read out of the cycle: one
/// timestamp for every iteration, silently. This pins the emitted position.
#[test]
fn time_inside_a_loop_stays_inside_the_loop() {
    let src = r#"
fn main() {
    0 -> mut i;
    loop {
        () -> time -> t;
        t -> println;
        i + 1 -> i;
        (i < 3) -> {
            -true-> -> loop;
            -false-> -> ret;
        }
    }
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    let host = flow_main(&ll);
    let read = host
        .find("call double @flow_time_ms()")
        .expect("the clock read is emitted");
    // The loop header is the block the back edge targets; the read must sit
    // after that label and before the back edge that closes the cycle.
    let header = host.find("br label %bb1").expect("loop entry branch");
    let back = host.rfind("br label %bb1").expect("loop back edge");
    assert!(
        header < read && read < back,
        "the clock read belongs inside the loop body, not hoisted above it:\n{host}"
    );
}

#[test]
fn time_bracket_fences_the_tasks_it_brackets() {
    let src = r#"
fn main() {
    8192 -> iota -> xs;
    () -> time -> t0;
    xs -> map { x -> (x * 7) % 13 } -> ys;
    (0, ys) -> fold { acc, y -> acc + y } -> total;
    () -> time -> t1;
    total -> println;
    t1 - t0 -> elapsed;
    elapsed -> println;
}
"#;
    let ll = emit(&lower_src(src)).unwrap();
    assert!(
        ll.contains("declare double @flow_time_ms()"),
        "the clock extern is declared:\n{ll}"
    );
    let host = flow_main(&ll);
    let reads: Vec<usize> = host
        .match_indices("call double @flow_time_ms()")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        reads.len(),
        2,
        "both clock reads land on the host spine, neither CSE'd nor DCE'd:\n{host}"
    );
    // The `i32 <len>` argument of the last `flow_par_wait` before `at`.
    let wait_len = |at: usize| -> u32 {
        let w = host[..at]
            .rfind("call void @flow_par_wait(")
            .expect("a checkpoint wait precedes each clock read");
        host[w..]
            .lines()
            .next()
            .unwrap()
            .rsplit("i32 ")
            .next()
            .unwrap()
            .trim_end_matches(')')
            .parse()
            .expect("wait entry count")
    };
    assert!(
        wait_len(reads[0]) > 0,
        "t0 fences the generation written above the bracket:\n{host}"
    );
    assert!(
        wait_len(reads[1]) > wait_len(reads[0]),
        "t1 fences that PLUS the bracketed kernel — strictly more than t0, \
         which is what makes the interval the work written between them:\n{host}"
    );
    assert!(
        host.find("fsub double").expect("the elapsed subtraction") > reads[1],
        "the clock values' consumer cone stays on the host spine, after both reads:\n{host}"
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

#[test]
fn perf_timing_golden() {
    let opts = EmitOpts {
        perf_timing: true,
        ..EmitOpts::default()
    };
    let parallel = emit_with_opts(&build_example("abs"), &opts).unwrap();
    let main = flow_main(&parallel);
    assert!(
        main.starts_with(
            "define internal void @flow_main() {\nentry:\n  call void @flow_perf_begin()\n"
        ),
        "perf begin is the first entry instruction:\n{main}"
    );
    let finish = main
        .find("call void @flow_par_finish")
        .expect("parallel finish");
    let end = main.find("call void @flow_perf_end()").expect("perf end");
    let ret = main.rfind("ret void").expect("return");
    assert!(finish < end && end < ret, "parallel timer order:\n{main}");
    insta::assert_snapshot!("perf_timing_flow_main", main);

    let sequential = emit_with_opts(&lower_src("fn main() {}\n"), &opts).unwrap();
    let main = flow_main(&sequential);
    assert!(
        !main.contains("flow_par_finish"),
        "sequential fixture:\n{main}"
    );
    assert!(
        main.starts_with(
            "define internal void @flow_main() {\nentry:\n  call void @flow_perf_begin()\n"
        ),
        "sequential perf begin:\n{main}"
    );
    assert!(
        main.ends_with("  call void @flow_perf_end()\n  ret void\n}\n"),
        "sequential perf end:\n{main}"
    );
}

#[test]
fn default_opts_are_byte_identical() {
    let ir = build_example("vector_add");
    assert_eq!(
        emit(&ir).unwrap(),
        emit_with_opts(&ir, &EmitOpts::default()).unwrap()
    );
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

/// plan-s31-target-profiles, the headline property: the KC nest turns itself
/// off **by derivation**, not by a default-off flag.
///
/// `apple-m` differs from `generic` in exactly one machine fact — a 16 MB L2
/// instead of 512 KB — so its k-panel is `(l2/2) / (nc x sizeof) = 4096`, which
/// is deeper than this site's K=300. The gate `site.k > tile_kc` therefore never
/// opens, and asking for `--kc` on this machine is a no-op. That is S29/S30's
/// measured verdict (the nest is a 3x loss on M4 Pro) reproduced as arithmetic.
///
/// The strong form of the assertion: apple-m WITH the nest requested is
/// byte-equal to generic WITHOUT it — the fallback is the real j-outer nest,
/// not a differently-shaped near-miss.
#[test]
fn profile_closes_the_kc_gate_by_derivation() {
    let ir = flow_rewrite::rewrite(lower_src(KC_SRC)).ir;
    let kc_on = |target| {
        emit_with_opts(
            &ir,
            &EmitOpts {
                kc_nest: true,
                contract: true,
                target,
                ..EmitOpts::default()
            },
        )
        .expect("emits")
    };

    // TI x KC = 4 x 128 — the a-panel pack scratch, present only in the nest.
    let apack = "alloca [512 x float], align 64";
    assert!(
        kc_on("generic").contains(apack),
        "generic (512 KB L2) must still open the KC gate at K=300"
    );
    assert!(
        !kc_on("apple-m").contains(apack),
        "apple-m (16 MB L2) must close the KC gate by derivation at K=300"
    );

    let generic_kc_off = emit_with_opts(
        &ir,
        &EmitOpts {
            contract: true,
            ..EmitOpts::default()
        },
    )
    .expect("emits");
    assert_eq!(
        kc_on("apple-m"),
        generic_kc_off,
        "a closed gate must fall back to the j-outer nest byte-for-byte"
    );
}

/// An unknown profile name is an error, never a silent fall back to `generic`:
/// a typo that quietly emits the default numbers is the failure the table
/// exists to remove (plan rule 3).
#[test]
fn unknown_profile_is_an_error_not_a_silent_default() {
    let ir = lower_src(KC_SRC);
    let err = emit_with_opts(
        &ir,
        &EmitOpts {
            target: "apple_m",
            ..EmitOpts::default()
        },
    )
    .expect_err("unknown profile must not emit");
    let EmitError::Internal(msg) = err else {
        panic!("expected Internal, got {err:?}");
    };
    assert!(
        msg.contains("apple_m") && msg.contains("generic, apple-m, zen3"),
        "{msg}"
    );
}
