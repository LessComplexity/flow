//! `emit_fn` (DESIGN §2/§5): entry-block allocas, the `topo_order` walk, and the
//! full op table over per-object stack slots. One `alloca` per materialized
//! object; a morphism loads its operand slots, computes, stores its target slot
//! — the piecewise functor application (§8.5).

use mapal_ir::{
    BoundsProof, CategoryIr, ElemPlan, ElemSrc, FuncId, FuncKind, GuardSite, LastUsePlan,
    MorphismId, ObjectId, ObjectKind, Operation, PathPlan, TaskKind, TilePlan, TileSite, Ty, Value,
    WaitEntry,
};
use slotmap::SecondaryMap;

use crate::module::StrGlobal;
use crate::profile::TargetProfile;
use crate::ty::{
    erased_index, lower_body_input_ty, lower_named_input_ty, lower_ty, residual_arity,
};

/// Truthful fn attributes (trap-aware; suggestions #7): the conservative
/// syntactic capability set. A fn is **clean** — pure by construction — when it
/// has no integer `Div`/`Mod` (zero/`MIN/-1` guards call `mapal_trap`), no
/// trap-capable `Index`/`Update` (an unproven `Index`'s bounds guard calls
/// `mapal_trap`; an S20 `bounds_proof`-proven `Index` can never fire — its guard
/// is elided, so it does not count), no `Print`/token use (the token-threaded
/// `mapal_print_*` externs are not readonly), and — transitively, callerward
/// over `Call` and `Map`/`Fold` body edges — every callee is equally clean (the
/// spirit of the CUDA backend's `kernel.rs:TrapCaps` fixpoint).
///
/// A clean fn reads memory (by-ref array arguments) but never writes state
/// visible to its caller, never unwinds, and — unless its closure can loop —
/// always returns: clean fns get `readonly nounwind`, plus `willreturn` when
/// the transitive closure has no `LoopEnter` and no call cycle (a possibly
/// infinite loop or unbounded recursion would make `willreturn` a lie — UB).
/// Unclean fns get nothing. The rule is syntactic on the op set apart from the
/// `Index` proof, like `TrapCaps`: an integer `Div`/`Mod` counts even when a
/// future elision could remove its guard — keeping the fn attribute-free is
/// harmless, and the two passes stay independently simple.
pub(crate) struct FnAttrs {
    clean: SecondaryMap<FuncId, bool>,
    loopy: SecondaryMap<FuncId, bool>,
}

