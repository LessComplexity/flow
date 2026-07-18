//! Deterministic Mermaid dump + lint (DESIGN §14; HANDOFF §5 item 6).
//!
//! [`CategoryIr::to_mermaid`] renders one `subgraph` per function in declaration
//! order, with per-dump node ids `f{i}o{j}` (never raw slotmap bits). Format is
//! normative for the §16 golden tests; [`lint_mermaid`] encodes the three §14
//! rule groups and runs on every dump in tests.

use slotmap::SecondaryMap;

use crate::graph::{CategoryIr, FuncId, ObjectId, ObjectKind, Operation};
use crate::ty::{Value, ty_label};

impl CategoryIr {
    /// Render the graph as deterministic Mermaid text (DESIGN §14).
    ///
    /// Node label: `name: ty` (named), `value: ty` (constants), bare `ty`
    /// otherwise; Return renders the name `ret`, LoopMerge prepends `⟲ `.
    /// Exactly one arrow style (`-->`), every label double-quoted with `"`
    /// escaped as `#quot;`, LoopBack edges labelled `"LoopBack ↩"` (D9).
    ///
    /// `O(V + E)`: one pass groups objects by owner and assigns the per-dump
    /// `f{i}o{j}` node names into a `SecondaryMap` (no per-edge linear scan).
    pub fn to_mermaid(&self) -> String {
        let mut out = String::from("flowchart LR\n");

        // Function index by FuncId (declaration order).
        let mut func_ix: SecondaryMap<FuncId, usize> = SecondaryMap::new();
        for (fi, (fid, _)) in self.funcs().enumerate() {
            func_ix.insert(fid, fi);
        }

        // Group object ids by owner in one pass (insertion order preserved), and
        // assign per-function ordinals into a name map.
        let nfuncs = func_ix.len();
        let mut owned: Vec<Vec<ObjectId>> = vec![Vec::new(); nfuncs];
        let mut node_name: SecondaryMap<ObjectId, String> = SecondaryMap::new();
        for (oid, _) in self.objects() {
            if let Some(&fi) = self.try_owner(oid).and_then(|f| func_ix.get(f)) {
                let oi = owned[fi].len();
                owned[fi].push(oid);
                node_name.insert(oid, format!("f{fi}o{oi}"));
            }
        }

        // Bucket edges by owning function in one pass (insertion order).
        let mut edges: Vec<Vec<crate::graph::MorphismId>> = vec![Vec::new(); nfuncs];
        for (mid, m) in self.morphisms() {
            if let Some(&fi) = self.try_owner(m.source).and_then(|f| func_ix.get(f)) {
                edges[fi].push(mid);
            }
        }

        for (fi, (_, def)) in self.funcs().enumerate() {
            out.push_str(&format!(
                "    subgraph f{fi}[\"fn {}\"]\n",
                escape(&def.name)
            ));

            // Nodes (insertion order).
            for &oid in &owned[fi] {
                let obj = self.object(oid).unwrap();
                let label = node_label(obj);
                // One round node shape `(( ))` for all data objects (§14).
                out.push_str(&format!(
                    "        {}((\"{}\"))\n",
                    node_name[oid],
                    escape(&label)
                ));
            }

            // Edges (insertion order); source==target owner by I6.
            for &mid in &edges[fi] {
                let m = self.morphism(mid).unwrap();
                let lbl = edge_label(&m.op);
                out.push_str(&format!(
                    "        {} -- \"{}\" --> {}\n",
                    node_name[m.source],
                    escape(&lbl),
                    node_name[m.target],
                ));
            }

            out.push_str("    end\n");
        }

        out
    }
}

/// The text inside a node, before quoting/escaping (DESIGN §14).
fn node_label(obj: &crate::graph::Object) -> String {
    let ty = ty_label(&obj.ty);
    let base = match (&obj.value, obj.kind, &obj.name) {
        // Constants: `value: ty`.
        (Some(v), ObjectKind::Constant, _) => format!("{}: {ty}", value_label(v)),
        // Return: name `ret`.
        (_, ObjectKind::Return, _) => format!("ret: {ty}"),
        // Named (Parameter/Temporary with a name): `name: ty`.
        (_, _, Some(n)) => format!("{n}: {ty}"),
        // Bare ty.
        (_, _, None) => ty.clone(),
    };
    match obj.kind {
        ObjectKind::LoopMerge => format!("⟲ {base}"),
        _ => base,
    }
}

/// A constant value rendered for a label; `Str` is escaped + truncated (20).
fn value_label(v: &Value) -> String {
    match v {
        Value::I32(x) => x.to_string(),
        Value::I64(x) => x.to_string(),
        Value::U8(x) => x.to_string(),
        Value::F32(x) => format!("{x:?}"),
        Value::F64(x) => format!("{x:?}"),
        Value::Bool(x) => x.to_string(),
        Value::Str(s) => {
            let mut shown: String = s.chars().take(20).collect();
            if s.chars().count() > 20 {
                shown.push('…');
            }
            format!("\"{shown}\"")
        }
    }
}

