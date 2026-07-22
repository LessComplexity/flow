//! Smart arena allocation (suggestions.md #18; the authoritative spec is
//! `docs/components/backend-cuda/plans/plan-smart-arenas.md`, v1.0): the
//! deduced query `arena_plan : IR × FuncId → Option<ArenaPlan>` — the BL7
//! shape (a pure function of the sealed graph, never stored). One **FnScope
//! zone per fn** covers every NON-loop-cone buffer construction site: the
//! fn's N top-level `cudaMalloc`s collapse into ONE arena `cudaMalloc` at fn
//! entry, each member's device address is `arena0 + OFF` (a compile-time
//! constant, 256 B-aligned — the per-site pointer init replaces the site's
//! `cudaMalloc`), and the per-buffer epilogue frees collapse into one zone
//! release, vetoed iff an escape lvalue points into
//! `[arena0, arena0 + capacity)` (plan rule 3 — the escape guard keeps
//! comparing pointer VALUES, at zone granularity).
//!
//! Loop-cone sites keep per-buffer `cudaMalloc` (plan §5's recorded v1.0
//! scoping: per-iteration capacity is not statically bounded — distinct
//! iterations' buffers may coexist, so the O(k·n) residency is semantic, not
//! a leak); a cone Index/Fold site's 1-cell readback temp is a cone site too.
//! `d_trap` is outside the model entirely (plan §4). v1.1 (recorded, not
//! built): last-use coloring (capacity becomes max-clique, not sum) and
//! loop-cone zones with two-slot rotation.

use flow_ir::{CategoryIr, FuncId, MorphismId, ObjectId, Operation, Ty};
use slotmap::SecondaryMap;
use std::collections::HashMap;

use crate::EmitError;
use crate::kernel;
use crate::ty::lower_ty;

/// The recorded compile-time capacity guard (plan §5, rule 4 — the F7
/// `MAX_LOCAL_ARRAY_BYTES` precedent): a fn zone larger than this is
/// genuinely unsupported-scale today, so it is an honest
/// `EmitError::Unsupported` at emit time, never a device query. Covers the
/// bench family by 100×+; runtime allocation failure stays the `cu_check`
/// exit-102 channel. Revisit with ADR-0023 dynamic sizes.
pub(crate) const ARENA_MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Every zone offset is a multiple of 256 B (plan rule 2): cudaMalloc's own
/// alignment guarantee, preserved per buffer — member addresses stay
/// 256 B-aligned, so the epilogue guard's pointer-value comparisons remain
/// unique per buffer (no two buffers share an address).
fn align256(x: u64) -> u64 {
    kernel::align_up(x, 256)
}

/// A zone member's identity: an object-slot buffer (an array literal's or a
/// bulk-op's target object) or the 1-cell readback temp of a scalar-result
/// Index/Fold site (no ObjectId of its own — keyed by the site morphism).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum ArenaKey {
    Obj(ObjectId),
    Cell(MorphismId),
}

/// The deduced plan for one fn (plan §2's `ArenaPlan`): per-member byte
/// offset in the zone plus the zone capacity (`Σ align256(member bytes)` —
/// the disjoint layout of rule 1; every fn-scope buffer is live at fn
/// granularity in v1.0, so coloring degenerates to the sum; last-use
/// interference is v1.1). The offsets map is lookup-only (L2).
#[derive(Debug)]
pub(crate) struct ArenaPlan {
    offsets: HashMap<ArenaKey, u64>,
    /// The zone's total bytes (a multiple of 256).
    pub capacity: u64,
}

impl ArenaPlan {
    /// The member's byte offset in the zone, or `None` when the buffer is
    /// not a zone member (a loop-cone site — per-buffer `cudaMalloc`, v1.0).
    pub(crate) fn offset(&self, key: ArenaKey) -> Option<u64> {
        self.offsets.get(&key).copied()
    }
}

