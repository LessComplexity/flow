//! Pass A — the type table (DESIGN §5). Collects `type` declarations into
//! `name → Ty::Struct`, resolves field tys (`TyKind → flow_ir::Ty`), and
//! rejects duplicates, recursive type definitions, empty types/arrays, and
//! over-deep tys. `flow_ir::Ty::Struct` carries fully-inlined nested fields, so
//! resolution fully expands struct references.

use std::collections::BTreeMap;

use flow_ir::{MAX_TY_DEPTH, Ty};
use flow_syntax::{self as syn, Program, Ty as SynTy, TyKind};

use crate::diag::{LCode, diag};

/// Convert a `flow_syntax::SourceLoc` to a `flow_ir::SourceLoc` (field-identical;
/// DESIGN §1 / D8).
pub fn ir_loc(s: syn::SourceLoc) -> flow_ir::SourceLoc {
    flow_ir::SourceLoc::new(s.start, s.end)
}

/// The resolved type table: surface type name → its fully-expanded
/// `Ty::Struct`. Keyed deterministically (BTreeMap; I12/§11).
pub struct TypeTable {
    map: BTreeMap<String, Ty>,
}

impl TypeTable {
    /// Look up a resolved struct ty by surface name.
    pub fn get(&self, name: &str) -> Option<&Ty> {
        self.map.get(name)
    }
}

/// Build the type table for `program` (DESIGN §5), collecting every diagnostic.
/// Returns the table regardless (errored entries are simply absent), so later
/// passes can still resolve the well-formed ones and report their own errors.
pub fn build(source: &str, program: &Program) -> (TypeTable, Vec<syn::Diagnostic>) {
    let mut diags = Vec::new();

    // 1. Index declarations by name in source order; report duplicates (L1004).
    //    `decls` keeps the first declaration per name (deterministic).
    let mut decl_order: Vec<&syn::TypeDecl> = Vec::new();
    let mut seen_names: BTreeMap<String, ()> = BTreeMap::new();
    for item in &program.items {
        if let syn::Item::Type(td) = item {
            let name = name_text(source, td.name).to_string();
            if seen_names.contains_key(&name) {
                diags.push(diag(
                    LCode::DuplicateType,
                    td.name.span,
                    format!("duplicate type declaration `{name}`"),
                ));
                continue;
            }
            seen_names.insert(name, ());
            decl_order.push(td);
        }
    }

    // Map name → decl for reference resolution.
    let mut by_name: BTreeMap<&str, &syn::TypeDecl> = BTreeMap::new();
    for td in &decl_order {
        by_name.insert(name_text(source, td.name), *td);
    }

    // 2. Reject empty types and duplicate fields up front (L1010 / L1005).
    for td in &decl_order {
        if td.fields.is_empty() {
            diags.push(diag(
                LCode::EmptyType,
                td.span,
                format!(
                    "type `{}` has no fields; a zero-field type cannot be constructed",
                    name_text(source, td.name)
                ),
            ));
        }
        let mut field_seen: BTreeMap<&str, ()> = BTreeMap::new();
        for f in &td.fields {
            let fname = name_text(source, f.name);
            if field_seen.contains_key(fname) {
                diags.push(diag(
                    LCode::DuplicateField,
                    f.name.span,
                    format!("duplicate field `{fname}`"),
                ));
            } else {
                field_seen.insert(fname, ());
            }
        }
    }

    // 3. Detect struct-reference cycles (L1007) via iterative DFS over the decl
    //    reference graph (a struct references another struct by Named tyref).
    let cyclic = detect_cycles(source, &decl_order, &by_name, &mut diags);

    // 4. Resolve each non-cyclic declaration's fields to a fully-expanded
    //    `Ty::Struct`. Resolution recurses through referenced structs.
    let mut map: BTreeMap<String, Ty> = BTreeMap::new();
    for td in &decl_order {
        let name = name_text(source, td.name).to_string();
        if cyclic.contains(&name) {
            continue;
        }
        if let Some(ty) = resolve_struct(source, td, &by_name, &cyclic, &mut diags) {
            map.insert(name, ty);
        }
    }

    (TypeTable { map }, diags)
}

/// The text of a `Name` (a bare span into `source`).
pub fn name_text(source: &str, name: syn::Name) -> &str {
    &source[name.span.start as usize..name.span.end as usize]
}

/// The text of an arbitrary span into `source` (literal lexemes, etc.).
pub fn src_slice(source: &str, span: syn::SourceLoc) -> &str {
    &source[span.start as usize..span.end as usize]
}

