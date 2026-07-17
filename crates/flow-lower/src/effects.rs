//! Pass B — effect & call-graph analysis (DESIGN §6.1/§6.2). A lexically-scoped
//! name walk per function finds direct `print` uses (→ directly effectful) and
//! the direct callee set (call edges, including calls inside the owner's
//! map/fold blocks — LD24). Then: call-graph cycles → L1008; transitive
//! effectfulness by iterative worklist propagation (J1: no recursion).

use std::collections::{BTreeMap, BTreeSet};

use flow_syntax::{
    self as syn, ArmPayload, Block, BlockItem, Chain, ExprKind, Program, StageKind, StmtKind,
};

use crate::diag::{LCode, diag};
use crate::scope::ScopeStack;
use crate::tys::name_text;

/// The result of Pass B: which functions are (transitively) effectful, and the
/// direct call edges (for the L1008 cycle report, already run).
pub struct Effects {
    /// fn name → effectful?
    effectful: BTreeMap<String, bool>,
}

impl Effects {
    /// Whether the function named `name` is (transitively) effectful.
    pub fn is_effectful(&self, name: &str) -> bool {
        self.effectful.get(name).copied().unwrap_or(false)
    }
}

/// Run Pass B over `program`. `fn_names` is the set of surface function names
/// (resolution targets). On a call-graph cycle, returns the L1008 diagnostics
/// (lowering must abort before declaring functions). Otherwise returns the
/// effect table.
pub fn analyze(
    source: &str,
    program: &Program,
    fn_names: &BTreeSet<String>,
) -> Result<Effects, Vec<syn::Diagnostic>> {
    // 1. Per fn: direct print use + direct callees (deduplicated, source order).
    let mut direct_effect: BTreeMap<String, bool> = BTreeMap::new();
    let mut callees: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut fn_spans: BTreeMap<String, syn::SourceLoc> = BTreeMap::new();

    for item in &program.items {
        if let syn::Item::Fn(fd) = item {
            let fname = name_text(source, fd.name).to_string();
            fn_spans.entry(fname.clone()).or_insert(fd.name.span);
            let mut walk = NameWalk {
                source,
                fn_names,
                direct_effect: false,
                callees: Vec::new(),
                seen_callee: BTreeSet::new(),
            };
            let mut scope: ScopeStack<()> = ScopeStack::new();
            for p in &fd.params {
                scope.bind(name_text(source, p.name), ());
            }
            walk.block(&fd.body, &mut scope);
            direct_effect
                .entry(fname.clone())
                .or_insert(walk.direct_effect);
            callees.entry(fname).or_insert(walk.callees);
        }
    }

    // 2. Cycle check over the call graph (L1008) — iterative DFS.
    if let Some(cycle) = find_cycle(&callees) {
        let span = cycle
            .first()
            .and_then(|n| fn_spans.get(n))
            .copied()
            .unwrap_or(program.span);
        let joined = cycle.join(" -> ");
        return Err(vec![diag(
            LCode::RecursiveCall,
            span,
            format!("recursive call cycle: {joined} (recursion is out of Core)"),
        )]);
    }

    // 3. Transitive effect propagation (iterative worklist over the acyclic
    //    graph): a fn is effectful if it directly prints or calls an effectful fn.
    let mut effectful: BTreeMap<String, bool> = direct_effect.clone();
    let mut changed = true;
    while changed {
        changed = false;
        // Deterministic iteration order (BTreeMap).
        let names: Vec<String> = callees.keys().cloned().collect();
        for n in names {
            if effectful.get(&n).copied().unwrap_or(false) {
                continue;
            }
            let calls_effectful = callees
                .get(&n)
                .map(|cs| {
                    cs.iter()
                        .any(|c| effectful.get(c).copied().unwrap_or(false))
                })
                .unwrap_or(false);
            if calls_effectful {
                effectful.insert(n, true);
                changed = true;
            }
        }
    }

    Ok(Effects { effectful })
}

