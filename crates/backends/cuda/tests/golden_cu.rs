//! WP2 golden tests: insta snapshots of the emitted `.cu` for the scalar-only
//! examples (`abs` — Phi strict select; `calc` — calls, str globals, chained
//! guards, Div/Mod guards) plus a micro shape pinning the signed Div/Mod
//! guard text, structural assertions on the flagship `abs`, and emit-twice
//! byte equality (L2). WP3 adds the array flagships: `vector_add` (literal
//! uploads, Zip/Map launches, Index readbacks, the Fold kernel) and
//! `zip_demo` (Enumerate) as full-module snapshots, and `sepia` structural
//! pins (the BC8 `__host__ __device__` case, the struct-element literal,
//! the Pixel fold).
//!
//! WP4+WP5 complete the suite: snapshots of every remaining example — the
//! loop flagships `sum_to_n` (scalar carried state) and `fir` (speculative
//! `Index` in the advance cone) plus `sepia`, `fanout`, `pipeline`,
//! `seq_demo` — and the §6.8 pins: two sequential loops in one fn (the S12
//! shape), `exit_only_payload_emitted_once` (llvm `golden_ll.rs:198` ported)
//! with its exit-arm `Print` sibling (countdown), a loop-driven array
//! `Update` program (ADR-0021's motivating `c[t] <- v` pattern, smallest
//! honest version), and emit-twice byte equality over the loop examples.

use flow_backend_cuda::{EmitOpts, emit, emit_with_opts};
use flow_ir::{CategoryIr, Dest, FuncKind, IrBuilder, SourceLoc, Ty};

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

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

const IOTA_FILL_SRC: &str = r#"
fn main() {
    iota(4) -> a;
    iota(4) -> b;
    fill(7, 4) -> sevens;
    (a, b) -> zip -> map { p -> p.0 + p.1 } -> twice;
    (twice, sevens) -> zip -> map { p -> p.0 + p.1 } -> out;
    out[3] -> println;
}
"#;

#[test]
fn golden_examples() {
    for name in [
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
    ] {
        let cu = emit(&build_example(name)).unwrap();
        insta::assert_snapshot!(format!("example_{name}"), cu);
    }
}

/// Signed `Div`/`Mod`: zero-guard host trap + `MIN/-1` value guard
/// (Div ⇒ `INT32_MIN`, Mod ⇒ `0`), plus unsigned-cast wrapping `Add`.
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
    let cu = emit(&lower_src(src)).unwrap();
    insta::assert_snapshot!("micro_arith", cu);
}

#[test]
fn widen_is_scalar_and_emits_host_c_casts() {
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
    let cu = emit(&b.seal(f).unwrap()).unwrap();
    assert!(!cu.contains("__global__"), "{cu}");
    assert!(!cu.contains("arena0"), "{cu}");
    assert_eq!(cu.matches("(int64_t)(").count(), 1, "{cu}");
    assert_eq!(cu.matches("(float)(").count(), 1, "{cu}");
    assert_eq!(cu.matches("(double)(").count(), 2, "{cu}");
}

#[test]
fn golden_iota_fill() {
    let cu = emit(&lower_src(IOTA_FILL_SRC)).unwrap();
    insta::assert_snapshot!("iota_fill", cu);
}

/// Two sequential canonical loops in one fn (S12 P0 shape, the llvm
/// `golden_micro_two_loops` source verbatim) — each its own guard-first
/// quartet, exit attribution by route-feeder membership.
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
    let cu = emit(&lower_src(src)).unwrap();
    insta::assert_snapshot!("micro_two_loops", cu);
}

/// The ADR-0021 motivating pattern, smallest honest version: a loop-carried
/// `mut c` array built by `c[t] <- v` per iteration and returned — the merge
/// is a host handle, the back edge a pointer swap, and each iteration's
/// `Update` is IN PLACE (plan-last-use §2 rule 4: the carried source dies at
/// the update — no fresh buffer, no per-iteration malloc; the launch is
/// unchanged, writing the source handle itself). The escape through the loop
/// exit rides the zone release's range-test veto: the returned handle IS the
/// in-placed, zone-resident init buffer.
const LOOP_UPDATE_SRC: &str = r#"
fn build(n: i32) -> [i32; 4] {
    [0, 0, 0, 0] -> z: [i32; 4];
    mut c: [i32; 4] <- z;
    mut t: i32 <- 0;
    loop {
        (t < n) -> {
            -true-> { c[t] <- t * 10; t + 1 -> t; -> loop; }
            -false-> c -> ret;
        }
    }
}
fn main() {
    4 -> build -> c;
    c[2] -> println;
}
"#;

#[test]
fn golden_micro_loop_update() {
    let cu = emit(&lower_src(LOOP_UPDATE_SRC)).unwrap();
    insta::assert_snapshot!("micro_loop_update", cu);
}

// --- plan-last-use §2 rule 4: the in-place Update (structural pins) ---------

