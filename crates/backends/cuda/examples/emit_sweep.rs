//! Local emission sweep over the deterministic testgen draws — the emit half
//! of `differential_testgen_closed_sweep`, runnable WITHOUT nvcc. Emitter
//! panics in the sweep otherwise surface only on a GPU box (the macOS local
//! run skips the whole differential); this closes that blind spot. Found by
//! the S23 box run (`neg operand` panic, kernel.rs Neg arm).
//!
//! Exit 0 = every closed draw emits (raw + rewritten); exit 1 lists panics.

#[path = "../../../flow-rewrite/tests/testgen/mod.rs"]
mod testgen;

use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;
use testgen::{Built, build, prog_strategy};

fn main() {
    let mut runner = TestRunner::deterministic();
    let mut n = 0usize;
    let mut ok = 0usize;
    let mut panics: Vec<String> = Vec::new();
    for (count, trap_free) in [(256usize, false), (64usize, true)] {
        let strat = prog_strategy(trap_free, false);
        for _ in 0..count {
            let prog = strat.new_tree(&mut runner).unwrap().current();
            let Built { ir, open, .. } = build(&prog);
            if open {
                continue; // excluded (BL8), as in the differential
            }
            let rewritten = flow_rewrite::rewrite(build(&prog).ir).ir;
            for (leg, ir) in [("raw", &ir), ("rewritten", &rewritten)] {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    flow_backend_cuda::emit(ir)
                }));
                match r {
                    Ok(_) => ok += 1,
                    Err(_) => {
                        panics.push(format!("testgen#{n}/{leg}\n{prog:#?}"));
                    }
                }
            }
            n += 1;
        }
    }
    println!("draws {n}, emissions ok {ok}, panics {}", panics.len());
    if let Some(first) = panics.first() {
        eprintln!("FIRST PANICKING CASE:\n{first}");
    }
    // FLOW_EMIT_SWEEP_MERMAID=<n> dumps that draw's raw IR for feeder tracing.
    if let Ok(want) = std::env::var("FLOW_EMIT_SWEEP_MERMAID") {
        let want: usize = want.parse().unwrap();
        let mut runner = TestRunner::deterministic();
        let mut k = 0usize;
        'outer: for (count, trap_free) in [(256usize, false), (64usize, true)] {
            let strat = prog_strategy(trap_free, false);
            for _ in 0..count {
                let prog = strat.new_tree(&mut runner).unwrap().current();
                let Built { ir, open, .. } = build(&prog);
                if open {
                    continue;
                }
                if k == want {
                    eprintln!("{}", ir.to_mermaid());
                    break 'outer;
                }
                k += 1;
            }
        }
    }
    std::process::exit(if panics.is_empty() { 0 } else { 1 });
}
