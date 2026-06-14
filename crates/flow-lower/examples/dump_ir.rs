//! Runnable dev tool: dump a `.flow` file's sealed Category-IR as Mermaid.
//!
//! ```text
//! cargo run -p flow-lower --example dump_ir -- path/to/file.flow
//! ```
//!
//! Runs the real frontend (`flow_syntax::parse` → `flow_lower::lower`) and prints
//! a lint-clean Mermaid flowchart of the sealed [`flow_ir::CategoryIr`] to stdout.
//! Parse/lower diagnostics (with `line:col`) go to stderr and set a non-zero exit.
//!
//! No interpreter required — this is the `flow dump-ir --mermaid` slice, available
//! before `flow-cli` exists (the categorical pipeline `parse ; lower ; to_mermaid`
//! is three calls; see `docs/architecture/categorical-model.md` §3). It is a dev
//! example, not the user-facing CLI: rendering here is minimal (flow-cli owns real
//! diagnostic rendering — C3).

use std::process::ExitCode;

use flow_syntax::{Diagnostic, LineIndex, Severity, parse};

fn main() -> ExitCode {
    // First non-flag argument is the source path (flags like `--mermaid` are accepted
    // and ignored — Mermaid is the only output today; `--json` lands with §5.3).
    let Some(path) = std::env::args().skip(1).find(|a| !a.starts_with('-')) else {
        eprintln!("usage: cargo run -p flow-lower --example dump_ir -- <file.flow>");
        return ExitCode::from(2);
    };

    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("dump_ir: cannot read {path}: {e}");
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
            // `lower` requires a parse-clean tree (out-of-Core forms are Error-severity
            // P-codes); lowering an unclean tree is meaningless, so stop here.
            eprintln!("dump_ir: {errors} parse error(s) — not lowering.");
            return ExitCode::FAILURE;
        }
    }

    // --- frontend, pass 2: lower → sealed IR ---
    match flow_lower::lower(&source, &parsed.program) {
        Ok(ir) => {
            let mermaid = ir.to_mermaid();
            // Defense-in-depth: every dump must lint clean (ir DESIGN §14 / D9). A
            // finding here is a flow-ir bug, not a user error — surface it, still print.
            for finding in flow_ir::lint_mermaid(&mermaid) {
                eprintln!("dump_ir: WARNING mermaid lint (a flow-ir bug): {finding}");
            }
            println!("{mermaid}");
            ExitCode::SUCCESS
        }
        Err(diags) => {
            report(&path, &lines, "lower", &diags);
            eprintln!(
                "dump_ir: {} lower error(s) — rejected (out of Flow-Core, or ill-typed).",
                diags.len()
            );
            ExitCode::FAILURE
        }
    }
}

/// Minimal renderer-free diagnostic print: `file:line:col severity[CODE] (stage) msg`.
fn report(path: &str, lines: &LineIndex, stage: &str, diags: &[Diagnostic]) {
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