/// Rule 4's escape half: the update's source escapes through a returned
/// product's field (rule 2's "through Pair fields") — the full copy +
/// fresh buffer stay.
#[test]
fn update_escaping_source_keeps_full_copy() {
    let src = r#"
fn mk(x: i32) -> ([i32; 4], [i32; 4]) {
    [1, 2, 3, 4] -> a: [i32; 4];
    mut b: [i32; 4] <- a;
    b[0] <- x;
    (a, b) -> ret;
}
fn main() {
    9 -> mk -> p;
    p.1 -> b;
    b[0] -> println;
}
"#;
    let cu = emit(&lower_src(src)).unwrap();
    let mk_start = cu.find("static FlowProd").expect("mk def:\n{cu}");
    let mk_end = cu[mk_start..].find("\n}\n").map(|e| mk_start + e).unwrap();
    let mk = &cu[mk_start..mk_end];
    // Two zone pointer inits (the literal a AND the update target) — the
    // in-place form would skip the target's.
    assert_eq!(
        mk.matches("= (int32_t*)(arena0 + ").count(),
        2,
        "escape: the update keeps its own fresh buffer:\n{mk}"
    );
    // The launch's out/src are distinct handles.
    let launch = mk.lines().find(|l| l.contains("<<<")).expect("launch");
    let args = launch.split(">>>(").nth(1).expect("launch args");
    let mut operands = args.split(',').map(str::trim);
    assert_ne!(
        operands.next().expect("out"),
        operands.next().expect("src"),
        "out/src stay distinct:\n{mk}"
    );
}

/// Rule 4's ordering half: the source read AFTER the update (a later use of
/// the old value) vetoes the in-place write — the full copy keeps the old
/// buffer's contents intact for the read.
#[test]
fn update_source_read_after_write_keeps_full_copy() {
    let src = r#"
fn main() {
    [1, 2, 3, 4] -> a: [i32; 4];
    mut b: [i32; 4] <- a;
    b[0] <- 9;
    a[1] -> println;
    b[0] -> println;
}
"#;
    let cu = emit(&lower_src(src)).unwrap();
    // The update target gets its own fresh (zone) buffer and the launch's
    // out/src stay distinct handles — the in-place form would have aliased
    // them and corrupted the later `a[1]` read.
    let launch = cu.lines().find(|l| l.contains("<<<")).expect("launch");
    let args = launch.split(">>>(").nth(1).expect("launch args");
    let mut operands = args.split(',').map(str::trim);
    let out = operands.next().expect("out");
    let src = operands.next().expect("src");
    assert_ne!(out, src, "out/src stay distinct:\n{cu}");
    assert!(
        cu.contains(&format!("{out} = (int32_t*)(arena0 + ")),
        "the update keeps its own fresh buffer:\n{cu}"
    );
}

/// Rule 4's consumer half (flow-ir's `last_use_borrowed_init_is_never_dead`
/// pin): a loop carrying a BORROWED init (the fn's array parameter feeds the
/// LoopEnter) never writes in place and is never freed at the back edge —
/// the per-iteration full-copy malloc + the value-guarded per-buffer free
/// stay (today's O(k·n) behavior, the conservative default).
#[test]
fn loop_update_borrowed_init_keeps_full_copy() {
    let src = r#"
fn build(z: [i32; 4], n: i32) -> [i32; 4] {
    mut c: [i32; 4] <- z;
    mut t: i32 <- 0;
    loop {
        (t < n) -> {
            -true-> { c[t] <- t * 10; t + 1 -> t; -> loop; }
            -false-> c -> ret;
        }
    }
}
fn main() {
    ([1, 2, 3, 4], 4) -> build -> c;
    c[2] -> println;
}
"#;
    let cu = emit(&lower_src(src)).unwrap();
    let build_start = cu.find("static int32_t* fn").expect("build def:\n{cu}");
    let build_end = cu[build_start..]
        .find("\n}\n")
        .map(|e| build_start + e)
        .unwrap();
    let build = &cu[build_start..build_end];
    // The borrowed init vetoes the in-place update: the per-iteration
    // full-copy buffer stays (one cudaMalloc inside the loop body).
    let brk = build.find(") { break; }").unwrap();
    let malloc = build
        .find("cu_check(cudaMalloc((void**)&o")
        .expect("per-buffer malloc");
    assert!(brk < malloc, "the full-copy malloc stays in-loop:\n{build}");
    // The update target stays a registered allocation: its value-guarded
    // per-buffer free at fn exit is the tier-1 escape guard.
    assert_eq!(build.matches("!= o1) {").count(), 1, "{build}");
    // No back-edge free either (the plan says borrowed): no cudaFree inside
    // the loop body.
    let loop_close = build[brk..].find("\n  }\n").map(|i| brk + i).unwrap();
    assert!(
        !build[brk..loop_close].contains("cudaFree"),
        "no back-edge free where the plan says borrowed:\n{build}"
    );
}

