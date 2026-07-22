//! Emit textual LLVM IR for a `.flow` file (ADR-0020; dev tool — the future
//! `flow build` embryo). Usage: `cargo run -p flow-backend-llvm --example emit -- <file.flow>`
//! (writes `<file>.ll` next to the source, or stdout with `-`).

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: emit <file.flow> [-]");
        return ExitCode::from(2);
    };
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
    match flow_backend_llvm::emit(&ir) {
        Ok(ll) => {
            if args.next().as_deref() == Some("-") {
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
