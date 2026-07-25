//! Emit textual LLVM IR for a `.flow` file (ADR-0020; dev tool — the future
//! `flow build` embryo). Usage: `cargo run -p flow-backend-llvm --example emit -- <file.flow> [-] [--perf] [--no-tile] [--no-pack] [--kc] [--contract] [--rewrite] [--target=<name>]`
//! (writes `<file>.ll` next to the source, `<file>_perf.ll` with `--perf`, or
//! stdout with `-`; `--rewrite` rewrites the lowered IR before emission).

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!(
            "usage: emit <file.flow> [-] [--perf] [--no-tile] [--no-pack] [--kc] [--contract] [--rewrite] [--target=<name>]"
        );
        return ExitCode::from(2);
    };
    let mut to_stdout = false;
    let mut rewrite = false;
    let mut opts = flow_backend_llvm::EmitOpts::default();
    for a in args {
        match a.as_str() {
            "-" => to_stdout = true,
            "--perf" => opts.perf_timing = true,
            "--no-tile" => opts.tiling = false,
            "--no-pack" => opts.packing = false,
            // The KC k-panel nest is default-OFF (a measured 3x loss locally at
            // 1024 f32, S29); this is how a box run opts in.
            "--kc" => opts.kc_nest = true,
            "--contract" => opts.contract = true,
            "--rewrite" => rewrite = true,
            // Machine facts by name (plan-s31-target-profiles): `generic` is
            // the default and reproduces today's literals; `apple-m` and
            // `zen3` differ. Never probed — a box run names its machine.
            _ if a.starts_with("--target=") => {
                opts.target = a["--target=".len()..].to_owned().leak();
            }
            other => {
                eprintln!(
                    "unknown flag: {other} (usage: emit <file.flow> [-] [--perf] [--no-tile] [--no-pack] [--kc] [--contract] [--rewrite] [--target=<name>])"
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
    match flow_backend_llvm::emit_with_opts(&ir, &opts) {
        Ok(ll) => {
            if to_stdout {
                print!("{ll}");
            } else {
                let stem = path.strip_suffix(".flow").unwrap_or(&path);
                let out = if opts.perf_timing {
                    format!("{stem}_perf.ll")
                } else {
                    format!("{stem}.ll")
                };
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