/// The map-in-loop carried shape — suggestions.md #2's back-edge freeing:
/// the carried buffer is NOT produced by an Update, so it keeps its
/// per-iteration malloc (a registered cone-site buffer), but the back edge
/// frees the merge's OUTGOING handle under the pointer-value init guard —
/// the per-iteration buffers no longer accumulate to fn exit. The plan's
/// half: `dead_after(merge, LoopBack)` (the guard cond reads only `t`) and
/// an owned init literal; the consumer's half: the producer is a registered
/// allocation and the init is not borrowed. The producer's final instance
/// still rides the fn-exit escape value guard (the second line of defense).
#[test]
fn map_in_loop_back_edge_frees_outgoing_handle() {
    let src = r#"
fn build(n: i32) -> [i32; 4] {
    [1, 2, 3, 4] -> z: [i32; 4];
    mut c: [i32; 4] <- z;
    mut t: i32 <- 0;
    loop {
        (t < n) -> {
            -true-> { c -> map { x -> x + 1 } -> c; t + 1 -> t; -> loop; }
            -false-> c -> ret;
        }
    }
}
fn main() {
    4 -> build -> c;
    c[0] -> println;
}
"#;
    let cu = emit(&lower_src(src)).unwrap();
    let build_start = cu.find("static int32_t* fn").expect("build def:\n{cu}");
    let build_end = cu[build_start..]
        .find("\n}\n")
        .map(|e| build_start + e)
        .unwrap();
    let build = &cu[build_start..build_end];
    for needle in [
        // The per-iteration map buffer (registered) stays — this class is
        // not in-place-able, it is FREED instead.
        "cu_check(cudaMalloc((void**)&o9, sizeof(int32_t) * 4ULL), \"cudaMalloc(o9)\");",
        // The back-edge free: the merge's outgoing handle under the
        // pointer-value init guard (the first iteration holds the init's
        // zone buffer — never a registered allocation — so the guard skips
        // it; later iterations hold the previous producer buffer).
        "if (o4.f0 != o3.f0) {",
        "cu_check(cudaFree(o4.f0), \"cudaFree(o4.f0)\");",
        // The final instance is still freed at fn exit under the escape
        // value guard (the emitted-text second line of defense, unchanged).
        "if (o9 != o1) {",
    ] {
        assert!(build.contains(needle), "missing `{needle}` in:\n{build}");
    }
    // Sequencing: the free sits after the guard's break and before the
    // back-edge swap — the outgoing handle dies exactly at the swap.
    let brk = build.find(") { break; }").unwrap();
    let free = build.find("cudaFree(o4.f0)").unwrap();
    let swap = build.find("o4 = o13.f0;").expect("the back-edge swap");
    assert!(
        brk < free && free < swap,
        "the back-edge free precedes the swap:\n{build}"
    );
    // One free per iteration replaces accumulate-to-exit: exactly one
    // cudaFree inside the loop body, and the fn-exit frees are unchanged.
    let loop_close = build[swap..].find("\n  }\n").map(|i| swap + i).unwrap();
    assert_eq!(
        build[brk..loop_close].matches("cu_check(cudaFree(").count(),
        1,
        "{build}"
    );
    // The freed shape is deterministic (L2): same IR → byte-identical text.
    assert_eq!(
        cu,
        emit(&lower_src(src)).unwrap(),
        "back-edge freeing is byte-deterministic"
    );
}
/// a body-local array whose only use is the update skips the per-thread
/// copy — the store lands in the source array directly, and the target
/// declares no produced local.
#[test]
fn twin_update_dead_source_skips_copy() {
    let src = r#"
fn main() {
    [1, 2, 3, 4] -> arr: [i32; 4];
    arr -> map { x ->
        mut local: [i32; 2] <- [10, 20];
        local[0] <- x;
        local[0]
    } -> out;
    out[2] -> println;
}
"#;
    let cu = emit(&lower_src(src)).unwrap();
    let twin_start = cu.find("static __device__").expect("a device twin:\n{cu}");
    let twin_end = cu[twin_start..]
        .find("\n}\n")
        .map(|e| twin_start + e)
        .unwrap();
    let twin = &cu[twin_start..twin_end];
    // The twin's update wrote the source array directly: the bounds guard
    // and the direct store stay, the per-thread copy loop is gone — the
    // twin has no for-loop at all in this shape.
    assert!(twin.contains("< 0 ||"), "{twin}");
    assert!(!twin.contains("for (unsigned long long"), "{twin}");
    let store = twin
        .lines()
        .find(|l| l.contains("[(unsigned long long)t") && l.contains("] = "))
        .expect("the direct store:\n{twin}");
    assert!(!store.contains("].f"), "scalar element store:\n{twin}");
}

/// The flagship's load-bearing lines, pinned robustly (snapshot-orthogonal).
#[test]
fn abs_structural_lines() {
    let cu = emit(&build_example("abs")).unwrap();
    for needle in [
        "#include <cstdint>",
        "[[noreturn]] void flow_trap(uint32_t kind);",
        "static unsigned int* d_trap = nullptr;",
        // The negated arm: unsigned-cast wrapping multiply (BC2).
        "(int32_t)((uint32_t)",
        // Phi: both arms into temporaries, then the strict select (BC7).
        "? t0 : t1;",
        // Direct C++ call to the abs fn + its host-only definition.
        "static int32_t fn0(int32_t in)",
        "flow_print_i32(",
        "  trap_init();",
        "int main() {",
        "  return 0;",
    ] {
        assert!(cu.contains(needle), "missing `{needle}` in:\n{cu}");
    }
    // No kernels, no launches: nothing device-side but the trap flag.
    assert!(!cu.contains("__global__"), "{cu}");
    assert!(!cu.contains("<<<"), "{cu}");
}

/// sum_to_n's WP4 quartet, pinned robustly (snapshot-orthogonal): init copy,
/// `while (true)`, the guard-first break, the back edge, the exit copy.
#[test]
fn sum_to_n_structural_lines() {
    let cu = emit(&build_example("sum_to_n")).unwrap();
    for needle in [
        // Entry: init → merge local, then the host-driven loop.
        "while (true) {",
        // Guard-first: the decide cone's cond guards the advance cone.
        "if (!",
        ") { break; }",
        // The exit copy and the scalar return.
        "return o1;",
    ] {
        assert!(cu.contains(needle), "missing `{needle}` in:\n{cu}");
    }
    // Exactly one loop in the module.
    assert_eq!(cu.matches("while (true) {").count(), 1, "{cu}");
    // The break precedes the back edge; the back edge precedes the exit copy.
    let brk = cu.find(") { break; }").unwrap();
    let back = cu.find("o13.f0;").expect("back edge in:\n{cu}");
    let close = cu[back..].find("\n  }\n").map(|i| back + i).unwrap();
    let exit = cu.find("o1 = o14.f0;").expect("exit copy in:\n{cu}");
    assert!(brk < back, "guard-first: break before back edge:\n{cu}");
    assert!(back < close, "back edge inside the loop body:\n{cu}");
    assert!(close < exit, "exit copy after the loop:\n{cu}");
    // Scalar program: no kernels, no launches.
    assert!(!cu.contains("__global__"), "{cu}");
    assert!(!cu.contains("<<<"), "{cu}");
}