/// The lexically-scoped name walk (a thin precursor of the typing walk; DESIGN
/// §6.1). Tracks only names, recording direct `print` use and call edges.
struct NameWalk<'a> {
    source: &'a str,
    fn_names: &'a BTreeSet<String>,
    direct_effect: bool,
    callees: Vec<String>,
    seen_callee: BTreeSet<String>,
}

impl NameWalk<'_> {
    fn add_callee(&mut self, name: &str) {
        if self.seen_callee.insert(name.to_string()) {
            self.callees.push(name.to_string());
        }
    }

    fn block(&mut self, block: &Block, scope: &mut ScopeStack<()>) {
        for item in &block.items {
            match item {
                BlockItem::Stmt(s) => self.stmt(&s.kind, scope),
                BlockItem::Arm(arm) => self.arm_payload(&arm.payload, scope),
            }
        }
        if let Some(tail) = &block.tail {
            self.chain(tail, scope);
        }
    }

    fn stmt(&mut self, kind: &StmtKind, scope: &mut ScopeStack<()>) {
        match kind {
            StmtKind::Chain(c) => self.chain(c, scope),
            StmtKind::Bind(b) => {
                // value walked, then the name binds (shadowing fns).
                self.expr_names(&b.value, scope);
                scope.bind(name_text(self.source, b.name), ());
            }
            StmtKind::Loop(l) => {
                scope.push();
                self.block(&l.body, scope);
                scope.pop();
            }
            StmtKind::Error => {}
        }
    }

    fn arm_payload(&mut self, payload: &ArmPayload, scope: &mut ScopeStack<()>) {
        scope.push();
        match payload {
            ArmPayload::Chain(c) => self.chain(c, scope),
            ArmPayload::Block(b) => self.block(b, scope),
        }
        scope.pop();
    }

    fn chain(&mut self, chain: &Chain, scope: &mut ScopeStack<()>) {
        if let Some(head) = &chain.head {
            self.expr_names(head, scope);
        }
        for stage in &chain.stages {
            match &stage.kind {
                StageKind::Expr(e) => {
                    // A bare Var in stage position resolving past locals to a fn
                    // name is a call edge; to `print` is a direct effect.
                    if let ExprKind::Var(n) = &e.kind {
                        let text = name_text(self.source, *n);
                        if !scope.contains(text) {
                            if crate::is_print_builtin(text) {
                                self.direct_effect = true;
                            } else if self.fn_names.contains(text) {
                                self.add_callee(text);
                            }
                        }
                    } else {
                        self.expr_names(e, scope);
                    }
                }
                StageKind::Bind { name, .. } => {
                    scope.bind(name_text(self.source, *name), ());
                }
                StageKind::OpShorthand { expr } => self.expr_names(expr, scope),
                StageKind::Guard(arms) => {
                    for arm in arms {
                        self.arm_payload(&arm.payload, scope);
                    }
                }
                StageKind::Fanout { branches, .. } => {
                    // Branches lower in the enclosing scope (no child scope).
                    for b in branches {
                        self.chain(b, scope);
                    }
                }
                StageKind::SeqBlock(body) => {
                    // The seq body lowers in the enclosing scope (ADR-0019 pin b —
                    // bindings escape). A `print` inside DOES make the owner
                    // effectful: `seq` is the E2 legal effect site (pin e), unlike
                    // a map/fold body (LD24). Reuse the block walker directly.
                    self.block(body, scope);
                }
                StageKind::MapFold { params, body, .. } => {
                    // Block body: calls inside count as owner→callee edges (LD24);
                    // direct print inside is per-block (L1605), NOT owner-effectful.
                    scope.push();
                    for p in params {
                        scope.bind(name_text(self.source, *p), ());
                    }
                    self.block_callees_only(body, scope);
                    scope.pop();
                }
                StageKind::Ret { .. } | StageKind::LoopJump => {}
                StageKind::StmtBlock(_) | StageKind::Error(_) => {}
            }
        }
    }

    /// Walk a map/fold body collecting **call edges only** — block-internal
    /// `print` does NOT make the owner effectful (LD24/§6.1), it is L1605 at
    /// emission. Calls still count toward the owner's reference graph (I6).
    fn block_callees_only(&mut self, block: &Block, scope: &mut ScopeStack<()>) {
        let saved = self.direct_effect;
        self.block(block, scope);
        // Restore: block-internal print is not an owner effect.
        self.direct_effect = saved;
    }

    /// Walk an expression for nested call references (member/index/tuple/etc.).
    /// Variables here are values (locals); function names in value position are
    /// L1105 (reported later), not call edges.
    fn expr_names(&mut self, expr: &syn::Expr, scope: &mut ScopeStack<()>) {
        let mut stack: Vec<&syn::Expr> = vec![expr];
        while let Some(e) = stack.pop() {
            match &e.kind {
                ExprKind::Unary { operand, .. } => stack.push(operand),
                ExprKind::Binary { lhs, rhs, .. } => {
                    stack.push(lhs);
                    stack.push(rhs);
                }
                ExprKind::Member { base, .. } => stack.push(base),
                ExprKind::Index { base, index } => {
                    stack.push(base);
                    stack.push(index);
                }
                ExprKind::Tuple(es) | ExprKind::Array(es) => {
                    for inner in es {
                        stack.push(inner);
                    }
                }
                ExprKind::Struct { fields, .. } => {
                    for fi in fields {
                        if let Some(v) = &fi.value {
                            stack.push(v);
                        }
                    }
                }
                ExprKind::Call { callee, args } => {
                    stack.push(callee);
                    for a in args {
                        stack.push(a);
                    }
                }
                ExprKind::Question(inner) => stack.push(inner),
                _ => {}
            }
            let _ = scope; // scope unused for expr value names (no edges here).
        }
    }
}

