//! DESIGN §4/§6 item 6: the sepia-at-N perf baseline (ignored-by-default long
//! run). A builder-generated sepia-shaped `map`+`fold` over `[Pixel; N]`, timed
//! native (`-O0` and `-O2`) vs the interpreter. Numbers are printed for STATUS
//! recording (HANDOFF §8 P5).
//!
//! Run: `cargo test -p flow-backend-llvm --test perf_baseline -- --ignored --nocapture`
//!
//! The array is a literal (N `Pair` stores) — Core has no array-fill, so very
//! large N produces a large `.ll`; the `map`/`fold` themselves stay compact
//! (counted loops). N = 262144 is included per DESIGN; drop it if clang time is
//! prohibitive on the runner.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Once;
use std::time::Instant;

use flow_interp::{Outcome, run};
use flow_ir::{CategoryIr, Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value};

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

fn pixel_ty() -> Ty {
    Ty::Struct {
        name: "Pixel".into(),
        fields: vec![
            ("r".into(), Ty::f32()),
            ("g".into(), Ty::f32()),
            ("b".into(), Ty::f32()),
        ],
    }
}

/// Build a `main : Unit → f32` that maps the sepia r-weights over `[Pixel; n]`
/// (gray 200) and folds the total r channel — the sepia perf shape at size `n`.
fn build_sepia(n: usize) -> CategoryIr {
    let mut b = IrBuilder::new();
    let px = pixel_ty();
    let arr = Ty::Array {
        elem: Box::new(px.clone()),
        size: n as u64,
    };

    // map body: Pixel → Pixel (r' = .393r + .769g + .189b, same weights per chan).
    let mapb = b
        .declare(FuncKind::MapBody, "sepia_px", px.clone(), px.clone(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(mapb).unwrap();
        let p = fb.input();
        let r = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let g = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        let bl = fb.proj(p, 2, Dest::Fresh(None), L).unwrap();
        let chan = |fb: &mut flow_ir::FnBuilder, r, g, bl, c0, c1, c2| {
            let k0 = fb.constant(Value::F32(c0), L).unwrap();
            let k1 = fb.constant(Value::F32(c1), L).unwrap();
            let k2 = fb.constant(Value::F32(c2), L).unwrap();
            let t0 = fb
                .binop(Operation::Mul, r, k0, Dest::Fresh(None), L)
                .unwrap();
            let t1 = fb
                .binop(Operation::Mul, g, k1, Dest::Fresh(None), L)
                .unwrap();
            let t2 = fb
                .binop(Operation::Mul, bl, k2, Dest::Fresh(None), L)
                .unwrap();
            let s0 = fb
                .binop(Operation::Add, t0, t1, Dest::Fresh(None), L)
                .unwrap();
            fb.binop(Operation::Add, s0, t2, Dest::Fresh(None), L)
                .unwrap()
        };
        let nr = chan(&mut fb, r, g, bl, 0.393, 0.769, 0.189);
        let ng = chan(&mut fb, r, g, bl, 0.349, 0.686, 0.168);
        let nb = chan(&mut fb, r, g, bl, 0.272, 0.534, 0.131);
        fb.pack_struct(px.clone(), &[nr, ng, nb], Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }

    // fold body: (f32, Pixel) → f32 (acc + px.r).
    let foldb = b
        .declare(
            FuncKind::FoldBody,
            "sum_r",
            Ty::Tuple(vec![Ty::f32(), px.clone()]),
            Ty::f32(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(foldb).unwrap();
        let p = fb.input();
        let acc = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let pix = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        let r = fb.proj(pix, 0, Dest::Fresh(None), L).unwrap();
        fb.binop(Operation::Add, acc, r, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }

    let main = b
        .declare(FuncKind::Named, "main", Ty::Unit, Ty::f32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let c200 = fb.constant(Value::F32(200.0), L).unwrap();
        let one = fb
            .pack_struct(px.clone(), &[c200, c200, c200], Dest::Fresh(None), L)
            .unwrap();
        let elems = vec![one; n];
        let image = fb.pack_array(&elems, Dest::Fresh(None), L).unwrap();
        let mapped = fb.map(mapb, image, Dest::Fresh(None), L).unwrap();
        let seed = fb.constant(Value::F32(0.0), L).unwrap();
        let pair = fb.pack(&[seed, mapped], Dest::Fresh(None), L).unwrap();
        let _ = arr;
        fb.fold(foldb, pair, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    b.seal(main).unwrap()
}

fn rt_lib() -> PathBuf {
    static BUILD: Once = Once::new();
    BUILD.call_once(|| {
        Command::new("cargo")
            .args(["build", "-p", "flow-rt"])
            .status()
            .unwrap();
    });
    PathBuf::from(format!(
        "{}/../../target/debug/libflow_rt.a",
        env!("CARGO_MANIFEST_DIR")
    ))
}

fn compile(ll: &str, opt: &str, dir: &std::path::Path) -> PathBuf {
    let llp = dir.join("p.ll");
    let exe = dir.join(format!("p{opt}"));
    std::fs::write(&llp, ll).unwrap();
    let out = Command::new("clang")
        .arg(opt)
        .arg(&llp)
        .arg(rt_lib())
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "clang {opt}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    exe
}

/// Run `exe` under a raised stack limit. The alloca-slot scheme (BL1) keeps
/// whole array aggregates in the frame — at N = 262144 sepia's `flow_main` holds
/// ~9 MB of `[Pixel; N]` allocas, over the 8 MB default; the perf shape is the
/// only place that exceeds it (examples/testgen arrays are tiny). In-place /
/// heap lowering is the recorded headroom that lifts this ceiling.
fn run_big_stack(exe: &std::path::Path) -> std::process::Output {
    Command::new("bash")
        .args(["-c", r#"ulimit -s hard && exec "$0""#])
        .arg(exe)
        .output()
        .unwrap()
}

#[test]
#[ignore = "long-running perf baseline; run with --ignored --nocapture"]
fn sepia_perf_baseline() {
    // DESIGN's top N (262144) is dropped per this file's own escape hatch: Core
    // has no array-fill, so the input is N literal Pair stores — a ~1M-line
    // module clang -O2 chews >25 min CPU on (observed S13; 65536 is still
    // minutes). An array-fill primitive / heap lowering (recorded headroom)
    // restores large N; 4096 already shows the scale story (interp ~400 ms vs
    // native ~6 ms).
    for &n in &[16usize, 4096] {
        let ir = build_sepia(n);

        // Interp timing (correctness anchor + oracle number).
        let t = Instant::now();
        let rr = run(&ir, u64::MAX);
        let interp_ms = t.elapsed().as_secs_f64() * 1e3;
        assert!(
            matches!(rr.outcome, Outcome::Done(_)),
            "N={n}: interp not Done"
        );

        let ll = flow_backend_llvm::emit(&ir).unwrap();
        let dir = tempfile::tempdir().unwrap();

        let mut native = Vec::new();
        for opt in ["-O0", "-O2"] {
            let exe = compile(&ll, opt, dir.path());
            // Warm once, then time.
            let _ = run_big_stack(&exe);
            let t = Instant::now();
            let r = run_big_stack(&exe);
            let ms = t.elapsed().as_secs_f64() * 1e3;
            assert_eq!(r.status.code(), Some(0), "N={n} {opt}: nonzero exit");
            native.push((opt, ms));
        }
        eprintln!(
            "sepia N={n:>6}: interp {interp_ms:8.2} ms | native -O0 {:8.2} ms | -O2 {:8.2} ms",
            native[0].1, native[1].1
        );
    }
}