/// The edge label for an operation (DESIGN §14).
fn edge_label(op: &Operation) -> String {
    match op {
        Operation::Pair { slot, arity } => format!("Pair {slot}/{arity}"),
        Operation::Proj { index } => format!("Proj {index}"),
        Operation::Add => "Add".into(),
        Operation::Sub => "Sub".into(),
        Operation::Mul => "Mul".into(),
        Operation::Div => "Div".into(),
        Operation::Mod => "Mod".into(),
        Operation::Neg => "Neg".into(),
        Operation::Eq => "Eq".into(),
        Operation::Neq => "Neq".into(),
        Operation::Lt => "Lt".into(),
        Operation::Gt => "Gt".into(),
        Operation::Le => "Le".into(),
        Operation::Ge => "Ge".into(),
        Operation::And => "And".into(),
        Operation::Or => "Or".into(),
        Operation::Not => "Not".into(),
        Operation::Phi => "Phi".into(),
        Operation::Call(_) => "Call".into(),
        Operation::Map { .. } => "Map".into(),
        Operation::Fold { .. } => "Fold".into(),
        Operation::Index => "Index".into(),
        Operation::Zip => "Zip".into(),
        Operation::Enumerate => "Enumerate".into(),
        Operation::Update => "Update".into(),
        Operation::Print { newline: true } => "Println".into(),
        Operation::Print { newline: false } => "Print".into(),
        Operation::LoopEnter => "LoopEnter".into(),
        Operation::LoopBack => "LoopBack ↩".into(),
        Operation::LoopExit => "LoopExit".into(),
        Operation::Output => "Output".into(),
    }
}

/// Escape a label per §14 rule 1: `"` becomes `#quot;` before quoting.
fn escape(s: &str) -> String {
    s.replace('"', "#quot;")
}

/// Lint a Mermaid dump against the three §14 rule groups (DESIGN §14). Returns
/// one message per violation; empty means clean. Run on every dump in tests.
pub fn lint_mermaid(s: &str) -> Vec<String> {
    let mut errs = Vec::new();

    // Rule group 3 (structural): header, balanced subgraph/end, node ids.
    let mut lines = s.lines();
    match lines.next() {
        Some("flowchart LR") => {}
        _ => errs.push("missing `flowchart LR` header".into()),
    }
    let mut depth: i32 = 0;
    for line in s.lines() {
        let t = line.trim();
        if t.starts_with("subgraph ") {
            depth += 1;
        } else if t == "end" {
            depth -= 1;
            if depth < 0 {
                errs.push("unbalanced `end` (no matching subgraph)".into());
            }
        }
    }
    if depth != 0 {
        errs.push(format!("unbalanced subgraph/end (residual depth {depth})"));
    }

    // Rule group 2: exactly one arrow style — `-->`. No `-.->`, `==>`, `---`.
    // The check is on the STRUCTURAL arrow position only: a `"..."` label may
    // legitimately contain `---`/`==>`/`-.->` (e.g. a Str constant `"a---b"` or a
    // function named `a---b`), and a valid single-`-->` dump must not be flagged
    // for label content (reviewer F5). Strip the quoted-label span before
    // scanning the remainder for forbidden arrow tokens.
    for line in s.lines() {
        let scan = strip_quoted_label(line);
        if scan.contains("-.->") || scan.contains("==>") {
            errs.push(format!("disallowed arrow style in: {}", line.trim()));
        }
        // `---` (undirected link) — but `-->` contains no `---`.
        if scan.contains("---") {
            errs.push(format!("disallowed `---` link in: {}", line.trim()));
        }
    }

    // Rule group 1: every node/edge label is double-quoted; raw `"` escaped.
    // A label sits inside `"..."`. We verify each edge line has a quoted label
    // and node lines have a quoted label; and that no unescaped `"` appears
    // inside a label (i.e. labels contain `#quot;`, never a stray `"`).
    for line in s.lines() {
        let t = line.trim();
        if t.starts_with("subgraph") {
            // subgraph header carries a quoted label too: f0["fn main"].
            if !has_balanced_quoted_label(t) {
                errs.push(format!("subgraph label not double-quoted: {t}"));
            }
            continue;
        }
        if is_edge_line(t) {
            // edge: A -- "label" --> B
            if !t.contains("-- \"") || !t.contains("\" -->") {
                errs.push(format!("edge label not double-quoted: {t}"));
            }
        } else if is_node_line(t) && !has_balanced_quoted_label(t) {
            errs.push(format!("node label not double-quoted: {t}"));
        }
    }

    errs
}

/// Whether a (trimmed) line declares a node: `id((...))` etc., not an edge.
fn is_node_line(t: &str) -> bool {
    !t.is_empty()
        && t != "end"
        && !t.starts_with("flowchart")
        && !t.starts_with("subgraph")
        && !is_edge_line(t)
        && t.contains("((")
}

/// Whether a (trimmed) line declares an edge.
fn is_edge_line(t: &str) -> bool {
    t.contains("-->")
}

/// Remove a line's `"..."` label span so the arrow-style lint scans only the
/// structural (non-label) text (reviewer F5). Labels are escaped (raw `"` →
/// `#quot;`), so a line carries at most one outer quote pair; everything between
/// the first and last `"` (inclusive) is the label and is dropped. Lines with no
/// quote (or an unbalanced one) are returned unchanged.
fn strip_quoted_label(line: &str) -> String {
    match (line.find('"'), line.rfind('"')) {
        (Some(a), Some(b)) if a < b => {
            let mut out = String::with_capacity(line.len());
            out.push_str(&line[..a]);
            out.push_str(&line[b + 1..]);
            out
        }
        _ => line.to_owned(),
    }
}

/// Whether a line carries exactly one `"..."` label with no stray inner quote
/// (inner quotes must have been escaped to `#quot;`).
fn has_balanced_quoted_label(t: &str) -> bool {
    let first = t.find('"');
    let last = t.rfind('"');
    match (first, last) {
        (Some(a), Some(b)) if a < b => {
            // The substring between the outer quotes must contain no raw `"`.
            let inner = &t[a + 1..b];
            !inner.contains('"')
        }
        _ => false,
    }
}
