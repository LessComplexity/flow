//! Runnable dev tool: run a `.mapal` file through the interpreter (the oracle).
//!
//! ```text
//! cargo run -p mapal-interp --example run -- path/to/file.mapal
//! cargo run -p mapal-interp --example run -- examples/fir.mapal --budget 1000000
//! ```
//!
//! Runs the real pipeline (`mapal_syntax::parse` → `mapal_lower::lower` →
//! `mapal_interp::run`) and prints the program's output to stdout. The run is
//! **fueled** (E1): `--budget N` caps the step count (default 100_000_000); an
//! unbounded loop exhausts the budget and reports `diverged` rather than hanging.
//! Parse/lower diagnostics (`line:col`) and the non-`Done` outcome go to stderr
//! and set a non-zero exit.
//!
//! This is the `mapal run` slice, available before `mapal-cli` exists — it is a dev
//! example, not the user-facing CLI (mapal-cli owns real diagnostic rendering, C3).

use std::process::ExitCode;

use mapal_interp::{Outcome, RunResult, TrapKind, run};
use mapal_syntax::{Diagnostic, LineIndex, Severity, parse};

const DEFAULT_BUDGET: u64 = 100_000_000;

fn main() -> ExitCode {
    // Args: the first non-flag is the source path; `--budget N` (optional) caps fuel.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.iter().find(|a| !a.starts_with('-')).cloned() else {
        eprintln!("usage: cargo run -p mapal-interp --example run -- <file.mapal> [--budget N]");
        return ExitCode::from(2);
    };
    let budget = match parse_budget(&args) {
        Ok(b) => b,
        Err(msg) => {
            eprintln!("run: {msg}");
            return ExitCode::from(2);
        }
    };

    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("run: cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let lines = LineIndex::new(&source);

    // --- frontend, pass 1: parse ---
    let parsed = parse(&source);
    if !parsed.diagnostics.is_empty() {
        report(&path, &lines, "parse", &parsed.diagnostics);
        let errors = parsed
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        if errors > 0 {
            eprintln!("run: {errors} parse error(s) — not lowering.");
            return ExitCode::FAILURE;
        }
    }

    // --- frontend, pass 2: lower → sealed IR ---
    let ir = match mapal_lower::lower(&source, &parsed.program) {
        Ok(ir) => ir,
        Err(diags) => {
            report(&path, &lines, "lower", &diags);
            eprintln!(
                "run: {} lower error(s) — rejected (out of Mapal-Core, or ill-typed).",
                diags.len()
            );
            return ExitCode::FAILURE;
        }
    };

    // --- run the oracle (fueled) ---
    let RunResult { outcome, output } = run(&ir, budget);
    // Program output is raw (it already carries its own newlines from print/println).
    print!("{output}");
    match outcome {
        Outcome::Done(_) => ExitCode::SUCCESS,
        Outcome::Diverged => {
            eprintln!(
                "run: diverged — step budget ({budget}) exhausted (E1). Raise it with --budget N."
            );
            ExitCode::FAILURE
        }
        Outcome::Trapped(kind) => {
            let what = match kind {
                TrapKind::DivZero => "division/modulo by zero",
                TrapKind::IndexOob => "array index out of bounds",
            };
            eprintln!("run: trapped — {what} (ADR-0013).");
            ExitCode::FAILURE
        }
    }
}

/// Parse an optional `--budget N` flag; default [`DEFAULT_BUDGET`].
fn parse_budget(args: &[String]) -> Result<u64, String> {
    match args.iter().position(|a| a == "--budget") {
        Some(i) => args
            .get(i + 1)
            .ok_or_else(|| "--budget requires a value".to_string())?
            .parse::<u64>()
            .map_err(|_| "--budget value must be a non-negative integer".to_string()),
        None => Ok(DEFAULT_BUDGET),
    }
}

/// Minimal renderer-free diagnostic print: `file:line:col severity[CODE] (stage) msg`.
fn report(path: &str, lines: &LineIndex<'_>, stage: &str, diags: &[Diagnostic]) {
    for d in diags {
        let lc = lines.line_col(d.span.start);
        let sev = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        eprintln!(
            "{path}:{}:{} {sev}[{}] ({stage}) {}",
            lc.line, lc.col, d.code.0, d.message
        );
    }
}
