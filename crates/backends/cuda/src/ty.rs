//! `Ty → CUDA C++ type text` and the token-erasure remap (backend-cuda DESIGN
//! §5/L4 — the llvm `ty.rs` scheme, re-spelled for C++).
//!
//! Two rules live here:
//!
//! - **Type lowering.** `i32→int32_t, i64→int64_t, u8→uint8_t, f32→float,
//!   f64→double, bool→bool`; `Array{T,n}→T*` — a `DevHandle` host variable
//!   holding a device pointer, with `n` (and any static strides) tracked from
//!   the `Ty`, never in the text (L5/BC1); product → named residual struct —
//!   all subject to the erased-representation rule. `Unit`/`IoToken`/`Str`
//!   have **no** runtime representation on either site (erased, L4);
//!   `lower_ty` returns `None` for them and for any product whose residual is
//!   empty.
//! - **Erased-representation rule (unchanged from llvm).** A product's
//!   *residual* is its component list minus the components that lower to
//!   `None`. Residual arity ≥ 2 ⇒ struct type; residual arity 1 ⇒ the object
//!   materializes as the **bare** surviving component type (no struct
//!   wrapper); residual arity 0 ⇒ no slot. The remap is derived on demand
//!   from the ty.
//!
//! **Named-struct scheme (as-built, WP1).** C++ has no structural typing —
//! two textually equal anonymous struct definitions are *different* types, so
//! llvm's literal-struct text (`{ i32, i1 }`) cannot transfer to fn
//! signatures. A residual ≥ 2 product therefore lowers to a deterministic
//! name, `FlowProd_<comp>…` over the lowered component texts with `*`
//! sanitized to `p` (e.g. `Tuple[i32, bool]` → `FlowProd_int32_t_bool`,
//! `Tuple[f32*, i64]` → `FlowProd_floatp_int64_t`). The scheme is structural:
//! `Ty::Struct` names are ignored, exactly as llvm. The definition
//! (`struct <name> { <comp> f<i>; };`, fields named by erased slot index) is
//! emitted once per shape by module.rs (WP2+); ty.rs only names it.
//!
//! **Nested arrays (as-built, WP1).** `Array{Array{T,m},n}` peels to the
//! innermost element and lowers to `T*` — the DESIGN §5 flat aggregate with
//! static stride (llvm's nested `[n x [m x T]]` is likewise flat); strides
//! stay in the `Ty` for the WP2+ index arithmetic.

use flow_ir::Ty;

/// The CUDA C++ value type for `ty`, or `None` if `ty` is erased (has no
/// runtime representation): `Unit`, `IoToken`, `Str`, or a product whose
/// residual after erasure is empty (DESIGN L4 — same set as llvm).
pub(crate) fn lower_ty(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Int { bits, signed } => {
            let sign = if *signed { "" } else { "u" };
            Some(format!("{sign}int{bits}_t"))
        }
        Ty::Float { bits: 32 } => Some("float".into()),
        Ty::Float { bits: 64 } => Some("double".into()),
        Ty::Float { .. } => None, // non-Core width; unreachable for sealed IR
        Ty::Bool => Some("bool".into()),
        Ty::Unit | Ty::Str | Ty::IoToken => None,
        Ty::Array { .. } => {
            let e = lower_ty(array_base(ty))?;
            Some(format!("{e}*"))
        }
        Ty::Tuple(_) | Ty::Struct { .. } => {
            let kept: Vec<String> = residual_tys(ty);
            match kept.len() {
                0 => None,
                1 => Some(kept.into_iter().next().unwrap()),
                _ => Some(prod_name(&kept)),
            }
        }
    }
}

/// The innermost element ty of a (possibly nested) array — the flat
/// aggregate's base (DESIGN §5). Identity for non-arrays.
fn array_base(ty: &Ty) -> &Ty {
    let mut t = ty;
    while let Ty::Array { elem, .. } = t {
        t = elem;
    }
    t
}