/// The loop-driven `Update` program's load-bearing shapes, pinned robustly:
/// the in-place update (plan-last-use §2 rule 4) — NO per-iteration malloc
/// in the loop body, the target handle IS the source handle (`o12 = o5;`),
/// the element-write kernel launch + its §3 trap check unchanged (out == src
/// is race-free: disjoint per-thread indices), the back-edge pointer swap —
/// and the zone release's range-test veto as the escape guard, now
/// load-bearing: the returned handle is the in-placed init's zone buffer, so
/// the veto fires and the caller inherits the bounded-leak duty (§2
/// amendment (ii)'s recorded shape).
#[test]
fn loop_update_structural_lines() {
    let cu = emit(&lower_src(LOOP_UPDATE_SRC)).unwrap();
    for needle in [
        // The BC5 element-write kernel exists for the Update site (its text
        // is unchanged — out == src writes each index at most once).
        "out[i] = ((int64_t)i == idx) ? val : src[i];",
        // Its launch sits inside the loop, with the §3 check after it.
        "trap_check_after_launch();",
        // Rule 4's in-place update: the target handle is the source handle —
        // no fresh buffer is allocated for the update.
        "o12 = o5;",
        // The loop-carried escape, tier 2 (#18, plan rule 3): the zone
        // release's range-test veto — the returned handle pointing into the
        // arena pins the whole zone.
        "bool escaped0 = ((char*)o1 >= (char*)arena0 && (char*)o1 < (char*)arena0 + 256ULL);",
        "if (!escaped0) {",
    ] {
        assert!(cu.contains(needle), "missing `{needle}` in:\n{cu}");
    }
    // The Update launch is inside the advance cone: after the guard's break.
    let brk = cu.find(") { break; }").unwrap();
    let launch = cu.find("<<<").expect("a kernel launch in:\n{cu}");
    assert!(
        brk < launch,
        "guard-first: the Update launch is post-guard:\n{cu}"
    );
    // Rule 4's payoff: NO cudaMalloc inside build at all for the update —
    // the per-iteration malloc+copy is gone (the one remaining malloc is the
    // fn zone's); the in-placed target is never a registered allocation, so
    // build frees nothing per buffer either.
    let build_start = cu.find("static int32_t* fn").expect("build def:\n{cu}");
    let build_end = cu[build_start..]
        .find("\n}\n")
        .map(|e| build_start + e)
        .unwrap();
    let build = &cu[build_start..build_end];
    assert_eq!(
        build.matches("cu_check(cudaMalloc((void**)&o").count(),
        0,
        "in place: no fresh buffer for the cone Update:\n{build}"
    );
    assert!(
        !build.contains("cudaFree(o12)"),
        "the in-placed target is not a registered allocation:\n{build}"
    );
    // The one launch passes the source handle as out AND src (o12 == o5).
    assert!(
        build.contains(">>>(o12, o5, (int64_t)o6, o10, d_trap);"),
        "{build}"
    );
    // Every remaining free in build is the zone release under the range-test
    // veto (the returned handle points into the arena this time).
    for line in build.lines().filter(|l| l.contains("cudaFree")) {
        assert!(
            line.contains("cu_check(cudaFree(arena0)"),
            "guarded free: {line}\n{cu}"
        );
    }
}

/// S13 orchestrator-review regression (llvm `golden_ll.rs:198` ported): an
/// exit-only computed payload (`acc + 12345`, consumed only by the exit
/// route) belongs to the decide cone but lives outside the loop SCC. The
/// walk must not re-emit it after the loop — the constant appears exactly
/// once in the module.
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
    let cu = emit(&lower_src(src)).unwrap();
    assert_eq!(
        cu.matches("12345").count(),
        1,
        "exit-only payload must be emitted exactly once (driver-owned):\n{cu}"
    );
}

/// The exit-arm `Print` sibling (the OF-1/R1 miscompile class): countdown's
/// `n -> println` rides the token chain into the decide cone — it must be
/// emitted exactly once (a double emission is a DOUBLE side effect per
/// iteration) and it must sit inside the loop (it prints the final 0 on the
/// exit step — ADR-0016, the interp oracle's `5\n4\n3\n2\n1\n0\n`).
#[test]
fn exit_arm_print_emitted_once() {
    // interp acceptance.rs's COUNTDOWN_SRC verbatim.
    let src = r#"
fn countdown(mut n: i32) {
    loop {
        n -> println;
        (n > 0) -> {
            -true-> { n - 1 -> n; -> loop; }
            -false-> -> ret;
        }
    }
}
fn main() { 5 -> countdown; }
"#;
    let cu = emit(&lower_src(src)).unwrap();
    assert_eq!(
        cu.matches("flow_print_i32(o").count(),
        1,
        "the exit-arm print must be emitted exactly once:\n{cu}"
    );
    // The one emission is inside the while body (runs on the exit step).
    let loop_start = cu.find("while (true) {").unwrap();
    let brk = cu.find(") { break; }").unwrap();
    let print = cu.find("flow_print_i32(o").unwrap();
    assert!(
        loop_start < print && print < brk,
        "the decide-cone print precedes the guard:\n{cu}"
    );
}

#[test]
fn determinism_emit_twice_byte_equal() {
    for name in [
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
    ] {
        let a = emit(&build_example(name)).unwrap();
        let b = emit(&build_example(name)).unwrap();
        assert_eq!(a, b, "{name}: emit is not byte-deterministic");
    }
}

