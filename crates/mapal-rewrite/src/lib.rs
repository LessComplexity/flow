//! `mapal-rewrite`: the Mapal-Core optimizer (DESIGN `docs/components/rewrite`).
//!
//! Every pass factors as *analysis* (`&CategoryIr → RewritePlan`, pure) followed
//! by *replay* (`&CategoryIr × &RewritePlan → CategoryIr`, through the public
//! `IrBuilder`) — one shared replayer, six plan channels (DESIGN §1).
//! Sealed IR is immutable, so
//! rebuild-through-builder is the only constructor; output well-formedness is by
//! construction (R1/R2).
//!
//! WP1 ships the plan + replayer + identity anchor. WP4 adds layer 1 (map
//! fusion + `map(id)` elimination, `functor_laws`) and the layer-2 naturality
//! rule table (data only, `naturality`). WP5 adds layers 3/4 (`equations`,
//! `graph_rewrites`) and the fixpoint `driver` — the public `rewrite` entry.
//! Region-emission Move 1 adds the `inline` strip pass; R-LF/R-LM add counted
//! loop lifting. Both are in the default fixpoint (`Inline`, then `LiftLoops`).

mod driver;
mod equations;
mod functor_laws;
mod graph_rewrites;
mod inline;
mod lift;
mod naturality;
mod plan;
mod replay;

pub use driver::{MAX_ROUNDS, PassId, RewriteReport, RewriteResult, rewrite, rewrite_with};
pub use equations::analyze_const_fold;
pub use functor_laws::analyze_map_fusion;
pub use graph_rewrites::{analyze_cse, analyze_dce};
pub use inline::{INLINE_MAX_BODY, analyze_inline};
pub use lift::analyze_lift;
pub use naturality::{NATURALITY_LAWS, NaturalityLaw};
pub use plan::{FusionSpec, LiftKind, LiftSpec, RewritePlan};
pub use replay::{ReplayError, is_canonical, replay};