/// Iterative DFS cycle detection over the call graph. Returns the cycle nodes
/// (in path order) on the first back edge, else `None`.
fn find_cycle(callees: &BTreeMap<String, Vec<String>>) -> Option<Vec<String>> {
    let mut color: BTreeMap<String, u8> = BTreeMap::new();
    for k in callees.keys() {
        color.insert(k.clone(), 0);
    }
    let roots: Vec<String> = callees.keys().cloned().collect();
    for root in roots {
        if color.get(&root).copied().unwrap_or(2) != 0 {
            continue;
        }
        let mut path: Vec<String> = Vec::new();
        struct Frame {
            node: String,
            refs: Vec<String>,
            cursor: usize,
        }
        let mut stack: Vec<Frame> = vec![Frame {
            node: root.clone(),
            refs: callees.get(&root).cloned().unwrap_or_default(),
            cursor: 0,
        }];
        color.insert(root.clone(), 1);
        path.push(root);

        while let Some(frame) = stack.last_mut() {
            if frame.cursor < frame.refs.len() {
                let w = frame.refs[frame.cursor].clone();
                frame.cursor += 1;
                if !callees.contains_key(&w) {
                    continue;
                }
                match color.get(&w).copied().unwrap_or(2) {
                    0 => {
                        color.insert(w.clone(), 1);
                        path.push(w.clone());
                        let refs = callees.get(&w).cloned().unwrap_or_default();
                        stack.push(Frame {
                            node: w,
                            refs,
                            cursor: 0,
                        });
                    }
                    1 => {
                        let start = path.iter().position(|x| x == &w).unwrap_or(0);
                        let mut cyc = path[start..].to_vec();
                        cyc.push(w);
                        return Some(cyc);
                    }
                    _ => {}
                }
            } else {
                color.insert(frame.node.clone(), 2);
                path.pop();
                stack.pop();
            }
        }
    }
    None
}
