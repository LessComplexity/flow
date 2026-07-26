//! Emit textual CUDA C++ for a `.mapal` file (ADR-0020; dev tool — the future
//! `mapal build` embryo). Usage: `cargo run -p mapal-backend-cuda --example emit -- <file.mapal> [-] [--perf] [--rewrite]`
//! (writes `<file>.cu` next to the source, or stdout with `-`; `--rewrite`
//! rewrites the lowered IR before emission; `--perf` instruments every kernel
//! launch with CUDA-event timing — suggestions.md #19a, the `MAPAL_PERF launch=`
//! / `MAPAL_PERF total ms=` lines).

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: emit <file.mapal> [-] [--perf] [--rewrite]");
        return ExitCode::from(2);
    };
    let mut to_stdout = false;
    let mut rewrite = false;
    let mut opts = mapal_backend_cuda::EmitOpts::default();
    for a in args {
        match a.as_str() {
            "-" => to_stdout = true,
            "--perf" => opts.perf_timing = true,
            "--rewrite" => rewrite = true,
            other => {
                eprintln!(
                    "unknown flag: {other} (usage: emit <file.mapal> [-] [--perf] [--rewrite])"
                );
                return ExitCode::from(2);
            }
        }
    }
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("read {path}: {e}");
            return ExitCode::from(1);
        }
    };
    let po = mapal_syntax::parse(&src);
    if !po.diagnostics.is_empty() {
        eprintln!("parse diagnostics: {:?}", po.diagnostics);
        return ExitCode::from(1);
    }
    let ir = match mapal_lower::lower(&src, &po.program) {
        Ok(ir) => ir,
        Err(d) => {
            eprintln!("lower: {d:?}");
            return ExitCode::from(1);
        }
    };
    let ir = if rewrite {
        let r = mapal_rewrite::rewrite(ir);
        r.ir
    } else {
        ir
    };
    match mapal_backend_cuda::emit_with_opts(&ir, &opts) {
        Ok(cu) => {
            if to_stdout {
                print!("{cu}");
            } else {
                let out = format!("{}.cu", path.strip_suffix(".mapal").unwrap_or(&path));
                std::fs::write(&out, cu).expect("write .cu");
                eprintln!("wrote {out}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("emit: {e:?}");
            ExitCode::from(1)
        }
    }
}
