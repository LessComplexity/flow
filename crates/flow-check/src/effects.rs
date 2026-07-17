//! Pass 2 — E2 effect legality (T0201; DESIGN §4).
//!
//! Two deduced facts, then one tree walk:
//!
//! 1. **name → effectful?** over `ir.funcs()` Named entries. `effectful?(f)` =
//!    [`ty_contains_token`] on `f`'s input **or** output object ty — the lowered
//!    signature already *is* the transitive effect closure (ir §8 token
//!    synthesis), so there is no fixpoint here (CK4).
//! 2. **The context-sensitive walk.** Since ADR-0019, `seq { … }` is its own
//!    node (`StageKind::SeqBlock`), distinct from a parallel `Fanout`. The walk
//!    discriminates on **node kind** (the natural reading): a `Fanout` opens an
//!    illegal-effect context **unconditionally** (`FanoutKind` is `Plain |
//!    Void`; `Void` is P0113-rejected at parse and unreachable here), and a
//!    `SeqBlock` recurses with the context **unchanged (sticky)** — a top-level
//!    `seq` never opens it, a `seq` inside a `Plain` branch does not clear it
//!    (C-check-4). The context clears only on leaving the branch. In context,
//!    any effectful morphism is a T0201 at its own site: a `print`/`println`
//!    builtin use, or a **call** — a stage-position bare `Var` not shadowed by
//!    an in-scope local (scope-aware, mirroring lower's effect walk) — whose
//!    resolved function is `effectful?`. Map/fold bodies and guard arms cannot
//!    be effectful (rejected upstream) and carry no effect site.

use std::collections::{BTreeMap, BTreeSet};

use flow_ir::{CategoryIr, ty_contains_token};
use flow_syntax::{
    self as syn, ArmPayload, Block, BlockItem, Chain, ExprKind, Program, StageKind, StmtKind,
};

use crate::diag::{TCode, diag};

/// Run pass 2 over `(source, program, ir)`, appending T0201 diagnostics in tree
/// walk order.
pub fn check(source: &str, program: &Program, ir: &CategoryIr, out: &mut Vec<syn::Diagnostic>) {
    // 1. name → effectful? over Named functions (CK4). Keyed by the stored fn
    //    name (== `&source[span]` at declaration); BTreeMap, no HashMap (D2).
    let mut effectful: BTreeMap<&str, bool> = BTreeMap::new();
    for (_, fd) in ir.funcs() {
        if fd.kind != flow_ir::FuncKind::Named {
            continue;
        }
        let in_tok = ir
            .object(fd.input)
            .map(|o| ty_contains_token(&o.ty))
            .unwrap_or(false);
        let out_tok = ir
            .object(fd.output)
            .map(|o| ty_contains_token(&o.ty))
            .unwrap_or(false);
        effectful.insert(fd.name.as_str(), in_tok || out_tok);
    }

    // 2. Tree walk, one fn at a time, carrying scope + illegal-effect context.
    let mut walk = Walk {
        source,
        effectful: &effectful,
        out,
    };
    for item in &program.items {
        if let syn::Item::Fn(fd) = item {
            let mut scope = Scope::new();
            for p in &fd.params {
                scope.bind(name_text(source, p.name));
            }
            // The fn body starts in sequential context (in_fanout = false).
            walk.block(&fd.body, &mut scope, false);
        }
    }
}

/// A minimal lexical scope stack (locals only) — mirrors lower's `ScopeStack`
/// discipline: resolution is "is this name bound by an in-scope local?", locals
/// shadow function names. No payload; presence is all pass 2 needs.
struct Scope {
    frames: Vec<BTreeSet<String>>,
}

impl Scope {
    fn new() -> Self {
        Scope {
            frames: vec![BTreeSet::new()],
        }
    }
    fn push(&mut self) {
        self.frames.push(BTreeSet::new());
    }
    fn pop(&mut self) {
        self.frames.pop();
    }
    fn bind(&mut self, name: &str) {
        self.frames
            .last_mut()
            .expect("non-empty scope stack")
            .insert(name.to_string());
    }
    fn contains(&self, name: &str) -> bool {
        self.frames.iter().any(|f| f.contains(name))
    }
}

struct Walk<'a> {
    source: &'a str,
    effectful: &'a BTreeMap<&'a str, bool>,
    out: &'a mut Vec<syn::Diagnostic>,
}

