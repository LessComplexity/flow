//! The fixpoint driver (DESIGN §5) — the crate's public entry point. Runs the
//! six passes to a fixpoint through the one shared replayer, capped at
//! [`MAX_ROUNDS`], asserting `validate()`-clean after every rebuild (R2).
//!
//! **By-value intake** (`CategoryIr` is not `Clone`): a round whose plans are all
//! empty returns the input graph *itself* — no rebuild. **Non-canonical guard**
//! (R6, RW8): if any function's loop shape is outside the canonical quartet,
//! `rewrite` is the identity on the whole graph, with `skipped_non_canonical`.

use mapal_ir::{CategoryIr, validate};

use crate::equations::analyze_const_fold;
use crate::functor_laws::analyze_map_fusion;
use crate::graph_rewrites::{analyze_cse, analyze_dce};
use crate::inline::analyze_inline;
use crate::lift::analyze_lift;
use crate::plan::RewritePlan;
use crate::replay::{is_canonical, replay};

/// Fixpoint round cap (DESIGN §5). No implemented rule grows the graph except
/// fusion's body synthesis, which strictly decreases the `Map`-edge count — every
/// pass shrinks a well-founded measure, so the cap is a safety net, not a bound.
pub const MAX_ROUNDS: u32 = 32;

/// The available passes (DESIGN §5 + region-emission plan Move 1 + R-LF/R-LM).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassId {
    /// Layer 3 — constant folding + algebraic identities (§2).
    ConstFold,
    /// Layer 4 — common-subexpression elimination (§3.2).
    Cse,
    /// Layer 4 — dead-code elimination (§3.1).
    Dce,
    /// Layer 1 — map fusion + `map(id)` (§4).
    MapFusion,
    /// Region-emission Move 1 — strip `Call` morphisms by graph substitution.
    Inline,
    /// R-LF / R-LM — canonical counted loops to captured `Fold` / `Map`.
    LiftLoops,
}

/// The rewrite of one graph plus its diagnostic log (DESIGN §5). Not `Clone`/
/// `PartialEq`: `CategoryIr` is neither (graph.rs is `Debug`-only), so §7's
/// blanket derive request cannot apply to the `ir`-bearing struct — [`RewriteReport`]
/// carries those derives instead.
#[derive(Debug)]
pub struct RewriteResult {
    /// The rewritten, sealed, `validate()`-clean graph.
    pub ir: CategoryIr,
    /// Which laws fired, how many times, and whether the R6 identity path ran.
    pub report: RewriteReport,
}

/// The §9.6 diagnostic log (DESIGN §5).
#[derive(Clone, Debug, PartialEq)]
pub struct RewriteReport {
    /// Fixpoint rounds executed (≥1; the last is the no-op round that halts it).
    pub rounds: u32,
    /// Cumulative applications per pass that fired ≥1, in round order.
    pub applied: Vec<(PassId, u64)>,
    /// Whether a non-canonical loop forced the whole-graph identity (RW8).
    pub skipped_non_canonical: bool,
}

/// The default rewrite: six passes, with loop lifting immediately after Inline,
/// to a fixpoint (DESIGN §5).
pub fn rewrite(ir: CategoryIr) -> RewriteResult {
    rewrite_with(
        ir,
        &[
            PassId::Inline,
            PassId::LiftLoops,
            PassId::ConstFold,
            PassId::Cse,
            PassId::Dce,
            PassId::MapFusion,
        ],
    )
}

/// Rewrite with an explicit pass list (DESIGN §5 — tests / CLI). Runs `passes` in
/// order each round until a full round applies nothing.
pub fn rewrite_with(ir: CategoryIr, passes: &[PassId]) -> RewriteResult {
    // R6 whole-graph identity: no property can even run on a non-canonical loop
    // (interp's M1 assert), so hand the input back untouched.
    if !is_canonical(&ir) {
        return RewriteResult {
            ir,
            report: RewriteReport {
                rounds: 0,
                applied: Vec::new(),
                skipped_non_canonical: true,
            },
        };
    }

    let mut cur = ir;
    let mut applied: Vec<(PassId, u64)> = Vec::new();
    let mut rounds = 0u32;

    loop {
        let mut any = false;
        for &pass in passes {
            let plan = analyze(pass, &cur);
            let n = plan_len(&plan);
            if n == 0 {
                continue; // empty plan: no rebuild (by-value identity).
            }
            let next = replay(&cur, &plan).expect("canonical graph replays");
            debug_assert!(
                validate(&next).is_empty(),
                "rewrite: {pass:?} produced an invalid graph: {:?}",
                validate(&next)
            );
            cur = next;
            record(&mut applied, pass, n);
            any = true;
        }
        rounds += 1;
        if !any {
            break;
        }
        if rounds >= MAX_ROUNDS {
            debug_assert!(false, "rewrite: MAX_ROUNDS hit without a fixpoint");
            break;
        }
    }

    RewriteResult {
        ir: cur,
        report: RewriteReport {
            rounds,
            applied,
            skipped_non_canonical: false,
        },
    }
}

/// Run one pass's analysis on `ir` (DESIGN §5 dispatch).
fn analyze(pass: PassId, ir: &CategoryIr) -> RewritePlan {
    match pass {
        PassId::ConstFold => analyze_const_fold(ir),
        PassId::Cse => analyze_cse(ir),
        PassId::Dce => analyze_dce(ir),
        PassId::MapFusion => analyze_map_fusion(ir),
        PassId::Inline => analyze_inline(ir),
        PassId::LiftLoops => analyze_lift(ir),
    }
}

/// The number of applications a plan encodes (its total channel entries).
fn plan_len(plan: &RewritePlan) -> u64 {
    (plan.alias.len()
        + plan.constify.len()
        + plan.drop.len()
        + plan.fuse.len()
        + plan.inline.len()
        + plan.lift.len()) as u64
}

/// Accumulate `n` applications of `pass` (first-occurrence order, deterministic).
fn record(applied: &mut Vec<(PassId, u64)>, pass: PassId, n: u64) {
    if let Some(e) = applied.iter_mut().find(|(p, _)| *p == pass) {
        e.1 += n;
    } else {
        applied.push((pass, n));
    }
}
