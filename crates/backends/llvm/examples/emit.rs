//! Emit textual LLVM IR for a `.flow` file (ADR-0020; dev tool — the future
//! `flow build` embryo). Usage: `cargo run -p flow-backend-llvm --example emit -- <file.flow> [-] [--rewrite]`
//! (writes `<file>.ll` next to the source, or stdout with `-`; `--rewrite`
//! rewrites the lowered IR before emission).

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: emit <file.flow> [-] [--rewrite]");
        return ExitCode::from(2);
    };
    let mut to_stdout = false;
    let mut rewrite = false;
    for a in args {
        match a.as_str() {
            "-" => to_stdout = true,
            "--rewrite" => rewrite = true,
            other => {
                eprintln!("unknown flag: {other} (usage: emit <file.flow> [-] [--rewrite])");
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
    let po = flow_syntax::parse(&src);
    if !po.diagnostics.is_empty() {
        eprintln!("parse diagnostics: {:?}", po.diagnostics);
        return ExitCode::from(1);
    }
    let ir = match flow_lower::lower(&src, &po.program) {
        Ok(ir) => ir,
        Err(d) => {
            eprintln!("lower: {d:?}");
            return ExitCode::from(1);
        }
    };
    let ir = if rewrite {
        let r = flow_rewrite::rewrite(ir);
        r.ir
    } else {
        ir
    };
    match flow_backend_llvm::emit(&ir) {
        Ok(ll) => {
            if to_stdout {
                print!("{ll}");
            } else {
                let out = format!("{}.ll", path.strip_suffix(".flow").unwrap_or(&path));
                std::fs::write(&out, ll).expect("write .ll");
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
