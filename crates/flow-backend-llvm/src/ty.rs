//! `Ty → LLVM type text` and the token-erasure remap (DESIGN §2).
//!
//! LLVM 15+ opaque pointers are assumed (`ptr` everywhere; the emitter targets a
//! modern clang, DESIGN §5.5). Two rules live here:
//!
//! - **Type lowering.** `i32→i32, i64→i64, u8→i8, f32→float, f64→double,
//!   bool→i1`, `Array{T,n}→[n x T]`, product → literal struct — all subject to
//!   the erased-representation rule. `Unit`/`IoToken`/`Str` have **no** LLVM
//!   value type (erased); `lower_ty` returns `None` for them and for any product
//!   whose residual is empty.
//! - **Erased-representation rule (DESIGN §2, S13).** A product's *residual* is
//!   its component list minus the components that lower to `None`
//!   (`IoToken`/`Unit`/`Str`). Residual arity ≥ 2 ⇒ `{…}` struct; residual arity
//!   1 ⇒ the object materializes as the **bare** surviving component type (no
//!   struct wrapper, so `Pair`/`Proj` are plain store/load, not GEP); residual
//!   arity 0 ⇒ no slot. The remap is derived on demand from the ty.

use flow_ir::Ty;

/// The LLVM value type for `ty`, or `None` if `ty` is erased (has no runtime
/// representation): `Unit`, `IoToken`, `Str`, or a product whose residual after
/// erasure is empty (DESIGN §2 / L4).
pub(crate) fn lower_ty(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Int { bits, .. } => Some(format!("i{bits}")),
        Ty::Float { bits: 32 } => Some("float".into()),
        Ty::Float { bits: 64 } => Some("double".into()),
        Ty::Float { .. } => None, // non-Core width; unreachable for sealed IR
        Ty::Bool => Some("i1".into()),
        Ty::Unit | Ty::Str | Ty::IoToken => None,
        Ty::Array { elem, size } => {
            let e = lower_ty(elem)?;
            Some(format!("[{size} x {e}]"))
        }
        Ty::Tuple(_) | Ty::Struct { .. } => {
            let kept: Vec<String> = residual_tys(ty);
            match kept.len() {
                0 => None,
                1 => Some(kept.into_iter().next().unwrap()),
                _ => Some(format!("{{ {} }}", kept.join(", "))),
            }
        }
    }
}

/// The LLVM type of a map/fold body's input product with the first `k` Array
/// components lowered to `ptr` — by-reference array captures (ADR-0027,
/// suggestions #6). Capture semantics are read-only, so the pointer is
/// observably identical to the inline array (the interp oracle is
/// representation-blind; the CUDA backend passes a device handle the same
/// way). A `ptr` component still survives erasure, so the residual arity and
/// `erased_index` remap are unchanged. `k = 0` (or a non-product) is
/// `lower_ty` verbatim.
pub(crate) fn lower_body_input_ty(ty: &Ty, k: u32) -> Option<String> {
    match ty {
        Ty::Tuple(_) | Ty::Struct { .. } if k > 0 => {
            let kept: Vec<String> = component_tys(ty)
                .iter()
                .enumerate()
                .filter_map(|(i, c)| match c {
                    Ty::Array { .. } if (i as u32) < k => Some("ptr".into()),
                    _ => lower_ty(c),
                })
                .collect();
            match kept.len() {
                0 => None,
                1 => Some(kept.into_iter().next().unwrap()),
                _ => Some(format!("{{ {} }}", kept.join(", "))),
            }
        }
        _ => lower_ty(ty),
    }
}

/// The lowered LLVM type of a **Named fn's** input with every top-level Array
/// (the input itself, or a direct product component) lowered to `ptr` —
/// by-reference array call arguments (BL5 amendment, suggestions #8). Flow
/// value semantics make array parameters observably read-only (functional
/// `Update` copies to a fresh alloca, so no callee writes through an array
/// argument), so the pointer is observably identical to the inline array — the
/// by-ref capture argument one call boundary down, and the interp oracle is
/// representation-blind. Reuses the capture lowering with `k = u32::MAX`: a
/// Named fn has no "leading captures" convention, so EVERY top-level Array
/// component qualifies; nested products-in-products stay by value (a
/// component that is itself a product lowers via `lower_ty`). Scalar-only
/// inputs lower identically to `lower_ty` (byte-identical text).
pub(crate) fn lower_named_input_ty(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Array { .. } => Some("ptr".into()),
        Ty::Tuple(_) | Ty::Struct { .. } => lower_body_input_ty(ty, u32::MAX),
        _ => lower_ty(ty),
    }
}

/// The lowered LLVM types of a product's surviving (non-erased) components, in
/// order. Empty for a fully-erased product or a non-product.
fn residual_tys(ty: &Ty) -> Vec<String> {
    component_tys(ty).iter().filter_map(lower_ty).collect()
}

/// The direct component tys of a Tuple/Struct (empty for anything else). Arrays
/// are homogeneous and never erasure-remapped, so they are excluded here.
fn component_tys(ty: &Ty) -> Vec<Ty> {
    match ty {
        Ty::Tuple(ts) => ts.clone(),
        Ty::Struct { fields, .. } => fields.iter().map(|(_, t)| t.clone()).collect(),
        _ => Vec::new(),
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

/// The erased slot index of original Tuple/Struct component `k`: its position
/// among the surviving components, or `None` if component `k` is itself erased.
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