/// sepia's load-bearing WP3 shapes, pinned robustly (snapshot-orthogonal):
/// the BC8 `__host__ __device__` single definition (clamp), the struct-
/// element literal upload, the Pixel map/fold kernels. #14's trim: sepia's
/// whole call graph is trap-free (no Div/Mod/Index/Update anywhere), so no
/// trap parameters, no `d_trap` arguments, and no post-launch readbacks —
/// the launches and their geometry are unchanged.
#[test]
fn sepia_structural_lines() {
    let cu = emit(&build_example("sepia")).unwrap();
    for needle in [
        // clamp: one two-site definition — trap-free (#14), no trap pointer.
        "static __host__ __device__ float fn0(FlowProd_float_float_float in)",
        // The Pixel literal: computed elements ⇒ plain local data array +
        // one H→D memcpy (§2, BC11).
        "FlowProd_float_float_float lit0[16] = { o2,",
        "cudaMemcpyHostToDevice",
        // The map kernel calls the body per element; the fold kernel is the
        // single-thread oracle loop (BC4) — both trap-free, no trap args.
        "out[i] = fn2(in[i]);",
        "acc = fn3(pair);",
        // Scalar fold acc: 1-cell buffer (a fn-zone member — #18's arena
        // pointer init) + D→H readback (§2 item 6).
        "= (float*)(arena0 + ",
        "\"cudaMemcpy(fold)\"",
    ] {
        assert!(cu.contains(needle), "missing `{needle}` in:\n{cu}");
    }
    // The map body calls clamp — no trap pointer threaded anywhere (#14:
    // sepia has no trap-capable fn or site, so the readback never follows
    // a launch; the prelude's [[maybe_unused]] definition always remains).
    assert!(cu.contains("fn0(o15);"), "{cu}");
    assert!(!cu.contains("trap_check_after_launch();"), "{cu}");
    // No twin needed (all bodies are pure-scalar): no __device__-only fns.
    assert!(!cu.contains("static __device__"), "{cu}");
}

/// ADR-0027's acceptance shape (the S16 bench note's finding #3, closed):
/// the one-kernel matmul — an outer map over the 16 cells with the inner
/// fold over k inside the outer body's `__device__` twin, reading the
/// captured operand matrices through the kernel's extra parameters.
const ONE_KERNEL_MATMUL_SRC: &str = r#"
fn main() {
    [ 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0] -> a;
    [ 17.0, 18.0, 19.0, 20.0, 21.0, 22.0, 23.0, 24.0, 25.0, 26.0, 27.0, 28.0, 29.0, 30.0, 31.0, 32.0] -> b;
    [0, 1, 2, 3] -> krange;
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0] -> seed;
    seed -> enumerate -> map { p ->
        p.0 -> t;
        t / 4 -> i;
        t % 4 -> j;
        (0.0, krange) -> fold { acc, k -> acc + a[i * 4 + k] * b[k * 4 + j] }
    } -> c;
    c[0] -> println;
    c[15] -> println;
}
"#;