/// Iterative DFS over the struct reference graph; returns the set of type names
/// that participate in a cycle (and reports one L1007 per cycle root reached).
fn detect_cycles(
    source: &str,
    decls: &[&syn::TypeDecl],
    by_name: &BTreeMap<&str, &syn::TypeDecl>,
    diags: &mut Vec<syn::Diagnostic>,
) -> std::collections::BTreeSet<String> {
    use std::collections::BTreeSet;
    let mut cyclic: BTreeSet<String> = BTreeSet::new();
    // 0 = white, 1 = gray (on path), 2 = black.
    let mut color: BTreeMap<String, u8> = BTreeMap::new();
    for td in decls {
        color.insert(name_text(source, td.name).to_string(), 0);
    }

    for td in decls {
        let root = name_text(source, td.name).to_string();
        if color.get(&root).copied().unwrap_or(2) != 0 {
            continue;
        }
        // Explicit stack: (node, referenced names, cursor).
        let mut path: Vec<String> = Vec::new();
        struct Frame {
            node: String,
            refs: Vec<(String, syn::SourceLoc)>,
            cursor: usize,
        }
        let mut stack: Vec<Frame> = vec![Frame {
            node: root.clone(),
            refs: struct_refs(source, by_name.get(root.as_str()).unwrap()),
            cursor: 0,
        }];
        color.insert(root.clone(), 1);
        path.push(root);

        while let Some(frame) = stack.last_mut() {
            if frame.cursor < frame.refs.len() {
                let (w, wspan) = frame.refs[frame.cursor].clone();
                frame.cursor += 1;
                // Only recurse into names that are actual struct decls.
                if !by_name.contains_key(w.as_str()) {
                    continue;
                }
                match color.get(&w).copied().unwrap_or(2) {
                    0 => {
                        color.insert(w.clone(), 1);
                        path.push(w.clone());
                        let refs = struct_refs(source, by_name.get(w.as_str()).unwrap());
                        stack.push(Frame {
                            node: w,
                            refs,
                            cursor: 0,
                        });
                    }
                    1 => {
                        // Back edge: every node from `w` onward in the path is cyclic.
                        let start = path.iter().position(|x| x == &w).unwrap_or(0);
                        for n in &path[start..] {
                            cyclic.insert(n.clone());
                        }
                        diags.push(diag(
                            LCode::RecursiveType,
                            wspan,
                            format!("recursive type definition involving `{w}`"),
                        ));
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
    cyclic
}

/// The struct-type names referenced (transitively at the syntactic surface) by
/// a declaration's field tys, with the span of each reference.
fn struct_refs(source: &str, td: &syn::TypeDecl) -> Vec<(String, syn::SourceLoc)> {
    let mut out = Vec::new();
    for f in &td.fields {
        collect_named_refs(source, &f.ty, &mut out);
    }
    out
}

/// Collect `Named` type references in a ty (iterative; covers tuple/array nesting).
fn collect_named_refs(source: &str, ty: &SynTy, out: &mut Vec<(String, syn::SourceLoc)>) {
    let mut stack: Vec<&SynTy> = vec![ty];
    while let Some(t) = stack.pop() {
        match &t.kind {
            TyKind::Named(n) => out.push((name_text(source, *n).to_string(), n.span)),
            TyKind::Tuple(ts) => {
                for inner in ts {
                    stack.push(inner);
                }
            }
            TyKind::Array { elem, .. } => stack.push(elem),
            TyKind::Dynamic(inner) => stack.push(inner),
            TyKind::Error => {}
        }
    }
}

/// Resolve a `type` declaration to a fully-expanded `Ty::Struct`.
fn resolve_struct(
    source: &str,
    td: &syn::TypeDecl,
    by_name: &BTreeMap<&str, &syn::TypeDecl>,
    cyclic: &std::collections::BTreeSet<String>,
    diags: &mut Vec<syn::Diagnostic>,
) -> Option<Ty> {
    let mut fields = Vec::with_capacity(td.fields.len());
    let mut field_seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for f in &td.fields {
        let fname = name_text(source, f.name);
        if !field_seen.insert(fname) {
            // Duplicate already reported; skip the dup so the struct ty is sane.
            continue;
        }
        let fty = resolve_ty(source, &f.ty, by_name, cyclic, diags)?;
        fields.push((fname.to_string(), fty));
    }
    if fields.is_empty() {
        // Empty type already reported (L1010); do not register it.
        return None;
    }
    let ty = Ty::Struct {
        name: name_text(source, td.name).to_string(),
        fields,
    };
    if !depth_ok(&ty) {
        diags.push(diag(
            LCode::TypeTooDeep,
            td.span,
            format!(
                "type `{}` nests deeper than the {MAX_TY_DEPTH}-level limit",
                name_text(source, td.name)
            ),
        ));
        return None;
    }
    Some(ty)
}

/// Resolve a single `TyKind` to a `flow_ir::Ty` (DESIGN §5). Reports the
/// appropriate L-code and returns `None` on failure.
pub fn resolve_ty(
    source: &str,
    ty: &SynTy,
    by_name: &BTreeMap<&str, &syn::TypeDecl>,
    cyclic: &std::collections::BTreeSet<String>,
    diags: &mut Vec<syn::Diagnostic>,
) -> Option<Ty> {
    let resolved = match &ty.kind {
        TyKind::Named(n) => {
            let text = name_text(source, *n);
            match text {
                "i32" => Ty::i32(),
                "i64" => Ty::i64(),
                "u8" => Ty::u8(),
                "f32" => Ty::f32(),
                "f64" => Ty::f64(),
                "bool" => Ty::Bool,
                _ => {
                    if cyclic.contains(text) {
                        // Part of a reported cycle; do not re-resolve.
                        return None;
                    }
                    match by_name.get(text) {
                        Some(td) => resolve_struct(source, td, by_name, cyclic, diags)?,
                        None => {
                            diags.push(diag(
                                LCode::UnknownType,
                                n.span,
                                format!("unknown type `{text}`"),
                            ));
                            return None;
                        }
                    }
                }
            }
        }
        TyKind::Tuple(ts) => {
            let mut out = Vec::with_capacity(ts.len());
            for inner in ts {
                out.push(resolve_ty(source, inner, by_name, cyclic, diags)?);
            }
            Ty::Tuple(out)
        }
        TyKind::Array {
            elem,
            len,
            len_span,
        } => {
            if *len == 0 {
                diags.push(diag(
                    LCode::EmptyArray,
                    *len_span,
                    "array type has size 0".to_string(),
                ));
                return None;
            }
            let e = resolve_ty(source, elem, by_name, cyclic, diags)?;
            Ty::Array {
                elem: Box::new(e),
                size: *len,
            }
        }
        TyKind::Dynamic(_) | TyKind::Error => {
            diags.push(diag(
                LCode::UncleanTree,
                ty.span,
                "unclean type node reached lowering".to_string(),
            ));
            return None;
        }
    };
    if !depth_ok(&resolved) {
        diags.push(diag(
            LCode::TypeTooDeep,
            ty.span,
            format!("type nests deeper than the {MAX_TY_DEPTH}-level limit"),
        ));
        return None;
    }
    Some(resolved)
}

/// Whether `ty` is within `MAX_TY_DEPTH` (mirrors flow-ir's intake guard; lower
/// must reject deep tys before they reach a builder call, per LD12).
pub fn depth_ok(ty: &Ty) -> bool {
    let mut stack: Vec<(&Ty, usize)> = vec![(ty, 1)];
    while let Some((t, depth)) = stack.pop() {
        if depth > MAX_TY_DEPTH {
            return false;
        }
        match t {
            Ty::Tuple(ts) => {
                for inner in ts {
                    stack.push((inner, depth + 1));
                }
            }
            Ty::Struct { fields, .. } => {
                for (_, inner) in fields {
                    stack.push((inner, depth + 1));
                }
            }
            Ty::Array { elem, .. } => stack.push((elem, depth + 1)),
            _ => {}
        }
    }
    true
}

/// Resolve a top-level annotation ty against the type table (the common
/// caller-facing entry: params, returns, bind annotations). Wraps
/// [`resolve_ty`] with the table's `by_name`/`cyclic` context unavailable —
/// callers in later passes use [`TypeTable::resolve`] instead.
impl TypeTable {
    /// Resolve a surface annotation ty to a `flow_ir::Ty` using the table.
    /// Struct references resolve to the table's fully-expanded entry; scalars
    /// and composites resolve structurally. Reports L-codes into `diags`.
    pub fn resolve(
        &self,
        source: &str,
        ty: &SynTy,
        diags: &mut Vec<syn::Diagnostic>,
    ) -> Option<Ty> {
        let resolved = match &ty.kind {
            TyKind::Named(n) => {
                let text = name_text(source, *n);
                match text {
                    "i32" => Ty::i32(),
                    "i64" => Ty::i64(),
                    "u8" => Ty::u8(),
                    "f32" => Ty::f32(),
                    "f64" => Ty::f64(),
                    "bool" => Ty::Bool,
                    _ => match self.map.get(text) {
                        Some(t) => t.clone(),
                        None => {
                            diags.push(diag(
                                LCode::UnknownType,
                                n.span,
                                format!("unknown type `{text}`"),
                            ));
                            return None;
                        }
                    },
                }
            }
            TyKind::Tuple(ts) => {
                let mut out = Vec::with_capacity(ts.len());
                for inner in ts {
                    out.push(self.resolve(source, inner, diags)?);
                }
                Ty::Tuple(out)
            }
            TyKind::Array {
                elem,
                len,
                len_span,
            } => {
                if *len == 0 {
                    diags.push(diag(
                        LCode::EmptyArray,
                        *len_span,
                        "array type has size 0".to_string(),
                    ));
                    return None;
                }
                let e = self.resolve(source, elem, diags)?;
                Ty::Array {
                    elem: Box::new(e),
                    size: *len,
                }
            }
            TyKind::Dynamic(_) | TyKind::Error => {
                diags.push(diag(
                    LCode::UncleanTree,
                    ty.span,
                    "unclean type node reached lowering".to_string(),
                ));
                return None;
            }
        };
        if !depth_ok(&resolved) {
            diags.push(diag(
                LCode::TypeTooDeep,
                ty.span,
                format!("type nests deeper than the {MAX_TY_DEPTH}-level limit"),
            ));
            return None;
        }
        Some(resolved)
    }
}