impl FnAttrs {
    pub(crate) fn analyze(ir: &CategoryIr) -> FnAttrs {
        let mut clean: SecondaryMap<FuncId, bool> = SecondaryMap::new();
        let mut loopy: SecondaryMap<FuncId, bool> = SecondaryMap::new();
        let mut edges: SecondaryMap<FuncId, Vec<FuncId>> = SecondaryMap::new();
        for (f, fd) in ir.funcs() {
            let bp = ir.bounds_proof(f);
            let mut unclean = false;
            let mut has_loop = false;
            for &m in &fd.morphisms {
                let morph = ir.morphism(m).expect("morphism resolves");
                match morph.op {
                    Operation::Div | Operation::Mod => {
                        let src_ty = &ir.object(morph.source).expect("source resolves").ty;
                        if matches!(src_ty.component_ty(0), Some(Ty::Int { .. })) {
                            // #13 credit (S20): a literal non-zero, non-−1
                            // constant divisor cannot trap — the guard is dead.
                            let safe = const_int_operand(ir, morph.source, 1)
                                .is_some_and(|v| v != 0 && v != -1);
                            if !safe {
                                unclean = true;
                            }
                        }
                    }
                    // A proven `Index` can never fire (S20 `bounds_proof`) —
                    // its guard is elided in emission, so it is not
                    // trap-capable and does not count (a proven one fails the
                    // guard and lands on the wildcard arm). `Update` stays
                    // counted (conservative this wave): its OOB class differs
                    // only if proven too.
                    Operation::Index if !bp.proven(m) => {
                        unclean = true;
                    }
                    Operation::Update | Operation::Print { .. } => {
                        unclean = true;
                    }
                    Operation::LoopEnter => has_loop = true,
                    Operation::Call(g) => {
                        let mut v = edges.get(f).cloned().unwrap_or_default();
                        v.push(g);
                        edges.insert(f, v);
                    }
                    Operation::Map { body, .. } | Operation::Fold { body, .. } => {
                        let mut v = edges.get(f).cloned().unwrap_or_default();
                        v.push(body);
                        edges.insert(f, v);
                    }
                    _ => {}
                }
            }
            // Token use without a `Print` (a token threaded through in/out or a
            // product): conservative — the fn may reach `mapal_print_*` through
            // any path, so it stays attribute-free.
            if !unclean {
                unclean = ir
                    .objects()
                    .any(|(id, obj)| ir.try_owner(id) == Some(f) && ty_has_token(&obj.ty));
            }
            clean.insert(f, !unclean);
            loopy.insert(f, has_loop);
        }
        // A call/body cycle can recurse without bound — not `willreturn` even
        // when clean. Seed `loopy` with every fn that can reach itself, then
        // propagate callerward with the same fixpoint.
        for (f, _) in ir.funcs() {
            if reaches_self(ir, &edges, f) {
                loopy.insert(f, true);
            }
        }
        loop {
            let mut changed = false;
            for (f, _) in ir.funcs() {
                for g in edges.get(f).cloned().unwrap_or_default() {
                    if clean.get(f).copied().unwrap_or(false)
                        && !clean.get(g).copied().unwrap_or(false)
                    {
                        clean.insert(f, false);
                        changed = true;
                    }
                    if !loopy.get(f).copied().unwrap_or(false)
                        && loopy.get(g).copied().unwrap_or(false)
                    {
                        loopy.insert(f, true);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        FnAttrs { clean, loopy }
    }

    /// `f` is in the clean set (`false` for unknown ids — a sealed graph's own
    /// fns are all present).
    pub(crate) fn clean(&self, f: FuncId) -> bool {
        self.clean.get(f).copied().unwrap_or(false)
    }

    /// `f`'s transitive closure can loop or recurse — withhold `willreturn`
    /// (`true` for unknown ids: the conservative answer).
    pub(crate) fn loopy(&self, f: FuncId) -> bool {
        self.loopy.get(f).copied().unwrap_or(true)
    }
}

/// `true` when `f` can reach itself through `edges` (a call/body cycle).
fn reaches_self(ir: &CategoryIr, edges: &SecondaryMap<FuncId, Vec<FuncId>>, f: FuncId) -> bool {
    let mut seen: SecondaryMap<FuncId, ()> = SecondaryMap::new();
    let mut stack = edges.get(f).cloned().unwrap_or_default();
    while let Some(g) = stack.pop() {
        if g == f {
            return true;
        }
        if seen.insert(g, ()).is_some() {
            continue;
        }
        if ir.func(g).is_some() {
            stack.extend(edges.get(g).cloned().unwrap_or_default());
        }
    }
    false
}

/// Does `ty` mention `IoToken` anywhere (top level or nested in a product)?
fn ty_has_token(ty: &Ty) -> bool {
    match ty {
        Ty::IoToken => true,
        Ty::Tuple(ts) => ts.iter().any(ty_has_token),
        Ty::Struct { fields, .. } => fields.iter().any(|(_, t)| ty_has_token(t)),
        _ => false,
    }
}

#[derive(Clone, Copy)]
enum GuardFlavor {
    Host,
    Task,
    TaskBody(u32),
}

#[derive(Clone)]
struct FrameField {
    owner: ObjectId,
    index: u32,
    ordinal: u32,
    llt: String,
}

#[derive(Clone)]
struct FrameLayout {
    fields: SecondaryMap<ObjectId, FrameField>,
    order: Vec<ObjectId>,
    packed: SecondaryMap<MorphismId, PackedField>,
}

impl FrameLayout {
    /// The literal-struct spelling of `%Frame` — the named-type definition's
    /// body, and the text `llt_bytes` sizes for the heap-lowering gate.
    fn struct_llt(&self) -> String {
        let mut fields = self
            .order
            .iter()
            .map(|o| self.fields[*o].llt.clone())
            .collect::<Vec<_>>();
        fields.extend((0..self.packed.len()).map(|_| "ptr".to_owned()));
        format!("{{ {} }}", fields.join(", "))
    }

    fn definition(&self) -> String {
        format!("%Frame = type {}\n", self.struct_llt())
    }
}

#[derive(Clone)]
struct PackedField {
    index: u32,
    ordinal: u32,
}

#[derive(Clone)]
struct CheckpointEmit {
    ordinal: usize,
    topo: u32,
    len: usize,
}

#[derive(Clone)]
struct PinnedEmit {
    task: usize,
    topo: u32,
    len: usize,
}

struct HostEmit {
    checkpoints: SecondaryMap<MorphismId, CheckpointEmit>,
    pinned: SecondaryMap<MorphismId, PinnedEmit>,
    /// Checkpoints living INSIDE an effectful loop, keyed by that loop's first
    /// `LoopEnter` in topo order: the loop's seed/entry glue reads
    /// task-produced frame slots, so the wait+check must also fire once BEFORE
    /// the loop is entered — the per-iteration hook inside the cone comes too
    /// late for the first read (S24 review find).
    pre_loop: SecondaryMap<MorphismId, Vec<CheckpointEmit>>,
}

/// Per-function emission state (DESIGN Dat `FnCtx`). `slots` is partial — erased
/// (token/unit/str) objects have no slot.
pub(crate) struct FnEmit<'a> {
    pub ir: &'a CategoryIr,
    pub f: FuncId,
    pub fnames: &'a SecondaryMap<FuncId, String>,
    pub strings: &'a SecondaryMap<ObjectId, StrGlobal>,
    /// The module's attribute-capability pre-pass (suggestions #7): clean fns
    /// get `readonly nounwind` (+ `willreturn` when loop-free), unclean fns
    /// nothing.
    pub attrs: &'a FnAttrs,
    pub slots: SecondaryMap<ObjectId, String>,
    pub allocas: String,
    pub body: String,
    pub next: u32,
    /// By-ref array input state: `(input object, by-ref prefix k, by-ref
    /// input-struct text)` — the first-`k` Array components of the fn input
    /// arrive as `ptr`. For a Map/Fold body fn `k` is the site's capture count
    /// (ADR-0027, suggestions #6); for a `Named` fn `k = u32::MAX` — every
    /// top-level Array component (BL5 by-ref call args, suggestions #8).
    byref: Option<(ObjectId, u32, String)>,
    /// The objects holding a forwarded array pointer (`alloca ptr`): array
    /// objects produced by `Proj{index < k}` from the by-ref input, plus the
    /// input object itself when a Named fn's whole input is one bare Array.
    ptr_resident: SecondaryMap<ObjectId, ()>,
    /// The fn's last-use plan (docs/components/ir/plans/plan-last-use.md §2) —
    /// the single source of dead/escape/carried facts for the `Update` memcpy
    /// elision (suggestions #2); never re-derived locally.
    lup: LastUsePlan,
    /// The fn's bounds-proof plan (mapal-ir `algo.rs:bounds_proof`, S20) — the
    /// provably-in-bounds `Index` set backing the guard elision: a proven
    /// `Index` can never fire, so `emit_index` drops its trap guard (just the
    /// GEP+load); everything unproven keeps today's guard byte-identical.
    bp: BoundsProof,
    /// plan-s39 guard sites keyed by their `Phi`: the condition picks the arm
    /// and only that arm's work is executed (branch emission).
    gsites: SecondaryMap<MorphismId, GuardSite>,
    /// Guard-arm-owned morphisms — emitted only inside their Phi's branches;
    /// every straight-line walk skips them, and the loop driver's cones skip
    /// them too (plan-s40 — a nested arm inside a loop body fires from its
    /// in-cone Phi).
    pub(crate) gated: SecondaryMap<MorphismId, ()>,
    /// `Update` targets whose whole-array memcpy is elided (rule 4's in-place
    /// write): they share the dead source array's slot, so the entry-block
    /// pass mints no alloca for them; `emit_update` inserts the shared slot.
    elided_updates: SecondaryMap<ObjectId, ()>,
    /// Elided Update target → source slot, retained explicitly for frame users
    /// in other task functions.
    update_aliases: SecondaryMap<ObjectId, ObjectId>,
    /// Parallel entry storage. Task emitters resolve fields lazily into
    /// `frame_geps`; the host resolves every field once in its prologue.
    frame: Option<FrameLayout>,
    frame_geps: String,
    /// Host guards call `mapal_trap`; task/body guards record into the run.
    guard_flavor: GuardFlavor,
    /// Split-task collection loops use `%lo..%hi`; every other loop keeps
    /// today's `0..n`.
    split_range: bool,
    /// Scalar-chain tasks publish progress after each trap-capable site.
    watermark: bool,
    /// Parallel host checkpoint/pinned injections.
    host: Option<HostEmit>,
    /// A task-flavor body only loses readonly attributes if it actually emits a
    /// runtime-state write.
    runtime_write: bool,
    /// Opt-in mapal_main compute timer; task and body functions stay untouched.
    perf_timing: bool,
    /// Matmul-shaped map sites recognized once for this function.
    tile_plan: Option<TilePlan>,
    /// plan-s37-stage-structure: the per-element law of each array. Consulted
    /// at elementwise reads; absence means "load it", i.e. today's behavior.
    elem: ElemPlan,
    /// Arrays whose every consumer builds the element from its law, so the
    /// buffer is never read and need not exist (step 3b). The producer emits
    /// no store loop and the object gets no `%Frame` field — the `elided_updates`
    /// pattern, applied to a producer instead of an in-place `Update`.
    elided_arrays: SecondaryMap<ObjectId, ()>,
    /// Pack tiled two-dimensional right-hand operands.
    packing: bool,
    /// Product-face FMA contraction, used only by the tiled per-cell chain.
    contract: bool,
    /// Split deep packed sites into k-panels (the KC nest). Default OFF — a
    /// measured 3x loss locally at 1024 f32 (S29); see `EmitOpts::kc_nest`.
    kc_nest: bool,
    /// The machine facts every tile factor derives from (plan-s31-target-
    /// profiles): vector width and register count, L2 budget, stack ceiling.
    /// Geometry comes from the record, constants come from here — mapal-ir never
    /// learns any of it (the backend-genericity contract).
    profile: &'static TargetProfile,
    /// This emitter's entry block runs **exactly once per program**, so a block
    /// it allocates may live in the `mapal_rt_alloc` arena and be released by
    /// one `mapal_rt_free_all` before its `ret` (plan-s29 composition rule 4).
    /// True only for the entry function's own emitter — the sequential
    /// `mapal_main` and the parallel host. Task/slice/body/named functions run
    /// an unbounded number of times and keep their `alloca`s, which is also why
    /// the teardown can free everything: nobody else registers a block.
    heap_ok: bool,
    /// This emitter lowered at least one block to the arena ⟹ it owes a
    /// teardown.
    heap_used: bool,
}

pub(crate) fn packing_site(site: &TileSite) -> bool {
    // k-split sites never pack: the packed panel layout has no encoding for
    // the (k÷div, k%div) decomposition.
    site.rows > 1 && site.b.ci == 0 && site.b.ksplit.is_none()
}

/// FIR-style 1-D sliding-window sites: a single-row map whose unit-stride
/// read `b` slides with the fold (`ck == 1`) while `a` is the invariant read
/// (`clane == 0` by the recognition invariant) — the rung-2 dual.
pub(crate) fn window1d_site(site: &TileSite) -> bool {
    site.rows == 1 && site.b.ck == 1 && site.b.ksplit.is_none()
}

/// conv2d-style k-split sites: the sliding read `b` is affine in the fold's
/// derived axes (`k÷div`, `k%div`) — never raw `k` (rule 1) — while `a` stays
/// the plain broadcast read. The micro-kernel unrolls the (kq, kr) taps at
/// compile-time offsets; every other k-split shape keeps the untiled
/// fallback (rule 3).
pub(crate) fn conv_site(site: &TileSite) -> bool {
    site.a.ksplit.is_none()
        && site.b.ksplit.is_some()
        && site.b.ck == 0
        && site.a.clane == 0
        && site.b.clane == 1
}

/// The FIR window rung's lane-block multiplier: full blocks step
/// `WINDOW_SUBROWS × TJ` lanes, subrow `r` living at accumulator offset
/// `r · TJ`.
///
/// **This is NOT `TargetProfile::tile_i`, despite sharing its value today.**
/// That one counts rows of `phi <TJ x elem>` accumulators and is bounded by the
/// vector register file. This one multiplies a `[TI·TJ x elem]` **memory**
/// accumulator (`emit_tile_trio_vec` is unreachable from
/// `emit_tile_window_block`), so no register budget binds it, and deriving it
/// from one would be deriving it from a constraint that does not apply here.
/// Unjustified at 4 — swept once alongside the matmul rung and never
/// separately (plan-s31-target-profiles work item 2; ADR-0034 would search it).
const WINDOW_SUBROWS: u64 = 4;

/// The vector accumulator type of one j-tile: `<TJ x elem>`, deliberately the
/// FULL tile width rather than the target's vector width — LLVM legalizes it
/// to whatever the machine has, so the emitter stays target-independent and
/// `TILE_J` never couples to a hardware register size (plan-s30).
fn tile_vec_llt(ctx: &TileCtx) -> String {
    vec_llt(&ctx.elem_llt, ctx.tile_j)
}

/// The same, for the conv rung — which carries its own context type but the
/// same `<TJ x elem>` accumulator shape.
fn vec_llt(elem_llt: &str, tile_j: u64) -> String {
    format!("<{tile_j} x {elem_llt}>")
}

/// Heap-lowering threshold (plan-s29 emission item 4): an entry-block block of
/// at least this many bytes is placed in the `mapal_rt_alloc` arena instead of
/// the stack. A target fact, not a language one (macOS caps the main thread at
/// 64 MB hard, so 2048² f32 ×3 plus the packed panel cannot live there) — since
/// S31 it is `TargetProfile::heap_min_bytes` rather than a literal here.
///
/// 256 KB sits far below anything that threatens a stack, so no program that
/// fits today changes a byte. It also sits above every tile scratch the emitter
/// mints — but that headroom is **profile-dependent, not the flat 2 KB the S29
/// note claimed**: the KC a-panel is `tile_i × tile_kc × sizeof`, which reduces
/// to `2 · l2_bytes / nc_tiles` — 2 KB under `generic`'s 512 KB L2, but 64 KB
/// under `apple-m`'s 16 MB. It grows linearly with the profile's L2, so a
/// profile past ~64 MB of L2 would push the a-panel over this threshold. No
/// shipped profile is close, and `scratch()` bypasses `entry_alloc` anyway, so
/// this is a bound to know rather than a bug to fix.

/// The size in bytes of an emitted LLVM type text, under the LLVM data layout
/// (`StructLayout`: each field at its own ABI alignment, tail-padded to the
/// widest). The grammar is closed — `ty.rs` emits only `ptr`, `float`,
/// `double`, `iN`, `[n x T]` and `{ T, … }` — and anything unrecognised sizes
/// to `0`, which keeps it on the stack (the conservative direction).
fn llt_bytes(llt: &str) -> u64 {
    let llt = llt.trim();
    if let Some(inner) = llt.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let Some((n, elem)) = inner.split_once(" x ") else {
            return 0;
        };
        return n.trim().parse::<u64>().unwrap_or(0) * llt_bytes(elem);
    }
    if let Some(inner) = llt.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        let mut size = 0u64;
        let mut align = 1u64;
        for field in llt_fields(inner) {
            let a = llt_align(field);
            size = size.next_multiple_of(a) + llt_bytes(field);
            align = align.max(a);
        }
        return size.next_multiple_of(align);
    }
    match llt {
        "ptr" | "double" => 8,
        "float" => 4,
        _ => llt
            .strip_prefix('i')
            .and_then(|bits| bits.parse::<u64>().ok())
            .map_or(0, |bits| bits.div_ceil(8)),
    }
}

/// The ABI alignment of an emitted LLVM type text: scalars at width, an array
/// at its element's, a struct at its widest field's.
fn llt_align(llt: &str) -> u64 {
    let llt = llt.trim();
    if let Some(inner) = llt.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return inner
            .split_once(" x ")
            .map_or(1, |(_, elem)| llt_align(elem));
    }
    if let Some(inner) = llt.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        return llt_fields(inner)
            .into_iter()
            .map(llt_align)
            .max()
            .unwrap_or(1);
    }
    llt_bytes(llt).max(1)
}