/// ADR-0027's acceptance shape (the S16 bench note's finding #3, closed):
/// the one-kernel matmul. The outer map over the 16 cells is ONE
/// elementwise `__global__`; the fold over k is a per-thread loop INSIDE
/// the outer body's `__device__` twin, reading the captured operand
/// matrices through the kernel's extra parameters. Emitter-quality passes
/// (#17/#12) shrink the module further: main's two Index sites share ONE
/// deduplicated kernel (#17 — four structurally identical Index kernels
/// collapse to the first site's name), and the two Twin bodies get NO host
/// definition at all — nothing on the host path calls them (#12), so their
/// launch-form sites (the inner fold's host-side kernel) emit nothing.
#[test]
fn golden_one_kernel_matmul() {
    let cu = emit(&lower_src(ONE_KERNEL_MATMUL_SRC)).unwrap();

    // Exactly ONE __global__ for the outer map: the elementwise kernel with
    // the captured arrays (krange, b, a) as extra parameters, positionally
    // after `in` — and **no trap pointer** (S20c: the fold's Index reads
    // prove in-bounds via the capture-range flow, and the map body's `t / 4`
    // / `t % 4` are #13-safe constant divisions — the whole chain is
    // trap-free).
    assert!(
        cu.contains(
            "__global__ void k0_1(double* out, FlowProd_int32_t_double* in, int32_t* cap0, double* cap1, double* cap2) {"
        ),
        "{cu}"
    );
    assert_eq!(
        cu.matches("= d_fn2(pair);").count(),
        1,
        "exactly one kernel calls the outer body twin:\n{cu}"
    );
    // The kernel's body call: the captures lead the body-input assembly,
    // then the enumerated (t, seed) element.
    for needle in [
        "pair.f0 = cap0;",
        "pair.f1 = cap1;",
        "pair.f2 = cap2;",
        "pair.f3 = in[i];",
        "out[i] = d_fn2(pair);",
    ] {
        assert!(cu.contains(needle), "missing `{needle}` in:\n{cu}");
    }
    // The launch passes the captured buffers positionally after the mapped
    // array (out, in, krange, a, b — first-use order, ADR-0027; no `d_trap`).
    assert!(
        cu.contains(
            "k0_1<<<(unsigned int)((16ULL + 255ULL) / 256ULL), 256>>>(o8, o6, o4, o2, o3);"
        ),
        "{cu}"
    );

    // The inner fold is a per-thread loop INSIDE the outer body's twin
    // (d_fn2) — the S16 acceptance shape. (The ` {` suffix selects the
    // definition over the prototype.) The twin carries no trap param and no
    // per-step check: the fold body chain is proven trap-free (S20c).
    let d_fn2_start = cu
        .find("static __device__ double d_fn2(FlowProd_int32_tp_doublep_doublep_FlowProd_int32_t_double in) {")
        .expect("d_fn2 def");
    let d_fn2_end = cu[d_fn2_start..].find("\n}\n").unwrap() + d_fn2_start;
    let d_fn2 = &cu[d_fn2_start..d_fn2_end];
    assert!(
        d_fn2.contains("for (unsigned long long t1 = 0; t1 < 4ULL; t1++) {"),
        "{d_fn2}"
    );
    assert!(d_fn2.contains("t0 = d_fn1(pair);"), "{d_fn2}");
    assert!(!d_fn2.contains("trap"), "{d_fn2}");
    // #13: the `t / 4` / `t % 4` guards are elided (constant non-zero
    // divisor) — plain divisions, no `*trap = 1u` anywhere in the twin.
    assert!(d_fn2.contains("o8 = o7.f0 / o7.f1;"), "{d_fn2}");
    assert!(d_fn2.contains("o10 = o9.f0 % o9.f1;"), "{d_fn2}");
    assert!(!d_fn2.contains("*trap = 1u"), "{d_fn2}");
    // The whole module is exactly THREE kernels: the enumerate, the outer
    // map, and ONE deduplicated Index kernel (#17 — main's two Index sites
    // and the Twin bodies' host-side Index sites shared one structural
    // shape; the dead twins' sites emit nothing, #12).
    assert_eq!(
        cu.matches("__global__ void").count(),
        3,
        "enumerate + map + one deduped index kernel:\n{cu}"
    );
    // #12: neither Twin body has a host caller, so neither gets a host
    // definition — no fn1/fn2 prototypes or definitions, and no host-side
    // fold kernel (k2_0) anywhere in the module.
    assert!(!cu.contains("static double fn"), "{cu}");
    assert!(!cu.contains("k2_0"), "{cu}");
    // #14 + S20c: EVERY kernel here is trap-free — the enumerate was already
    // free; the map kernel goes free (the fold's Index reads prove via the
    // capture-range flow, the map body's `t / 4` / `t % 4` are #13-safe
    // constant divisions); the two Index readbacks are constants — proven.
    assert!(
        cu.contains("__global__ void k0_0(FlowProd_int32_t_double* out, double* a) {"),
        "{cu}"
    );
    assert!(
        cu.contains("k0_0<<<(unsigned int)((16ULL + 255ULL) / 256ULL), 256>>>(o6, o5);"),
        "{cu}"
    );
    // The device path out of main is exactly enumerate + map + two index
    // readbacks (both launching the one deduped Index kernel) — the fold
    // contributes no launch. The S20 refinement + capture-range flow splits
    // the §3 checks honestly: every launch is trap-free (enumerate; the map
    // — fold Index reads proven + #13-safe divisions; both readbacks —
    // constants proven), so NO launch carries `d_trap` and NO readback
    // follows any launch.
    let main_start = cu.find("static void flow_main() {").unwrap();
    let main_end = cu[main_start..].find("\n}\n").unwrap() + main_start;
    let main_def = &cu[main_start..main_end];
    assert_eq!(main_def.matches("<<<").count(), 4, "{main_def}");
    assert_eq!(main_def.matches("k0_2<<<").count(), 2, "{main_def}");
    assert_eq!(main_def.matches(", d_trap);").count(), 0, "{main_def}");
    assert_eq!(
        main_def.matches("trap_check_after_launch();").count(),
        0,
        "{main_def}"
    );
    // The proven readback kernel (k0_2): no §3 bounds guard, no trap
    // parameter — the plain global read stays, verbatim.
    let k2_start = cu.find("__global__ void k0_2(").expect("index kernel");
    let k2_end = cu[k2_start..].find("\n}\n").unwrap() + k2_start;
    let k2 = &cu[k2_start..k2_end];
    assert!(
        k2.starts_with("__global__ void k0_2(double* result, double* arr, int64_t idx) {"),
        "{k2}"
    );
    assert!(!k2.contains("trap"), "{k2}");
    assert!(
        k2.contains("*result = arr[(unsigned long long)idx];"),
        "{k2}"
    );

    // The index arithmetic reads the captured `a`/`b` on device: the inner
    // fold body (d_fn1) — its bounds guards are ELIDED (S20c: both reads
    // prove in-bounds via the capture-range flow), the plain global loads
    // stay verbatim, and the twin carries no trap param. (First-use capture
    // order, ADR-0027: (a, i, b, j, acc, elem).)
    let d_fn1_start = cu
        .find("static __device__ double d_fn1(FlowProd_doublep_int32_t_doublep_int32_t_double_int32_t in) {")
        .expect("d_fn1 def");
    let d_fn1_end = cu[d_fn1_start..].find("\n}\n").unwrap() + d_fn1_start;
    let d_fn1 = &cu[d_fn1_start..d_fn1_end];
    assert!(!d_fn1.contains("trap"), "{d_fn1}");
    for needle in [
        "o13 = o2[(unsigned long long)t0];", // a[i * 4 + k]
        "o19 = o4[(unsigned long long)t1];", // b[k * 4 + j]
    ] {
        assert!(d_fn1.contains(needle), "missing `{needle}` in:\n{d_fn1}");
    }
}

// --- plan-smart-arenas §7: the structural perf gates (#18) -------------------

/// Whole-module `cudaMalloc` count EXCLUDING the prelude's `d_trap` pair
/// (plan §7's counting convention).
fn arena_mallocs(cu: &str) -> usize {
    cu.matches("cu_check(cudaMalloc(").count() - 1
}

/// Whole-module `cudaFree` count EXCLUDING the prelude's `d_trap` pair.
fn arena_frees(cu: &str) -> usize {
    cu.matches("cu_check(cudaFree(").count() - 1
}

