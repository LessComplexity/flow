//! Per-stage timing for the front end: lex+parse -> lower -> rewrite -> emit,
//! against the cost of actually compiling the emitted IR. Answers "what would
//! fusing parse and lower buy?" with a number instead of an argument.
//! Usage: cargo run --release -p mapal-backend-llvm --example stage_timing -- <file.mapal> [iters]
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: stage_timing <file.mapal> [iters]");
    let iters: u32 = args.next().map_or(20, |s| s.parse().unwrap());
    let src = std::fs::read_to_string(&path).expect("read");

    let (mut parse_us, mut lower_us, mut rewrite_us, mut emit_us) =
        (f64::MAX, f64::MAX, f64::MAX, f64::MAX);
    let mut ll_bytes = 0usize;
    for _ in 0..iters {
        let t = Instant::now();
        let po = mapal_syntax::parse(&src);
        parse_us = parse_us.min(t.elapsed().as_secs_f64() * 1e6);

        let t = Instant::now();
        let ir = mapal_lower::lower(&src, &po.program).expect("lower");
        lower_us = lower_us.min(t.elapsed().as_secs_f64() * 1e6);

        let t = Instant::now();
        let res = mapal_rewrite::rewrite(ir);
        rewrite_us = rewrite_us.min(t.elapsed().as_secs_f64() * 1e6);

        let t = Instant::now();
        let ll = mapal_backend_llvm::emit(&res.ir).expect("emit");
        emit_us = emit_us.min(t.elapsed().as_secs_f64() * 1e6);
        ll_bytes = ll.len();
    }
    println!(
        "{path}  (min of {iters}, source {} bytes, emitted {ll_bytes} bytes)",
        src.len()
    );
    println!("  lex+parse   {parse_us:9.1} us   <- the AST is built here");
    println!("  lower       {lower_us:9.1} us   <- AST -> graph");
    println!("  rewrite     {rewrite_us:9.1} us");
    println!("  emit        {emit_us:9.1} us");
    println!(
        "  front end   {:9.1} us   (parse+lower)",
        parse_us + lower_us
    );
}