impl Walk<'_> {
    /// `in_fanout` = are we lexically inside a `Plain` fanout branch (sticky).
    fn block(&mut self, block: &Block, scope: &mut Scope, in_fanout: bool) {
        for item in &block.items {
            match item {
                BlockItem::Stmt(s) => self.stmt(&s.kind, scope, in_fanout),
                BlockItem::Arm(arm) => self.arm(&arm.payload, scope, in_fanout),
            }
        }
        if let Some(tail) = &block.tail {
            self.chain(tail, scope, in_fanout);
        }
    }

    fn stmt(&mut self, kind: &StmtKind, scope: &mut Scope, in_fanout: bool) {
        match kind {
            StmtKind::Chain(c) => self.chain(c, scope, in_fanout),
            StmtKind::Bind(b) => {
                // The value is an expr (no call/effect site); then the name binds
                // (shadowing fns) — mirror of lower's effect walk.
                scope.bind(name_text(self.source, b.name));
            }
            StmtKind::Loop(l) => {
                scope.push();
                self.block(&l.body, scope, in_fanout);
                scope.pop();
            }
            StmtKind::Error => {}
        }
    }

    fn arm(&mut self, payload: &ArmPayload, scope: &mut Scope, in_fanout: bool) {
        scope.push();
        match payload {
            ArmPayload::Chain(c) => self.chain(c, scope, in_fanout),
            ArmPayload::Block(b) => self.block(b, scope, in_fanout),
        }
        scope.pop();
    }

    fn chain(&mut self, chain: &Chain, scope: &mut Scope, in_fanout: bool) {
        // The head is a value expression, never a call/effect site (mirror lower).
        for stage in &chain.stages {
            match &stage.kind {
                StageKind::Expr(e) => {
                    // A bare Var in stage position not shadowed by a local is a
                    // call/builtin use; classify it as an effect site.
                    if let ExprKind::Var(n) = &e.kind {
                        let text = name_text(self.source, *n);
                        if !scope.contains(text) {
                            self.classify_stage_call(text, n.span, in_fanout);
                        }
                    }
                }
                StageKind::Bind { name, .. } => scope.bind(name_text(self.source, *name)),
                StageKind::Guard(arms) => {
                    for arm in arms {
                        self.arm(&arm.payload, scope, in_fanout);
                    }
                }
                StageKind::Fanout { branches, .. } => {
                    // A `Fanout` node opens the illegal-effect context
                    // **unconditionally** (`FanoutKind` is `Plain | Void`;
                    // `Void` is P0113-rejected at parse and unreachable behind
                    // check's parse-clean precondition). Branches lower in the
                    // enclosing scope (no child scope), mirroring lower.
                    for b in branches {
                        self.chain(b, scope, true);
                    }
                }
                StageKind::SeqBlock(body) => {
                    // `seq` recurses with **sticky** context (C-check-4): a
                    // top-level `seq` never opens the illegal context, a `seq`
                    // inside a `Plain` branch does not clear it (its composite is
                    // effectful iff its body is — ADR-0019). Statements + tail
                    // lower in the enclosing scope (bindings escape — pin b).
                    self.block(body, scope, in_fanout);
                }
                // Ret/LoopJump/OpShorthand carry no call site; map/fold bodies and
                // anonymous stmt-blocks are token-free / rejected upstream (§4).
                StageKind::Ret { .. }
                | StageKind::LoopJump
                | StageKind::OpShorthand { .. }
                | StageKind::MapFold { .. }
                | StageKind::StmtBlock(_)
                | StageKind::Error(_) => {}
            }
        }
    }

    /// A stage-position bare-`Var` call site. `print`/`println` are effectful
    /// uses directly; a named function is effectful iff `effectful?`. Anything
    /// else (pure fn, `zip`/`enumerate`, unknown) is legal. In `Plain`-branch
    /// context an effect is a T0201 at `span`.
    fn classify_stage_call(&mut self, text: &str, span: syn::SourceLoc, in_fanout: bool) {
        if !in_fanout {
            return;
        }
        if is_print_builtin(text) {
            self.out.push(diag(
                TCode::EffectInFanout,
                span,
                format!(
                    "`{text}` is not permitted in a parallel fanout branch; \
                     effects must sequence — wrap the statements in a `seq` block"
                ),
            ));
        } else if self.effectful.get(text).copied().unwrap_or(false) {
            self.out.push(diag(
                TCode::EffectInFanout,
                span,
                format!(
                    "call to effectful function `{text}` is not permitted in a \
                     parallel fanout branch; effects must sequence — wrap the \
                     statements in a `seq` block"
                ),
            ));
        }
    }
}

/// The builtin print family (ADR-0015), same predicate as lower's
/// `is_print_builtin` (kept local — lower's is `pub(crate)`).
fn is_print_builtin(name: &str) -> bool {
    matches!(name, "print" | "println")
}

/// The text of a `Name` (a bare span into `source`), same as lower's helper.
fn name_text(source: &str, name: syn::Name) -> &str {
    &source[name.span.start as usize..name.span.end as usize]
}