/// plan-smart-arenas §7's gates, counted on emitted text (deterministic by
/// L2): a fn's non-loop-cone buffer cudaMallocs collapse into ONE arena
/// malloc, the per-buffer frees into one zone release. Loop-cone sites stay
/// per-buffer in v1.0 (the honest debt, pinned below).
#[test]
fn arena_gates_plan_section_7() {
    // The marathon's flagship shape: 8 fn-scope buffers (4 literals +
    // enumerate + map + 2 readback cells) → ONE zone (2 KiB: 8 × 256 B
    // slots). §7's gate was ≤3/≤2 (its "before" of 12 predates W1 — the W1
    // text was 8 + d_trap); the mechanism lands at 1/1.
    let cu = emit(&lower_src(ONE_KERNEL_MATMUL_SRC)).unwrap();
    assert_eq!(arena_mallocs(&cu), 1, "{cu}");
    assert_eq!(arena_frees(&cu), 1, "{cu}");
    assert!(
        cu.contains("cu_check(cudaMalloc((void**)&arena0, 2048ULL), \"cudaMalloc(arena0)\");"),
        "{cu}"
    );
    // Every member gets its compile-time 256 B-aligned pointer init…
    assert_eq!(cu.matches("= (double*)(arena0 + ").count(), 6, "{cu}");
    assert_eq!(
        cu.matches("= (FlowProd_int32_t_double*)(arena0 + ").count(),
        1,
        "{cu}"
    );
    assert_eq!(cu.matches("= (int32_t*)(arena0 + ").count(), 1, "{cu}");
    // …and W1's launch count is unchanged. The §3 checks drop honestly:
    // every launch is trap-free (the two constant readbacks prove (S20);
    // the map launch too — the fold's captured-index reads prove via the
    // S20c capture-range flow and its `t / 4` / `t % 4` are #13-safe
    // constant divisions).
    let main_start = cu.find("static void flow_main() {").unwrap();
    let main_end = cu[main_start..].find("\n}\n").unwrap() + main_start;
    let main_def = &cu[main_start..main_end];
    assert_eq!(main_def.matches("<<<").count(), 4, "{main_def}");
    assert_eq!(
        main_def.matches("trap_check_after_launch();").count(),
        0,
        "{main_def}"
    );

    // vector_add: main's 7 buffers → one zone (fn1/fn2 are
    // __host__ __device__ scalars — no buffers, no zones). Gate was ≤3/≤2.
    let cu = emit(&build_example("vector_add")).unwrap();
    assert_eq!(arena_mallocs(&cu), 1, "{cu}");
    assert_eq!(arena_frees(&cu), 1, "{cu}");

    // fir: main's two literals → main's zone; fn0's two Index readback
    // cells are advance-cone sites → per-buffer in v1.0 (plan §5's recorded
    // scoping). DEVIATION from §7's fir row (≤3/≤2): the recorded mechanism
    // lands at 3/3 (4/4 with d_trap) — the §7 cell was calibrated without
    // noticing both of fir's cells are cone sites. Recorded in the plan's
    // Status line and suggestions #18.
    let cu = emit(&build_example("fir")).unwrap();
    assert_eq!(arena_mallocs(&cu), 3, "{cu}");
    assert_eq!(arena_frees(&cu), 3, "{cu}");

    // micro_loop_update: rule 4's in-place update killed the cone site's
    // per-buffer malloc AND free — what remains is build's zone (the
    // top-level literal, whose buffer the in-placed update now writes and
    // returns) + main's zone (the readback cell): 2/2 excluding d_trap =
    // 3/3 with it. (The v1.1 debt note moved: the cone Update site no
    // longer allocates at all; its arena offset stays reserved-but-unused —
    // the recorded v1 simplification.)
    let cu = emit(&lower_src(LOOP_UPDATE_SRC)).unwrap();
    assert_eq!(arena_mallocs(&cu), 2, "{cu}");
    assert_eq!(arena_frees(&cu), 2, "{cu}");
    // The zero-iteration contract: every pointer local still declares
    // nullptr-initialized, zone member or not.
    assert!(cu.contains("int32_t* o12 = nullptr;"), "{cu}");
}

#[test]
fn iota_fill_are_arena_members() {
    let cu = emit(&lower_src(IOTA_FILL_SRC)).unwrap();
    assert_eq!(arena_mallocs(&cu), 1, "{cu}");
    assert_eq!(arena_frees(&cu), 1, "{cu}");
    assert!(
        cu.contains("cu_check(cudaMalloc((void**)&arena0, 2048ULL), \"cudaMalloc(arena0)\");"),
        "{cu}"
    );
    assert_eq!(cu.matches("= (int32_t*)(arena0 + ").count(), 6, "{cu}");
    assert_eq!(
        cu.matches("= (FlowProd_int32_t_int32_t*)(arena0 + ")
            .count(),
        2,
        "{cu}"
    );
}

/// The arena swap preserves L2: same IR → byte-identical text, zones
/// included (the assignment walk is topo-ordered, the offsets map is
/// lookup-only).
#[test]
fn arena_emit_twice_byte_equal() {
    for name in ["vector_add", "fir", "sepia"] {
        let a = emit(&build_example(name)).unwrap();
        let b = emit(&build_example(name)).unwrap();
        assert_eq!(a, b, "{name}: emit is not byte-deterministic");
    }
    let a = emit(&lower_src(ONE_KERNEL_MATMUL_SRC)).unwrap();
    let b = emit(&lower_src(ONE_KERNEL_MATMUL_SRC)).unwrap();
    assert_eq!(a, b, "one-kernel matmul: emit is not byte-deterministic");
}