/// The fn's driver-owned (loop decide∪advance cone) morphisms — the same set
/// the walk skips (`FnEmit::walk`, the llvm rule). Cone sites keep
/// per-buffer `cudaMalloc` in v1.0 (plan §5).
fn cone_morphisms(ir: &CategoryIr, f: FuncId) -> SecondaryMap<MorphismId, ()> {
    let mut owned: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
    for scc in ir.loop_structure(f) {
        for &mg in &scc.merges {
            if let Some(plan) = ir.loop_plan(f, mg) {
                for &mo in plan.decide_order.iter().chain(plan.advance_order.iter()) {
                    owned.insert(mo, ());
                }
            }
        }
    }
    owned
}

/// `arena_plan` (plan §2): walk `topo_order` (deterministic — rule 6; the
/// assignment walk's order IS the offset order) and give every non-cone
/// buffer site the next 256 B-aligned offset. `None` when the fn has no zone
/// members (no arena emitted: scalar fns, `__host__ __device__` fns — the
/// qualifier rule keeps launch-form ops out of them — and fns whose only
/// buffers are cone sites). `Err(Unsupported)` over `ARENA_MAX_BYTES`
/// (rule 4, emit-time).
pub(crate) fn arena_plan(ir: &CategoryIr, f: FuncId) -> Result<Option<ArenaPlan>, EmitError> {
    let cone = cone_morphisms(ir, f);
    let mut plan = ArenaPlan {
        offsets: HashMap::new(),
        capacity: 0,
    };
    for m in ir.topo_order(f) {
        if cone.contains_key(m) {
            continue; // cone site: per-buffer cudaMalloc (v1.0)
        }
        let morph = ir.morphism(m).expect("morphism resolves");
        let src_ty = ir.object(morph.source).expect("source resolves").ty.clone();
        let tgt_ty = ir.object(morph.target).expect("target resolves").ty.clone();
        // The site's buffer(s), mirroring func.rs's emission conditions
        // exactly (the zone-vs-per-buffer decision itself is single-sourced:
        // `FnEmit::alloc_buffer` keys on this plan's `offset`).
        match morph.op {
            // Array literal (BC11): the construction emits at the target's
            // last Pair edge; the offset assigns at the first — same object,
            // same offset either way (`assign` is first-wins).
            Operation::Pair { .. }
                if matches!(tgt_ty, Ty::Array { .. }) && lower_ty(&tgt_ty).is_some() =>
            {
                assign(&mut plan, ArenaKey::Obj(morph.target), &tgt_ty);
            }
            Operation::Map { .. }
            | Operation::Zip
            | Operation::Enumerate
            | Operation::Iota
            | Operation::Fill
                if lower_ty(&tgt_ty).is_some() =>
            {
                assign(&mut plan, ArenaKey::Obj(morph.target), &tgt_ty);
            }
            Operation::Update => {
                let arr_ty = src_ty.component_ty(0).cloned().expect("update array");
                if lower_ty(&arr_ty).is_some() {
                    assign(&mut plan, ArenaKey::Obj(morph.target), &arr_ty);
                }
            }
            Operation::Index if lower_ty(&tgt_ty).is_some() => {
                if matches!(tgt_ty, Ty::Array { .. }) {
                    assign(&mut plan, ArenaKey::Obj(morph.target), &tgt_ty);
                } else {
                    assign_cell(&mut plan, ArenaKey::Cell(m), &tgt_ty);
                }
            }
            Operation::Fold { captures, .. } => {
                // ADR-0027: the acc is the source product's component k.
                let acc_ty = src_ty.component_ty(captures).cloned().expect("fold acc");
                if lower_ty(&tgt_ty).is_some() {
                    if matches!(acc_ty, Ty::Array { .. }) {
                        assign(&mut plan, ArenaKey::Obj(morph.target), &acc_ty);
                    } else {
                        assign_cell(&mut plan, ArenaKey::Cell(m), &tgt_ty);
                    }
                }
            }
            _ => {}
        }
    }
    if plan.offsets.is_empty() {
        return Ok(None);
    }
    // Rule 4's compile-time guard (the F7 precedent, plan §5).
    if plan.capacity > ARENA_MAX_BYTES {
        return Err(EmitError::Unsupported {
            feature: format!(
                "fn arena over {ARENA_MAX_BYTES} bytes ({} bytes)",
                plan.capacity
            ),
            loc: ir.func(f).expect("func resolves").loc,
        });
    }
    Ok(Some(plan))
}