/// The deterministic struct name for a residual ≥ 2 product shape, from the
/// lowered component texts (`*` → `p`; see the module doc). Two products with
/// the same surviving component types share one name — one definition serves
/// both, matching llvm's structural literal structs.
fn prod_name(kept: &[String]) -> String {
    let mut name = String::from("FlowProd");
    for c in kept {
        name.push('_');
        name.push_str(&c.replace('*', "p"));
    }
    name
}

/// The lowered C++ types of a product's surviving (non-erased) components, in
/// order. Empty for a fully-erased product or a non-product.
fn residual_tys(ty: &Ty) -> Vec<String> {
    component_tys(ty).iter().filter_map(lower_ty).collect()
}

/// The direct component tys of a Tuple/Struct (empty for anything else).
/// Arrays are homogeneous and never erasure-remapped, so they are excluded
/// here (llvm rule, verbatim).
fn component_tys(ty: &Ty) -> Vec<Ty> {
    match ty {
        Ty::Tuple(ts) => ts.clone(),
        Ty::Struct { fields, .. } => fields.iter().map(|(_, t)| t.clone()).collect(),
        _ => Vec::new(),
    }
}

/// The named-struct shape of a residual ≥ 2 product: `(FlowProd_* name,
/// lowered component texts)` in surviving order, or `None` for non-products
/// and residual ≤ 1 products. module.rs collects these for the struct
/// definitions; the name is exactly [`lower_ty`]'s, keeping the naming
/// contract single-sourced here.
pub(crate) fn prod_shape(ty: &Ty) -> Option<(String, Vec<String>)> {
    match ty {
        Ty::Tuple(_) | Ty::Struct { .. } => {
            let kept = residual_tys(ty);
            if kept.len() >= 2 {
                Some((prod_name(&kept), kept))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The residual arity of a Tuple/Struct: the count of components that survive
/// erasure. (Arrays/scalars return 0 — they are not products for the remap.)
pub(crate) fn residual_arity(ty: &Ty) -> usize {
    component_tys(ty)
        .iter()
        .filter(|t| lower_ty(t).is_some())
        .count()
}

/// Is `ty` a product whose residual contains an array — directly or through
/// nested surviving products (the F3 device-cell predicate)? Such a product
/// lowers to a struct with a `T*` field; on the device that field can only
/// hold a per-thread local-memory or buffer-interior pointer, both dangling
/// once the struct escapes into global memory (DESIGN §5's recorded
/// `Unsupported` cell). False for:
///
/// - non-products (incl. arrays OF products — sepia's `[Pixel; 16]` element
///   is a product of scalars, and the array itself is a flat AoS buffer);
/// - products of products whose surviving leaves are all scalars;
/// - erased components (an `Array{Unit}` has no representation anywhere).
pub(crate) fn residual_contains_array(ty: &Ty) -> bool {
    match ty {
        Ty::Tuple(_) | Ty::Struct { .. } => component_tys(ty)
            .iter()
            .any(|c| lower_ty(c).is_some() && tree_contains_array(c)),
        _ => false,
    }
}

/// Does `ty`'s surviving (non-erased) tree contain a product-with-array node
/// (recursing through arrays too)? The F3 element/return-type predicate: an
/// array OF products-with-arrays is a flat AoS buffer of pointer-field
/// structs — exactly as unemittable in global memory as the bare product.
/// (An array of SCALAR products — sepia's `[Pixel; 16]` — has no pointer
/// fields and does not fire.)
pub(crate) fn tree_contains_product_with_array(ty: &Ty) -> bool {
    if lower_ty(ty).is_none() {
        return false;
    }
    match ty {
        Ty::Array { elem, .. } => tree_contains_product_with_array(elem),
        Ty::Tuple(_) | Ty::Struct { .. } => {
            residual_contains_array(ty)
                || component_tys(ty)
                    .iter()
                    .any(tree_contains_product_with_array)
        }
        _ => false,
    }
}

/// Does `ty`'s surviving (non-erased) tree contain an array? Erasure-aware:
/// a subtree with no representation cannot hold a device pointer.
fn tree_contains_array(ty: &Ty) -> bool {
    if lower_ty(ty).is_none() {
        return false;
    }
    match ty {
        Ty::Array { .. } => true,
        Ty::Tuple(_) | Ty::Struct { .. } => component_tys(ty).iter().any(tree_contains_array),
        _ => false,
    }
}

/// The erased slot index of original Tuple/Struct component `k`: its position
/// among the surviving components, or `None` if component `k` is itself
/// erased.
pub(crate) fn erased_index(ty: &Ty, k: u32) -> Option<u32> {
    let comps = component_tys(ty);
    let target = comps.get(k as usize)?;
    lower_ty(target)?; // k itself erased ⇒ no remapped index
    let mut e = 0u32;
    for c in comps.iter().take(k as usize) {
        if lower_ty(c).is_some() {
            e += 1;
        }
    }
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arr(elem: Ty, size: u64) -> Ty {
        Ty::Array {
            elem: Box::new(elem),
            size,
        }
    }

    fn tup(ts: Vec<Ty>) -> Ty {
        Ty::Tuple(ts)
    }

    fn strukt(name: &str, fields: Vec<(&str, Ty)>) -> Ty {
        Ty::Struct {
            name: name.into(),
            fields: fields.into_iter().map(|(n, t)| (n.into(), t)).collect(),
        }
    }

    #[test]
    fn scalars_lower_to_cpp_spellings() {
        assert_eq!(lower_ty(&Ty::i32()).as_deref(), Some("int32_t"));
        assert_eq!(lower_ty(&Ty::i64()).as_deref(), Some("int64_t"));
        assert_eq!(lower_ty(&Ty::u8()).as_deref(), Some("uint8_t"));
        assert_eq!(lower_ty(&Ty::f32()).as_deref(), Some("float"));
        assert_eq!(lower_ty(&Ty::f64()).as_deref(), Some("double"));
        assert_eq!(lower_ty(&Ty::Bool).as_deref(), Some("bool"));
    }

    #[test]
    fn signedness_is_honored_and_non_core_floats_erase() {
        // Core admits only (32,s), (64,s), (8,u); the mapping is total over
        // the remaining Int combos, mirroring llvm's totality.
        assert_eq!(
            lower_ty(&Ty::Int {
                bits: 8,
                signed: true
            })
            .as_deref(),
            Some("int8_t")
        );
        assert_eq!(
            lower_ty(&Ty::Int {
                bits: 16,
                signed: false
            })
            .as_deref(),
            Some("uint16_t")
        );
        // Only 32/64 floats are Core (same None branch as llvm).
        assert!(lower_ty(&Ty::Float { bits: 16 }).is_none());
    }

    #[test]
    fn erased_trio_lowers_to_none() {
        assert!(lower_ty(&Ty::Unit).is_none());
        assert!(lower_ty(&Ty::Str).is_none());
        assert!(lower_ty(&Ty::IoToken).is_none());
    }

    #[test]
    fn arrays_lower_to_device_pointer_text() {
        assert_eq!(lower_ty(&arr(Ty::i32(), 4)).as_deref(), Some("int32_t*"));
        assert_eq!(lower_ty(&arr(Ty::f64(), 1)).as_deref(), Some("double*"));
        assert_eq!(lower_ty(&arr(Ty::Bool, 2)).as_deref(), Some("bool*"));
        // `n` stays in the Ty, not the text: different sizes, same handle.
        assert_eq!(
            lower_ty(&arr(Ty::i32(), 4)),
            lower_ty(&arr(Ty::i32(), 4096))
        );
        // An array of products points at the residual struct (AoS, DESIGN Dat).
        assert_eq!(
            lower_ty(&arr(tup(vec![Ty::i32(), Ty::Bool]), 3)).as_deref(),
            Some("FlowProd_int32_t_bool*")
        );
        // Erased element ⇒ no representation at all (llvm's `?`).
        assert!(lower_ty(&arr(Ty::Unit, 2)).is_none());
    }

    #[test]
    fn nested_arrays_peel_to_flat_base_pointer() {
        // DESIGN §5: nested arrays are flat aggregates with static stride —
        // the handle points at the innermost element.
        let matrix = arr(arr(Ty::i32(), 2), 3);
        assert_eq!(lower_ty(&matrix).as_deref(), Some("int32_t*"));
        let cube = arr(arr(arr(Ty::f32(), 2), 2), 2);
        assert_eq!(lower_ty(&cube).as_deref(), Some("float*"));
        // Peeling through to an erased base ⇒ None.
        let ghost = arr(arr(Ty::IoToken, 2), 3);
        assert!(lower_ty(&ghost).is_none());
    }

    #[test]
    fn products_residual_zero_erase() {
        assert!(lower_ty(&tup(vec![Ty::Unit, Ty::IoToken])).is_none());
        // The print-internal pair (Str, IoToken) erases fully.
        assert!(lower_ty(&tup(vec![Ty::Str, Ty::IoToken])).is_none());
        // A nested fully-erased product erases the outer residual too.
        assert!(lower_ty(&tup(vec![Ty::Unit, tup(vec![Ty::Unit, Ty::IoToken])])).is_none());
        let tok_struct = strukt("Tok", vec![("t", Ty::IoToken), ("u", Ty::Unit)]);
        assert!(lower_ty(&tok_struct).is_none());
    }

    #[test]
    fn products_residual_one_materialize_bare() {
        // The llvm residual rule: no struct wrapper for a single survivor.
        assert_eq!(
            lower_ty(&tup(vec![Ty::IoToken, Ty::i32()])).as_deref(),
            Some("int32_t")
        );
        assert_eq!(
            lower_ty(&tup(vec![Ty::Bool, Ty::Unit])).as_deref(),
            Some("bool")
        );
        // The survivor may itself be an array handle or a product name.
        assert_eq!(
            lower_ty(&tup(vec![Ty::Unit, arr(Ty::f32(), 8)])).as_deref(),
            Some("float*")
        );
        assert_eq!(
            lower_ty(&tup(vec![Ty::Unit, tup(vec![Ty::i32(), Ty::Bool])])).as_deref(),
            Some("FlowProd_int32_t_bool")
        );
    }

    #[test]
    fn products_residual_two_plus_get_named_struct() {
        assert_eq!(
            lower_ty(&tup(vec![Ty::i32(), Ty::Bool])).as_deref(),
            Some("FlowProd_int32_t_bool")
        );
        // Struct names are ignored — structural, exactly as llvm.
        let pixel = strukt(
            "Pixel",
            vec![("r", Ty::u8()), ("g", Ty::u8()), ("b", Ty::u8())],
        );
        assert_eq!(
            lower_ty(&pixel).as_deref(),
            Some("FlowProd_uint8_t_uint8_t_uint8_t")
        );
        assert_eq!(
            lower_ty(&tup(vec![Ty::u8(), Ty::u8(), Ty::u8()])),
            lower_ty(&pixel)
        );
        // Nested products compose by name.
        let nested = tup(vec![tup(vec![Ty::i32(), Ty::i32()]), Ty::Bool]);
        assert_eq!(
            lower_ty(&nested).as_deref(),
            Some("FlowProd_FlowProd_int32_t_int32_t_bool")
        );
        // Array components sanitize `*` → `p`.
        let with_arr = tup(vec![arr(Ty::f32(), 8), Ty::i64()]);
        assert_eq!(
            lower_ty(&with_arr).as_deref(),
            Some("FlowProd_floatp_int64_t")
        );
    }

    #[test]
    fn residual_arity_counts_survivors() {
        // Scalars and arrays are not products for the remap (llvm rule).
        assert_eq!(residual_arity(&Ty::i32()), 0);
        assert_eq!(residual_arity(&arr(Ty::i32(), 4)), 0);
        assert_eq!(residual_arity(&tup(vec![Ty::Unit, Ty::IoToken])), 0);
        assert_eq!(residual_arity(&tup(vec![Ty::IoToken, Ty::i32()])), 1);
        assert_eq!(residual_arity(&tup(vec![Ty::i32(), Ty::Bool])), 2);
        // A nested fully-erased product counts as erased.
        assert_eq!(
            residual_arity(&tup(vec![tup(vec![Ty::Unit, Ty::IoToken]), Ty::i32()])),
            1
        );
    }

    #[test]
    fn erased_index_remaps_past_erased_slots() {
        let t = tup(vec![Ty::i32(), Ty::IoToken, Ty::Bool, Ty::Unit, Ty::f64()]);
        assert_eq!(erased_index(&t, 0), Some(0));
        assert!(erased_index(&t, 1).is_none()); // erased slot has no index
        assert_eq!(erased_index(&t, 2), Some(1));
        assert!(erased_index(&t, 3).is_none());
        assert_eq!(erased_index(&t, 4), Some(2));
        assert!(erased_index(&t, 5).is_none()); // out of range
        // Non-products have no components to remap.
        assert!(erased_index(&Ty::i32(), 0).is_none());
        assert!(erased_index(&arr(Ty::i32(), 4), 0).is_none());
        // Struct fields remap by position, name-free.
        let w = strukt("W", vec![("tok", Ty::IoToken), ("x", Ty::i32())]);
        assert!(erased_index(&w, 0).is_none());
        assert_eq!(erased_index(&w, 1), Some(0));
    }

    #[test]
    fn residual_contains_array_predicate() {
        // Products with an array in the residual — the F3 cell.
        assert!(residual_contains_array(&tup(vec![
            arr(Ty::i32(), 2),
            Ty::i32()
        ])));
        assert!(residual_contains_array(&tup(vec![
            arr(Ty::i32(), 2),
            arr(Ty::i32(), 2)
        ])));
        // Nested products recurse.
        let deep = tup(vec![tup(vec![arr(Ty::i32(), 2), Ty::i32()]), Ty::Bool]);
        assert!(residual_contains_array(&deep));
        // Residual-1 with an array survivor.
        assert!(residual_contains_array(&tup(vec![
            Ty::Unit,
            arr(Ty::i32(), 2)
        ])));
        // NOT the cell: non-products (incl. arrays of products — the AoS
        // element struct has only scalar fields).
        assert!(!residual_contains_array(&arr(
            tup(vec![Ty::f32(), Ty::f32()]),
            16
        )));
        assert!(!residual_contains_array(&arr(Ty::i32(), 2)));
        // Products of scalars; products of products without arrays.
        assert!(!residual_contains_array(&tup(vec![Ty::i32(), Ty::Bool])));
        let pnp = tup(vec![tup(vec![Ty::i32(), Ty::i32()]), Ty::Bool]);
        assert!(!residual_contains_array(&pnp));
        // Erased array components have no representation (no pointer field).
        assert!(!residual_contains_array(&tup(vec![
            arr(Ty::Unit, 2),
            Ty::i32()
        ])));
        // Scalars.
        assert!(!residual_contains_array(&Ty::i32()));
    }

    #[test]
    fn tree_contains_product_with_array_predicate() {
        // Bare products fire (the residual predicate's domain).
        assert!(tree_contains_product_with_array(&tup(vec![
            arr(Ty::i32(), 2),
            Ty::i32()
        ])));
        // An array OF products-with-arrays: a flat AoS buffer of pointer-
        // field structs — fires through the array recursion.
        assert!(tree_contains_product_with_array(&arr(
            tup(vec![arr(Ty::i32(), 2), Ty::i32()]),
            4
        )));
        // Nested: array of arrays of products-with-arrays.
        assert!(tree_contains_product_with_array(&arr(
            arr(tup(vec![arr(Ty::i32(), 2), Ty::i32()]), 2),
            2
        )));
        // NOT: arrays of scalar products (no pointer fields anywhere).
        assert!(!tree_contains_product_with_array(&arr(
            tup(vec![Ty::f32(), Ty::f32()]),
            16
        )));
        assert!(!tree_contains_product_with_array(&arr(Ty::i32(), 2)));
        assert!(!tree_contains_product_with_array(&tup(vec![
            Ty::i32(),
            Ty::Bool
        ])));
        // Erased subtrees carry no representation.
        assert!(!tree_contains_product_with_array(&arr(Ty::Unit, 2)));
        assert!(!tree_contains_product_with_array(&Ty::i32()));
    }
}