// --- suggestions.md #19a: kernel-time instrumentation ------------------------

/// With `perf_timing` on, every launch site is wrapped in CUDA events and
/// the machine-readable `FLOW_PERF` lines print; with it off (the `emit`
/// default) the text contains none of that machinery, and the launch /
/// trap-check counts are identical either way (the §3 convention is
/// untouched — the stop event is recorded before the trap check).
#[test]
fn perf_timing_instruments_launches() {
    let ir = build_example("vector_add");
    let on = emit_with_opts(&ir, &EmitOpts { perf_timing: true }).unwrap();
    for needle in [
        // One event pair per launch site, created once at fn entry.
        "cudaEvent_t fev0_start, fev0_stop;",
        "cudaEvent_t fev4_start, fev4_stop;",
        "cu_check(cudaEventCreate(&fev0_start), \"cudaEventCreate\");",
        // The launch wrap: Record(start) … launch … Record(stop) → Sync →
        // ElapsedTime → the FLOW_PERF line.
        "cu_check(cudaEventRecord(fev0_start), \"cudaEventRecord\");",
        "cu_check(cudaEventRecord(fev0_stop), \"cudaEventRecord\");",
        "cu_check(cudaEventSynchronize(fev0_stop), \"cudaEventSynchronize\");",
        "cu_check(cudaEventElapsedTime(&flow_perf_ms, fev0_start, fev0_stop), \"cudaEventElapsedTime\");",
        "flow_perf_total += flow_perf_ms;",
        "printf(\"FLOW_PERF launch=k0_0 ms=%.4f\\n\", flow_perf_ms);",
        // The per-fn total at fn end, then the destroys.
        "printf(\"FLOW_PERF total ms=%.4f\\n\", flow_perf_total);",
        "cu_check(cudaEventDestroy(fev0_stop), \"cudaEventDestroy\");",
    ] {
        assert!(on.contains(needle), "missing `{needle}` in:\n{on}");
    }
    // vector_add's 5 launch sites (zip, map, two deduped Index, fold) → 5
    // event pairs and 5 FLOW_PERF launch lines; one total line (only
    // flow_main launches — fn1/fn2 are pure-scalar __host__ __device__).
    assert_eq!(on.matches("cudaEventCreate(&fev").count(), 10, "{on}");
    assert_eq!(on.matches("FLOW_PERF launch=").count(), 5, "{on}");
    assert_eq!(on.matches("FLOW_PERF total ms=").count(), 1, "{on}");
    // The event stop is recorded BEFORE the trap check where the check
    // rides. vector_add's own sites are all trap-free now (S20: the two
    // constant readbacks prove), so the ordering pins on an unproven
    // readback (`17 % 5` is statically [0,4] ⊄ [0,4)): its Index site
    // keeps the §3 convention, and the stop event precedes the check.
    let idx_ir =
        lower_src("fn main() {\n    [1, 2, 3, 4] -> a: [i32; 4];\n    a[17 % 5] -> println;\n}\n");
    let idx_on = emit_with_opts(&idx_ir, &EmitOpts { perf_timing: true }).unwrap();
    let rec = idx_on.find("cudaEventRecord(fev0_stop)").unwrap();
    let chk = idx_on.find("trap_check_after_launch();").unwrap();
    assert!(rec < chk, "stop recorded before the trap check:\n{idx_on}");

    // Default off: no event machinery anywhere, and `emit` is byte-equal
    // to the defaulted options (the differential/goldens ride the default).
    let off = emit(&ir).unwrap();
    assert!(!off.contains("cudaEvent"), "{off}");
    assert!(!off.contains("FLOW_PERF"), "{off}");
    assert_eq!(
        off,
        emit_with_opts(&ir, &EmitOpts::default()).unwrap(),
        "emit == emit_with_opts(default)"
    );
    // Launch and trap-check counts are option-invariant.
    assert_eq!(
        on.matches("<<<").count(),
        off.matches("<<<").count(),
        "launch count unchanged"
    );
    assert_eq!(
        on.matches("trap_check_after_launch();").count(),
        off.matches("trap_check_after_launch();").count(),
        "trap-check count unchanged"
    );
    // Instrumentation is deterministic (L2) — the event ordinals ride
    // collect_sites order.
    assert_eq!(
        on,
        emit_with_opts(&ir, &EmitOpts { perf_timing: true }).unwrap(),
        "instrumented emit is byte-deterministic"
    );
}

/// A cone (in-loop) launch is instrumented too — per EXECUTION, so a loop
/// prints one FLOW_PERF line per iteration — and the fn-scope events are
/// still created once, at fn entry (not per iteration).
#[test]
fn perf_timing_instruments_cone_launches_once_created() {
    let on = emit_with_opts(&lower_src(LOOP_UPDATE_SRC), &EmitOpts { perf_timing: true }).unwrap();
    let build_start = on.find("static int32_t* fn").expect("build def:\n{on}");
    let build_end = on[build_start..]
        .find("\n}\n")
        .map(|e| build_start + e)
        .unwrap();
    let build = &on[build_start..build_end];
    // One site (the advance-cone Update): one pair, created at fn top,
    // destroyed at fn exit — the Record/print lines sit inside the loop.
    assert_eq!(build.matches("cudaEventCreate(&fev").count(), 2, "{build}");
    assert_eq!(build.matches("FLOW_PERF launch=").count(), 1, "{build}");
    let brk = build.find(") { break; }").unwrap();
    let rec = build.find("FLOW_PERF launch=").unwrap();
    assert!(
        brk < rec,
        "the cone launch's FLOW_PERF line is in-loop:\n{build}"
    );
    assert!(build.contains("FLOW_PERF total ms="), "{build}");
}