/// Assign an array-typed buffer the next aligned offset (first assignment
/// wins — a literal's many Pair edges assign once). `bytes` is the ABI-exact
/// flat buffer size (kernel::buffer_bytes_of).
fn assign(plan: &mut ArenaPlan, key: ArenaKey, arr_ty: &Ty) {
    if plan.offsets.contains_key(&key) {
        return;
    }
    let bytes = kernel::buffer_bytes_of(arr_ty).expect("array base lowers");
    plan.offsets.insert(key, plan.capacity);
    plan.capacity += align256(bytes);
}

/// Assign a 1-cell readback temp (an Index/Fold scalar-or-product result —
/// the `sizeof({ct})` cell of func.rs's site).
fn assign_cell(plan: &mut ArenaPlan, key: ArenaKey, ty: &Ty) {
    let bytes = kernel::abi_sizeof(ty).expect("cell type lowers");
    plan.offsets.insert(key, plan.capacity);
    plan.capacity += align256(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use flow_ir::{Dest, FuncKind, IrBuilder, SourceLoc, Value};

    const L: SourceLoc = SourceLoc { start: 0, end: 0 };

    fn lower_src(src: &str) -> CategoryIr {
        let po = flow_syntax::parse(src);
        assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
        flow_lower::lower(src, &po.program).unwrap_or_else(|d| panic!("lower: {d:?}"))
    }

    /// The plan of the module's entry fn.
    fn entry_plan(ir: &CategoryIr) -> Option<ArenaPlan> {
        arena_plan(ir, ir.entry()).unwrap()
    }

    #[test]
    fn scalar_fns_have_no_zone() {
        let ir = lower_src("fn main() {\n    7 -> println;\n}\n");
        assert!(entry_plan(&ir).is_none());
    }

    #[test]
    fn offsets_are_disjoint_aligned_and_topo_ordered() {
        // vector_add's main: two literals (64 B each), a Zip output
        // (FlowProd_int32_t_int32_t × 16 = 128 B), a Map output (64 B), and
        // three 1-cell readback temps (4 B each) — every member 256 B-slot
        // aligned, capacity = 7 × 256.
        let path = format!(
            "{}/../../examples/vector_add.flow",
            env!("CARGO_MANIFEST_DIR")
        );
        let src = std::fs::read_to_string(&path).unwrap();
        let ir = lower_src(&src);
        let plan = entry_plan(&ir).expect("vector_add's main has a zone");
        assert_eq!(plan.capacity, 7 * 256, "{:?}", plan.offsets);
        let mut offs: Vec<u64> = plan.offsets.values().copied().collect();
        offs.sort_unstable();
        assert_eq!(offs, vec![0, 256, 512, 768, 1024, 1280, 1536]);
    }

    #[test]
    fn cone_sites_are_not_zone_members() {
        // The loop-driven Update program: build's literal is a zone member,
        // the advance-cone Update site is NOT (v1.0's recorded scoping).
        let src = r#"
fn build(n: i32) -> [i32; 4] {
    [0, 0, 0, 0] -> z: [i32; 4];
    mut c: [i32; 4] <- z;
    mut t: i32 <- 0;
    loop {
        (t < n) -> {
            -true-> { c[t] <- t * 10; t + 1 -> t; -> loop; }
            -false-> c -> ret;
        }
    }
}
fn main() {
    4 -> build -> c;
    c[2] -> println;
}
"#;
        let ir = lower_src(src);
        let build = ir
            .funcs()
            .find(|(_, fd)| fd.name == "build")
            .map(|(id, _)| id)
            .expect("build fn");
        let plan = arena_plan(&ir, build).unwrap().expect("build has a zone");
        // Exactly ONE member (the literal z, 16 B → one 256 B slot); the
        // cone Update site and its per-iteration buffer stay per-buffer.
        assert_eq!(plan.offsets.len(), 1, "{:?}", plan.offsets);
        assert_eq!(plan.capacity, 256);
        // main's Index readback cell is a fn-zone member of main's zone.
        let main_plan = entry_plan(&ir).expect("main has a zone");
        assert_eq!(main_plan.offsets.len(), 1);
        assert_eq!(main_plan.capacity, 256);
    }

    #[test]
    fn product_elements_use_abi_sizes() {
        // An array of a padded product: Cell { f64, i32 } is 16 B by C
        // layout (8 + 4, tail-padded to 8) — nominal_sizeof would say 12.
        // Literal 2 × 16 = 32 B → one 256 B slot; the Index readback cell
        // (one Cell) → another.
        let src = "type Cell { v: f64, i: i32 }\n\
                   fn main() {\n    \
                   [Cell { v: 1.0, i: 1 }, Cell { v: 2.0, i: 2 }] -> cs: [Cell; 2];\n    \
                   cs[0] -> c;\n    c.i -> println;\n}\n";
        let ir = lower_src(src);
        let plan = entry_plan(&ir).expect("a zone");
        assert_eq!(plan.capacity, 512, "{:?}", plan.offsets);
        assert_eq!(plan.offsets.len(), 2, "{:?}", plan.offsets);
    }

    #[test]
    fn over_capacity_is_unsupported() {
        // Rule 4: one Update on a borrowed [f64; 536870913] input — the
        // target buffer alone is 8 × 536870913 = 4294967304 B > 4 GiB.
        let mut b = IrBuilder::new();
        let arr_ty = Ty::Array {
            elem: Box::new(Ty::f64()),
            size: 536870913,
        };
        let f = b
            .declare(FuncKind::Named, "main", arr_ty.clone(), arr_ty.clone(), L)
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let i = fb.input();
            let idx = fb.constant(Value::I64(0), L).unwrap();
            let v = fb.constant(Value::F64(1.0), L).unwrap();
            fb.update(i, idx, v, Dest::Ret { slot: None }, L).unwrap();
            fb.finish().unwrap();
        }
        let ir = b.seal(f).unwrap();
        match arena_plan(&ir, ir.entry()) {
            Err(EmitError::Unsupported { feature, .. }) => {
                assert!(feature.contains("fn arena over"), "{feature}");
                // 8 × 536870913 = 4294967304 B, 256 B-aligned ⇒ 4294967552.
                assert!(feature.contains("4294967552"), "{feature}");
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
        // And the whole-module emit surfaces the same cell (the F7 shape).
        match crate::emit(&ir) {
            Err(EmitError::Unsupported { feature, .. }) => {
                assert!(feature.contains("fn arena over"), "{feature}")
            }
            other => panic!("expected Unsupported from emit, got {other:?}"),
        }
    }

    #[test]
    fn at_capacity_emits() {
        // Exactly 4 GiB (8 × 536870912) is NOT over the guard — the F7
        // precedent's at-budget boundary.
        let mut b = IrBuilder::new();
        let arr_ty = Ty::Array {
            elem: Box::new(Ty::f64()),
            size: 536870912,
        };
        let f = b
            .declare(FuncKind::Named, "main", arr_ty.clone(), arr_ty.clone(), L)
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let i = fb.input();
            let idx = fb.constant(Value::I64(0), L).unwrap();
            let v = fb.constant(Value::F64(1.0), L).unwrap();
            fb.update(i, idx, v, Dest::Ret { slot: None }, L).unwrap();
            fb.finish().unwrap();
        }
        let ir = b.seal(f).unwrap();
        let plan = entry_plan(&ir).expect("at-budget zone plans fine");
        assert_eq!(plan.capacity, ARENA_MAX_BYTES);
        let cu = crate::emit(&ir).unwrap();
        assert!(
            cu.contains("cudaMalloc((void**)&arena0, 4294967296ULL)"),
            "{cu}"
        );
    }
}