/// Split a literal struct's body on its top-level commas (a field may itself be
/// an array or a struct, whose commas do not separate).
fn llt_fields(inner: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    for (i, c) in inner.char_indices() {
        match c {
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = inner[start..].trim();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

/// The KC rung's k-panel width: packed sites deeper than this split the fold
/// into [kc, kc+TILE_KC) panels so the per-panel b slice (TILE_KC×TJ lanes)
/// and the packed a block stay L1-resident. Shallower sites keep the j-outer
/// nest byte-for-byte (the negative control).

#[derive(Clone)]
struct PackedBuffer {
    ptr: String,
    llt: String,
}

/// Shared emission context for one gated tiled site's lane-loop trio: every
/// SSA name and lowered type the seed/k/store loops need, built once per site.
struct TileCtx {
    acc: String,
    acc_llt: String,
    elem_llt: String,
    seed: String,
    mul_op: &'static str,
    add_op: &'static str,
    a_ptr: String,
    b_ptr: String,
    out_ptr: String,
    a_llt: String,
    b_llt: String,
    out_llt: String,
    k_ctr: String,
    lane_ctr: String,
    tile_j: u64,
    /// The rung's block factor. Matmul: `TargetProfile::tile_i` — rows of
    /// vector accumulators, bounded by the register file. FIR window rung:
    /// [`WINDOW_SUBROWS`] — a lane-block multiplier over a memory accumulator.
    /// Same number today, different quantities; each is set at its own source.
    tile_i: u64,
    /// The KC rung's k-panel depth (`TargetProfile::tile_kc`), sized against
    /// half of L2 at this site's element width.
    tile_kc: u64,
    packed: Option<PackedBuffer>,
    contract_flag: &'static str,
}

/// Emission context for one conv site's unrolled tap nest: every SSA name and
/// lowered type the seed/tap/store lane loops need, built once per site. The
/// k-split sibling of `TileCtx` — no k counter: the (kq, kr) taps unroll.
struct ConvTileCtx {
    /// The j-tile width in lanes: the constant-TJ main tile runs on a
    /// `<TJ x elem>` SSA accumulator, the runtime-`tj` remainder on the memory
    /// form (plan-s31-deduced-blocking work item 2, the S30 carve-out).
    tile_j: u64,
    acc: String,
    acc_llt: String,
    elem_llt: String,
    seed: String,
    mul_op: &'static str,
    add_op: &'static str,
    a_ptr: String,
    b_ptr: String,
    out_ptr: String,
    a_llt: String,
    b_llt: String,
    out_llt: String,
    lane_ctr: String,
    contract_flag: &'static str,
}

// The `impl FnEmit` surface is split across these submodules (S41). Each is a
// child of this module, so it reaches `FnEmit`'s private fields directly and
// picks up the imports above through `use super::*`.
mod bulk;
mod conv;
mod core;
mod drive;
mod frame;
mod ops;
mod packed;
mod sme;
mod tile;
mod trio;
mod vec;
mod window;

// --- free helpers ---------------------------------------------------------

fn checkpoint_injection(
    ir: &CategoryIr,
    checkpoint: MorphismId,
    assigned: &SecondaryMap<MorphismId, ()>,
    topo: &[MorphismId],
) -> MorphismId {
    let mut seen = SecondaryMap::new();
    let mut stack = vec![
        ir.morphism(checkpoint)
            .expect("checkpoint morphism resolves")
            .source,
    ];
    let mut boundary = Vec::new();
    while let Some(object) = stack.pop() {
        if seen.insert(object, ()).is_some() {
            continue;
        }
        for &m in ir.in_edges(object) {
            if assigned.contains_key(m) {
                continue;
            }
            let morph = ir.morphism(m).expect("morphism resolves");
            if matches!(morph.op, Operation::Pair { .. } | Operation::Proj { .. }) {
                if ir
                    .in_edges(morph.source)
                    .iter()
                    .any(|producer| assigned.contains_key(*producer))
                {
                    boundary.push(m);
                } else {
                    stack.push(morph.source);
                }
            }
        }
    }
    boundary
        .into_iter()
        .min_by_key(|m| {
            topo.iter()
                .position(|candidate| candidate == m)
                .unwrap_or(usize::MAX)
        })
        .unwrap_or(checkpoint)
}

fn wait_global(name: &str, wait: &[WaitEntry]) -> String {
    let len = wait.len();
    let value = if wait.is_empty() {
        "zeroinitializer".to_string()
    } else {
        let entries = wait
            .iter()
            .map(|entry| {
                let threshold = entry.threshold.unwrap_or(u32::MAX);
                let packed = ((entry.task as u64) << 32) | u64::from(threshold);
                format!("i64 {packed}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("[{entries}]")
    };
    format!("@{name} = private unnamed_addr constant [{len} x i64] {value}\n")
}

/// The slot-`k` feeder of a product object (the free form of
/// `FnEmit::pair_source`, for the analysis passes — mirror of cuda
/// kernel.rs's free helper).
fn pair_source_ir(ir: &CategoryIr, agg: ObjectId, k: u32) -> Option<ObjectId> {
    for &m in ir.in_edges(agg) {
        let morph = ir.morphism(m).expect("morphism resolves");
        if let Operation::Pair { slot, .. } = morph.op
            && slot == k
        {
            return Some(morph.source);
        }
    }
    None
}

/// A literal int operand (the constant behind a pair slot), for the #13
/// constant-divisor credit (S20). `None` when not a literal int.
fn const_int_operand(ir: &CategoryIr, source: ObjectId, k: u32) -> Option<i128> {
    let obj = ir
        .object(pair_source_ir(ir, source, k)?)
        .expect("object resolves");
    if obj.kind != mapal_ir::ObjectKind::Constant {
        return None;
    }
    match &obj.value {
        Some(mapal_ir::Value::I32(n)) => Some(*n as i128),
        Some(mapal_ir::Value::I64(n)) => Some(*n as i128),
        Some(mapal_ir::Value::U8(n)) => Some(*n as i128),
        _ => None,
    }
}

fn is_float(ty: &Ty) -> bool {
    matches!(ty, Ty::Float { .. })
}

/// Does `ty` carry a top-level Array — the ty itself, or a direct product
/// component (nested products-in-products do NOT count: they stay by value,
/// suggestions #8's recorded limitation)?
fn has_top_array(ty: &Ty) -> bool {
    match ty {
        Ty::Array { .. } => true,
        Ty::Tuple(ts) => ts.iter().any(|t| matches!(t, Ty::Array { .. })),
        Ty::Struct { fields, .. } => fields.iter().any(|(_, t)| matches!(t, Ty::Array { .. })),
        _ => false,
    }
}

/// The direct component count of a Tuple/Struct (0 for anything else).
fn product_arity(ty: &Ty) -> u32 {
    match ty {
        Ty::Tuple(ts) => ts.len() as u32,
        Ty::Struct { fields, .. } => fields.len() as u32,
        _ => 0,
    }
}

fn sign_pred(signed: bool, s: &'static str, u: &'static str) -> &'static str {
    if signed { s } else { u }
}

/// `(elem_llt, size)` of an `Array` ty.
fn array_parts(ty: &Ty) -> (String, u64) {
    match ty {
        Ty::Array { elem, size } => (lower_ty(elem).expect("array elem lowers"), *size),
        _ => unreachable!("expected an array ty"),
    }
}

fn int_min(llt: &str) -> &'static str {
    match llt {
        "i32" => "-2147483648",
        "i64" => "-9223372036854775808",
        "i8" => "-128",
        _ => unreachable!("non-Core int width in Div/Mod"),
    }
}

/// `(mapal_rt_func, needs_zeroext, llvm_ty)` for a printable scalar.
fn print_dispatch(ty: &Ty) -> (&'static str, bool, &'static str) {
    match ty {
        Ty::Int { bits: 32, .. } => ("mapal_print_i32", false, "i32"),
        Ty::Int { bits: 64, .. } => ("mapal_print_i64", false, "i64"),
        Ty::Int { bits: 8, .. } => ("mapal_print_u8", true, "i8"),
        Ty::Bool => ("mapal_print_bool", true, "i1"),
        Ty::Float { bits: 32 } => ("mapal_print_f32", false, "float"),
        Ty::Float { bits: 64 } => ("mapal_print_f64", false, "double"),
        _ => unreachable!("non-printable Print operand"),
    }
}

/// The LLVM constant text for a scalar `Value` (floats as the 16-hex-digit form
/// LLVM uses for both `float` and `double`).
fn const_literal(v: &Value) -> String {
    match v {
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::U8(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::F32(x) => format!("0x{:016X}", (*x as f64).to_bits()),
        Value::F64(x) => format!("0x{:016X}", x.to_bits()),
        Value::Str(_) => unreachable!("Str is not a scalar operand"),
    }
}
