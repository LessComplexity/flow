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

impl<'a> FnEmit<'a> {
    pub fn new(
        ir: &'a CategoryIr,
        f: FuncId,
        fnames: &'a SecondaryMap<FuncId, String>,
        strings: &'a SecondaryMap<ObjectId, StrGlobal>,
        attrs: &'a FnAttrs,
        tiling: bool,
        packing: bool,
        contract: bool,
        kc_nest: bool,
        profile: &'static TargetProfile,
    ) -> Self {
        let mut gsites = SecondaryMap::new();
        let mut gated = SecondaryMap::new();
        for site in ir.guard_plan(f).into_iter().filter(GuardSite::gated) {
            for &m in site.on_true.own.iter().chain(site.on_false.own.iter()) {
                gated.insert(m, ());
            }
            gsites.insert(site.phi, site);
        }
        FnEmit {
            ir,
            f,
            fnames,
            strings,
            attrs,
            slots: SecondaryMap::new(),
            allocas: String::new(),
            body: String::new(),
            next: 0,
            byref: None,
            ptr_resident: SecondaryMap::new(),
            lup: ir.last_use_plan(f),
            bp: ir.bounds_proof(f),
            gsites,
            gated,
            elided_updates: SecondaryMap::new(),
            update_aliases: SecondaryMap::new(),
            frame: None,
            frame_geps: String::new(),
            guard_flavor: GuardFlavor::Host,
            split_range: false,
            watermark: false,
            host: None,
            runtime_write: false,
            perf_timing: false,
            tile_plan: tiling.then(|| ir.tile_plan(f)),
            elem: ir.elem_plan(f),
            elided_arrays: SecondaryMap::new(),
            packing,
            contract,
            kc_nest,
            profile,
            heap_ok: false,
            heap_used: false,
        }
    }

    pub(crate) fn set_perf_timing(&mut self, perf_timing: bool) {
        self.perf_timing = perf_timing;
    }

    pub(crate) fn set_task_body_site(&mut self, topo: u32) {
        self.guard_flavor = GuardFlavor::TaskBody(topo);
    }

    fn fresh(&mut self) -> u32 {
        let n = self.next;
        self.next += 1;
        n
    }

    fn tmp(&mut self) -> String {
        format!("%t{}", self.fresh())
    }

    /// A fresh block label (bare, no `%`).
    pub(crate) fn label(&mut self) -> String {
        format!("bb{}", self.fresh())
    }

    /// Append one indented body instruction line.
    pub(crate) fn line(&mut self, s: impl AsRef<str>) {
        self.body.push_str("  ");
        self.body.push_str(s.as_ref());
        self.body.push('\n');
    }

    /// Append a bare block label line (`name:`), no indent.
    pub(crate) fn label_line(&mut self, name: &str) {
        self.body.push_str(name);
        self.body.push_str(":\n");
    }

    fn slot(&mut self, o: ObjectId) -> Option<String> {
        if let Some(slot) = self.slots.get(o) {
            return Some(slot.clone());
        }
        let field = self.frame.as_ref()?.fields.get(o)?.clone();
        if let Some(slot) = self.slots.get(field.owner) {
            let slot = slot.clone();
            self.slots.insert(o, slot.clone());
            return Some(slot);
        }
        let slot = format!("%o{}", field.ordinal);
        self.frame_geps.push_str(&format!(
            "  {slot} = getelementptr %Frame, ptr %frame, i32 0, i32 {}\n",
            field.index
        ));
        self.slots.insert(field.owner, slot.clone());
        self.slots.insert(o, slot.clone());
        Some(slot)
    }

    fn obj_ty(&self, o: ObjectId) -> Ty {
        self.ir.object(o).expect("object resolves").ty.clone()
    }

    // --- operand materialization -----------------------------------------

    /// Load the whole value of object `o`: a literal for a (scalar) constant, a
    /// `load` from its slot otherwise. `None` if `o` is erased / a `Str`.
    fn load_whole(&mut self, o: ObjectId) -> Option<(String, String)> {
        let obj = self.ir.object(o).expect("object resolves");
        if obj.kind == ObjectKind::Constant {
            return match &obj.value {
                Some(Value::Str(_)) | None => None,
                Some(v) => Some((lower_ty(&obj.ty)?, const_literal(v))),
            };
        }
        let llt = lower_ty(&obj.ty)?;
        let slot = self.slot(o)?;
        if self.ptr_resident.contains_key(o) {
            // By-ref capture: load the array through the forwarded pointer.
            // The deep copy is observably the inline value (read-only
            // capture semantics), so escaping uses are unchanged.
            let p = self.tmp();
            self.line(format!("{p} = load ptr, ptr {slot}"));
            let v = self.tmp();
            self.line(format!("{v} = load {llt}, ptr {p}"));
            return Some((llt, v));
        }
        // The by-ref input product's WHOLE value (an escaping use — `Pair`
        // into an ordinary product, `Output`): its first-`bk` Array fields
        // hold `ptr`s, so assemble the by-value whole in a scratch of the
        // ordinary type — each by-ref array component deep-copied through its
        // forwarded pointer, every other component copied inline.
        let bk = match &self.byref {
            Some((input, bk, text)) if *input == o && *text != llt => Some(*bk),
            _ => None,
        };
        if let Some(bk) = bk {
            let buf = self.scratch(&llt);
            for k in 0..product_arity(&obj.ty) {
                if k < bk && matches!(obj.ty.component_ty(k), Some(Ty::Array { .. })) {
                    let cllt = lower_ty(obj.ty.component_ty(k)?)?;
                    let src = self.array_operand_ptr(o, Some(k))?;
                    let dst = self.field_ptr(&buf, &obj.ty, &llt, k)?;
                    self.emit_memcpy(&dst, &src, &cllt);
                } else if let Some((cllt, cval)) = self.load_component(o, k) {
                    self.field_store(&buf, &obj.ty, &llt, k, &cllt, &cval);
                }
            }
            let v = self.tmp();
            self.line(format!("{v} = load {llt}, ptr {buf}"));
            return Some((llt, v));
        }
        let v = self.tmp();
        self.line(format!("{v} = load {llt}, ptr {slot}"));
        Some((llt, v))
    }

    /// A pointer to component `k` of the aggregate object `agg` (GEP or the bare
    /// slot). `None` if component `k` is erased or `agg` has no slot.
    fn component_ptr(&mut self, agg: ObjectId, k: u32) -> Option<(String, String)> {
        let agg_ty = self.obj_ty(agg);
        match &agg_ty {
            Ty::Tuple(_) | Ty::Struct { .. } => {
                let comp_ty = agg_ty.component_ty(k)?;
                let agg_slot = self.slot(agg)?;
                // The by-ref body input GEPs against the by-ref struct text; a
                // first-k Array field holds the capture `ptr`, not the array.
                let (cllt, agg_llt) = match &self.byref {
                    Some((input, bk, text)) if *input == agg => (
                        if k < *bk && matches!(comp_ty, Ty::Array { .. }) {
                            "ptr".into()
                        } else {
                            lower_ty(comp_ty)?
                        },
                        text.clone(),
                    ),
                    _ => (
                        if self.pointer_only_array_component(agg, k) {
                            "ptr".into()
                        } else {
                            lower_ty(comp_ty)?
                        },
                        self.lower_slot_ty(agg, &agg_ty)?,
                    ),
                };
                if residual_arity(&agg_ty) == 1 {
                    Some((cllt, agg_slot)) // bare: the slot IS the component
                } else {
                    let eidx = erased_index(&agg_ty, k)?;
                    let ptr = self.tmp();
                    self.line(format!(
                        "{ptr} = getelementptr {agg_llt}, ptr {agg_slot}, i32 0, i32 {eidx}"
                    ));
                    Some((cllt, ptr))
                }
            }
            Ty::Array { elem, .. } => {
                let cllt = lower_ty(elem)?;
                let agg_slot = self.slot(agg)?;
                let agg_llt = lower_ty(&agg_ty)?;
                let ptr = self.tmp();
                self.line(format!(
                    "{ptr} = getelementptr {agg_llt}, ptr {agg_slot}, i64 0, i64 {k}"
                ));
                Some((cllt, ptr))
            }
            _ => None,
        }
    }

    /// Load component `k` of aggregate `agg`. `None` if erased.
    fn load_component(&mut self, agg: ObjectId, k: u32) -> Option<(String, String)> {
        let (cllt, ptr) = self.component_ptr(agg, k)?;
        let v = self.tmp();
        self.line(format!("{v} = load {cllt}, ptr {ptr}"));
        Some((cllt, v))
    }

    /// Whether this backend consumes `ElemSrc::Apply` (recomputing a
    /// classifiable `Map` producer at the read site). Off: see the refusal arm in
    /// [`FnEmit::emit_elem`] for the measurement. Legality lives in mapal-ir;
    /// this constant is the CPU backend's profitability answer, and it is `false`.
    const APPLY_INLINE: bool = false;

    /// Materialize `out[i]` from its element law (`mapal_ir::ElemSrc`) instead
    /// of loading it, at loop index `iv`. Returns `(llvm type, value operand)`,
    /// or `None` when the law cannot be realized here — in which case the caller
    /// keeps the load, which is always correct.
    ///
    /// This is the backend half of plan-s37-stage-structure: mapal-ir says what
    /// the element *is* (legality, machine-independent); the decision to inline
    /// it rather than read memory is the backend's, because the right answer
    /// differs per target. On this one it is unconditional for the bodyless
    /// laws — at most two loads and a `trunc`, never worse than the load it
    /// replaces.
    ///
    /// `Broadcast` is emitted inside the loop rather than hoisted by hand: the
    /// value is loop-invariant and LICM lifts it, and hand-hoisting would mean
    /// threading a pre-header through every caller for no measured gain.
    fn emit_elem(&mut self, law: &ElemSrc, elem_ty: &Ty, iv: &str) -> Option<(String, String)> {
        match law {
            // `iota`: the element is the index. ADR-0029 pins the element type
            // to `i32`, so the source-of-truth check is the type, not the tag —
            // a wider iota would need its own conversion and must not silently
            // truncate.
            ElemSrc::Index => {
                if *elem_ty != Ty::i32() {
                    return None;
                }
                let v = self.tmp();
                self.line(format!("{v} = trunc i64 {iv} to i32"));
                Some(("i32".to_string(), v))
            }
            ElemSrc::Broadcast { source, slot } => self.load_component(*source, *slot),
            ElemSrc::Load { source, slot } => {
                let arr_ty = match slot {
                    None => self.obj_ty(*source),
                    Some(k) => self.obj_ty(*source).component_ty(*k).cloned()?,
                };
                let (cllt, _) = array_parts(&arr_ty);
                let arr_llt = lower_ty(&arr_ty)?;
                let ptr = self.array_operand_ptr(*source, *slot)?;
                let ep = self.tmp();
                self.line(format!(
                    "{ep} = getelementptr {arr_llt}, ptr {ptr}, i64 0, i64 {iv}"
                ));
                let v = self.tmp();
                self.line(format!("{v} = load {cllt}, ptr {ep}"));
                Some((cllt, v))
            }
            // `zip` / `enumerate`: build the pair in registers instead of
            // reading it back out of a materialized array of structs.
            ElemSrc::Pair(a, b) => {
                let a_ty = elem_ty.component_ty(0).cloned()?;
                let b_ty = elem_ty.component_ty(1).cloned()?;
                let (a_llt, a_v) = self.emit_elem(a, &a_ty, iv)?;
                let (b_llt, b_v) = self.emit_elem(b, &b_ty, iv)?;
                let pair_llt = lower_ty(elem_ty)?;
                let p0 = self.tmp();
                self.line(format!(
                    "{p0} = insertvalue {pair_llt} poison, {a_llt} {a_v}, 0"
                ));
                let p1 = self.tmp();
                self.line(format!(
                    "{p1} = insertvalue {pair_llt} {p0}, {b_llt} {b_v}, 1"
                ));
                Some((pair_llt, p1))
            }
            // A classifiable `Map` producer: recompute its element by calling
            // the same body the producer calls, on the recursively-built inner
            // element. Nothing is spliced or merged — this is two calls in one
            // loop, which is why capture identity is not required.
            //
            // REFUSED ON THIS TARGET, on measurement (plan-s37-stage-structure
            // Table B — profitability is the backend's call, and this is the
            // backend making it). Recomputing a producer body is only a win
            // when arithmetic is cheaper than the load it replaces. On a CPU
            // with the array already materialized it is not: enabling this arm
            // put two extra calls inside saxpy's timed loop — `fn1` and `fn2`
            // regenerating `x[i]` and `y0[i]` from the index instead of reading
            // them — and cost 0.72x at one thread (0.4731 -> 0.6586 ms min).
            // Gather, the shape it was built for, came out at 1.17x min but
            // 0.97x median: inside noise.
            //
            // The FACT stays in mapal-ir because it is true and machine-
            // independent; only this consumer declines. A bandwidth-bound
            // target where registers are cheap should reach a different verdict
            // — that asymmetry is the whole reason the decision lives here and
            // not in the query. Re-enable behind an op-count test against an L2
            // round trip, with a measurement that moves a published cell.
            ElemSrc::Apply { array, .. } if !Self::APPLY_INLINE => {
                // Decline, but degrade to reading the producer's materialized
                // output rather than failing: a refusal nested inside a `Pair`
                // must not collapse the pair back to an array-of-structs read.
                let arr = *array;
                self.emit_elem(
                    &ElemSrc::Load {
                        source: arr,
                        slot: None,
                    },
                    elem_ty,
                    iv,
                )
            }
            ElemSrc::Apply {
                body,
                source,
                captures,
                inner,
                array: _,
            } => {
                let src_ty = self.obj_ty(*source);
                let inner_arr_ty = if *captures == 0 {
                    src_ty.clone()
                } else {
                    src_ty.component_ty(*captures).cloned()?
                };
                let inner_elem_ty = inner_arr_ty.component_ty(0).cloned()?;
                let (in_llt, in_v) = self.emit_elem(inner, &inner_elem_ty, iv)?;
                let out_llt = lower_ty(elem_ty)?;
                let callee = self.fnames[*body].clone();
                let arg = if *captures == 0 {
                    format!("{in_llt} {in_v}")
                } else {
                    let arg_ty = self.obj_ty(self.ir.func(*body)?.input);
                    let arg_llt = lower_body_input_ty(&arg_ty, *captures)?;
                    self.body_call_arg(*source, *captures, &arg_ty, &arg_llt, &[(&in_llt, &in_v)])
                };
                let v = self.tmp();
                self.line(format!("{v} = call {out_llt} @{callee}({arg})"));
                Some((out_llt, v))
            }
        }
    }

    /// Store `(llt, val)` into object `o`'s slot, if it has one.
    fn store_obj(&mut self, o: ObjectId, llt: &str, val: &str) {
        if let Some(slot) = self.slot(o) {
            self.line(format!("store {llt} {val}, ptr {slot}"));
        }
    }

    /// A fresh scratch alloca of `llt` in the entry block; returns its ptr name.
    fn scratch(&mut self, llt: &str) -> String {
        let name = format!("%s{}", self.fresh());
        self.allocas.push_str(&format!("  {name} = alloca {llt}\n"));
        name
    }

    /// Does a block of `bytes` belong in the arena rather than the stack
    /// (plan-s29 emission item 4)? Records the teardown debt when it does.
    fn heap_block(&mut self, bytes: u64) -> bool {
        let heap = self.heap_ok && bytes >= self.profile.heap_min_bytes;
        self.heap_used |= heap;
        heap
    }

    /// One named entry-block allocation of `llt`: today's `alloca` (with the
    /// explicit `align` when the site wants one), or an arena block once it
    /// crosses [`TargetProfile::heap_min_bytes`]. An `alloca` and a `mapal_rt_alloc` result are
    /// both just a `ptr`, so every `getelementptr {llt}, ptr …` consumer is
    /// unchanged — the swap is invisible below this line.
    fn entry_alloc(&mut self, name: &str, llt: &str, align: Option<u64>) {
        let bytes = llt_bytes(llt);
        let text = if self.heap_block(bytes) {
            let align = align.unwrap_or_else(|| llt_align(llt));
            format!("  {name} = call ptr @mapal_rt_alloc(i64 {bytes}, i64 {align})\n")
        } else {
            match align {
                Some(align) => format!("  {name} = alloca {llt}, align {align}\n"),
                None => format!("  {name} = alloca {llt}\n"),
            }
        };
        self.allocas.push_str(&text);
    }

    /// Release the arena, if this emitter filled any of it. Emitted once, at
    /// the last point that can read arena memory (plan-s29 composition rule 4).
    fn heap_teardown(&mut self) {
        if self.heap_used {
            self.line("call void @mapal_rt_free_all()");
        }
    }

    fn packed_type(profile: &TargetProfile, site: &TileSite) -> String {
        let tile_j = profile.tile_j(&site.elem);
        let tiles = site.c.div_ceil(tile_j);
        let elems = site
            .k
            .checked_mul(tiles)
            .and_then(|n| n.checked_mul(tile_j))
            .expect("packed tile size fits u64");
        format!(
            "[{elems} x {}]",
            lower_ty(&site.elem).expect("tile element lowers")
        )
    }

    fn packed_buffer(&mut self, m: MorphismId, site: &TileSite) -> PackedBuffer {
        let llt = Self::packed_type(self.profile, site);
        if let Some(field) = self
            .frame
            .as_ref()
            .and_then(|frame| frame.packed.get(m))
            .cloned()
        {
            let slot = format!("%pack_field{}", field.ordinal);
            let ptr = format!("%packed{}", field.ordinal);
            self.frame_geps.push_str(&format!(
                "  {slot} = getelementptr %Frame, ptr %frame, i32 0, i32 {}\n  {ptr} = load ptr, ptr {slot}\n",
                field.index
            ));
            PackedBuffer { ptr, llt }
        } else {
            let ptr = format!("%s{}", self.fresh());
            self.entry_alloc(&ptr, &llt, Some(64));
            PackedBuffer { ptr, llt }
        }
    }

    fn allocate_frame_packs(&mut self) {
        let Some(frame) = &self.frame else {
            return;
        };
        let Some(plan) = &self.tile_plan else {
            return;
        };
        let packs = frame
            .packed
            .iter()
            .map(|(m, field)| (m, field.clone(), plan.sites[m].clone()))
            .collect::<Vec<_>>();
        for (_, field, site) in packs {
            let llt = Self::packed_type(self.profile, &site);
            let ptr = format!("%pack{}", field.ordinal);
            let slot = format!("%pack_field{}", field.ordinal);
            self.entry_alloc(&ptr, &llt, Some(64));
            self.frame_geps.push_str(&format!(
                "  {slot} = getelementptr %Frame, ptr %frame, i32 0, i32 {}\n  store ptr {ptr}, ptr {slot}\n",
                field.index
            ));
        }
    }

    /// Copy one row-invariant b operand to packed[j-tile][k][lane], padding
    /// the final panel's dead lanes with zero.
    fn emit_pack_copy(&mut self, source: ObjectId, site: &TileSite, packed: &PackedBuffer) {
        debug_assert!(packing_site(site));
        let source_ty = self.obj_ty(source);
        let b_ty = source_ty
            .component_ty(site.b.slot)
            .cloned()
            .expect("tile b array");
        let b_llt = lower_ty(&b_ty).expect("tile b lowers");
        let elem_llt = lower_ty(&site.elem).expect("tile element lowers");
        let b_ptr = self
            .array_operand_ptr(source, Some(site.b.slot))
            .expect("tile b ptr");
        let tile_j = self.profile.tile_j(&site.elem);
        let tiles = site.c.div_ceil(tile_j);
        let panel_elems = site.k * tile_j;
        let jt_ctr = self.scratch("i64");
        let k_ctr = self.scratch("i64");
        let lane_ctr = self.scratch("i64");
        let (jt_head, jt_body, jt_done) = (self.label(), self.label(), self.label());
        let (k_head, k_body, k_done) = (self.label(), self.label(), self.label());
        let (lane_head, lane_body, lane_done) = (self.label(), self.label(), self.label());
        let (load, pad, store_done) = (self.label(), self.label(), self.label());

        self.line(format!("store i64 0, ptr {jt_ctr}"));
        self.line(format!("br label %{jt_head}"));
        self.label_line(&jt_head);
        let jt = self.tmp();
        self.line(format!("{jt} = load i64, ptr {jt_ctr}"));
        let all_tiles = self.tmp();
        self.line(format!("{all_tiles} = icmp uge i64 {jt}, {tiles}"));
        self.line(format!(
            "br i1 {all_tiles}, label %{jt_done}, label %{jt_body}"
        ));
        self.label_line(&jt_body);
        let j0 = self.tmp();
        self.line(format!("{j0} = mul i64 {jt}, {tile_j}"));
        let panel_base = self.tmp();
        self.line(format!("{panel_base} = mul i64 {jt}, {panel_elems}"));
        self.line(format!("store i64 0, ptr {k_ctr}"));
        self.line(format!("br label %{k_head}"));

        self.label_line(&k_head);
        let k = self.tmp();
        self.line(format!("{k} = load i64, ptr {k_ctr}"));
        let all_k = self.tmp();
        self.line(format!("{all_k} = icmp uge i64 {k}, {}", site.k));
        self.line(format!("br i1 {all_k}, label %{k_done}, label %{k_body}"));
        self.label_line(&k_body);
        let packed_k = self.tmp();
        self.line(format!("{packed_k} = mul i64 {k}, {tile_j}"));
        let packed_row = self.tmp();
        self.line(format!("{packed_row} = add i64 {panel_base}, {packed_k}"));
        self.line(format!("store i64 0, ptr {lane_ctr}"));
        self.line(format!("br label %{lane_head}"));

        self.label_line(&lane_head);
        let lane = self.tmp();
        self.line(format!("{lane} = load i64, ptr {lane_ctr}"));
        let all_lanes = self.tmp();
        self.line(format!("{all_lanes} = icmp uge i64 {lane}, {tile_j}"));
        self.line(format!(
            "br i1 {all_lanes}, label %{lane_done}, label %{lane_body}"
        ));
        self.label_line(&lane_body);
        let j = self.tmp();
        self.line(format!("{j} = add i64 {j0}, {lane}"));
        let packed_index = self.tmp();
        self.line(format!("{packed_index} = add i64 {packed_row}, {lane}"));
        let packed_ptr = self.tmp();
        self.line(format!(
            "{packed_ptr} = getelementptr {}, ptr {}, i64 0, i64 {packed_index}",
            packed.llt, packed.ptr
        ));
        let live = self.tmp();
        self.line(format!("{live} = icmp ult i64 {j}, {}", site.c));
        self.line(format!("br i1 {live}, label %{load}, label %{pad}"));

        self.label_line(&load);
        let b_index = self
            .emit_tile_index(
                (site.b.base != 0).then(|| site.b.base.to_string()),
                &[(site.b.ck, k.as_str()), (1, j.as_str())],
            )
            .expect("tile b has lane term");
        let b_elem_ptr = self.tmp();
        self.line(format!(
            "{b_elem_ptr} = getelementptr {b_llt}, ptr {b_ptr}, i64 0, i64 {b_index}"
        ));
        let value = self.tmp();
        self.line(format!("{value} = load {elem_llt}, ptr {b_elem_ptr}"));
        self.line(format!("store {elem_llt} {value}, ptr {packed_ptr}"));
        self.line(format!("br label %{store_done}"));

        self.label_line(&pad);
        self.line(format!(
            "store {elem_llt} zeroinitializer, ptr {packed_ptr}"
        ));
        self.line(format!("br label %{store_done}"));
        self.label_line(&store_done);
        let lane_next = self.tmp();
        self.line(format!("{lane_next} = add i64 {lane}, 1"));
        self.line(format!("store i64 {lane_next}, ptr {lane_ctr}"));
        self.line(format!("br label %{lane_head}"));

        self.label_line(&lane_done);
        let k_next = self.tmp();
        self.line(format!("{k_next} = add i64 {k}, 1"));
        self.line(format!("store i64 {k_next}, ptr {k_ctr}"));
        self.line(format!("br label %{k_head}"));
        self.label_line(&k_done);
        let jt_next = self.tmp();
        self.line(format!("{jt_next} = add i64 {jt}, 1"));
        self.line(format!("store i64 {jt_next}, ptr {jt_ctr}"));
        self.line(format!("br label %{jt_head}"));
        self.label_line(&jt_done);
    }

    /// The local slot type for a Pair-built staging product. Array components
    /// consumed only as addresses by collection/index/call ops are `ptr`
    /// fields (S20 #6/#8); value-observable components retain their ABI type.
    fn lower_slot_ty(&self, o: ObjectId, ty: &Ty) -> Option<String> {
        let components: Vec<&Ty> = match ty {
            Ty::Tuple(ts) => ts.iter().collect(),
            Ty::Struct { fields, .. } => fields.iter().map(|(_, ty)| ty).collect(),
            _ => return lower_ty(ty),
        };
        let kept: Vec<String> = components
            .iter()
            .enumerate()
            .filter_map(|(k, ty)| {
                if self.pointer_only_array_component(o, k as u32) {
                    Some("ptr".into())
                } else {
                    lower_ty(ty)
                }
            })
            .collect();
        match kept.len() {
            0 => None,
            1 => Some(kept.into_iter().next().unwrap()),
            _ => Some(format!("{{ {} }}", kept.join(", "))),
        }
    }

    /// A Pair array field is representation-only when every use reads that
    /// component as an address. Such fields stage a pointer, never array bytes.
    fn pointer_only_array_component(&self, agg: ObjectId, k: u32) -> bool {
        if !matches!(self.obj_ty(agg).component_ty(k), Some(Ty::Array { .. }))
            || self.pair_source(agg, k).is_none()
        {
            return false;
        }
        let uses = self.ir.out_edges(agg);
        !uses.is_empty()
            && uses.iter().all(
                |&m| match self.ir.morphism(m).expect("morphism resolves").op {
                    Operation::Index | Operation::Update => k == 0,
                    Operation::Zip => k <= 1,
                    Operation::Map { captures, .. } => k <= captures,
                    Operation::Fold { captures, .. } => k < captures || k == captures + 1,
                    Operation::Call(_) => true,
                    _ => false,
                },
            )
    }

    /// Address field `k` of raw aggregate storage. Erasure remapping is based
    /// on the source type; replacing an Array with `ptr` keeps it materialized.
    fn field_ptr(&mut self, base: &str, agg_ty: &Ty, agg_llt: &str, k: u32) -> Option<String> {
        if residual_arity(agg_ty) == 1 {
            return Some(base.to_string());
        }
        let eidx = erased_index(agg_ty, k)?;
        let p = self.tmp();
        self.line(format!(
            "{p} = getelementptr {agg_llt}, ptr {base}, i32 0, i32 {eidx}"
        ));
        Some(p)
    }

    /// Copy an aggregate value between allocas without creating aggregate SSA
    /// (ADR-0021's Update pattern). An exact identity is already in place.
    fn emit_memcpy(&mut self, dst: &str, src: &str, llt: &str) {
        if dst == src {
            return;
        }
        self.line(format!(
            "call void @llvm.memcpy.p0.p0.i64(ptr {dst}, ptr {src}, i64 ptrtoint (ptr getelementptr ({llt}, ptr null, i64 1) to i64), i1 false)"
        ));
    }

    /// Store `(vllt, val)` into field `k` of a raw aggregate pointer of ty
    /// `agg_ty`, whose lowered text is `agg_llt` (the by-ref input struct for a
    /// capturing body call, `lower_ty` otherwise — the GEP offsets differ).
    /// Used by the collection loops, which build products in scratch.
    fn field_store(
        &mut self,
        base: &str,
        agg_ty: &Ty,
        agg_llt: &str,
        k: u32,
        vllt: &str,
        val: &str,
    ) {
        let ptr = self
            .field_ptr(base, agg_ty, agg_llt, k)
            .expect("kept field");
        self.line(format!("store {vllt} {val}, ptr {ptr}"));
    }

    /// The base address of an op's array operand — component `k` of the `source`
    /// product (`Some(k)`), or the bare `source` object itself (`None`, the
    /// no-capture map source). When the array reaches the op from a ptr-resident
    /// by-ref capture (the `Pair` feeder, or `source` itself), the forwarded
    /// `load ptr` is the address — the op reads the caller's array directly
    /// instead of the inline deep copy. When `source` IS the by-ref fn input
    /// product, its Array field holds the forwarded `ptr` — load it. Anything
    /// else is the ordinary slot/component address, as before.
    fn array_operand_ptr(&mut self, source: ObjectId, k: Option<u32>) -> Option<String> {
        let feeder = match k {
            None => Some(source),
            Some(k) => self.pair_source(source, k),
        };
        if let Some(f) = feeder
            && self.ptr_resident.contains_key(f)
        {
            let slot = self.slot(f)?;
            let p = self.tmp();
            self.line(format!("{p} = load ptr, ptr {slot}"));
            return Some(p);
        }
        if let Some(f) = feeder
            && matches!(self.obj_ty(f), Ty::Array { .. })
            && let Some(slot) = self.slot(f)
        {
            return Some(slot);
        }
        match k {
            None => self.slot(source),
            Some(k) => {
                let byref_field = matches!(&self.byref, Some((input, bk, _))
                    if *input == source
                        && k < *bk
                        && matches!(self.obj_ty(source).component_ty(k), Some(Ty::Array { .. })));
                if byref_field {
                    let (_, fp) = self.component_ptr(source, k)?;
                    let p = self.tmp();
                    self.line(format!("{p} = load ptr, ptr {fp}"));
                    return Some(p);
                }
                Some(self.component_ptr(source, k)?.1)
            }
        }
    }

    // --- traps ------------------------------------------------------------

    /// Branch to a trap block when `cond` is true; continue otherwise
    /// (`kind`: 0 = div_zero, 1 = index_oob — DESIGN §1).
    fn trap_if(&mut self, cond: &str, kind: u32) {
        let trap = self.label();
        let cont = self.label();
        self.line(format!("br i1 {cond}, label %{trap}, label %{cont}"));
        self.label_line(&trap);
        self.line(format!("call void @mapal_trap(i32 {kind})"));
        self.line("unreachable");
        self.label_line(&cont);
    }

    fn task_site(&self, m: MorphismId) -> u32 {
        match self.guard_flavor {
            GuardFlavor::Host => unreachable!("host guard has no task site"),
            GuardFlavor::TaskBody(topo) => topo,
            GuardFlavor::Task => self
                .ir
                .topo_order(self.f)
                .iter()
                .position(|&candidate| candidate == m)
                .expect("task morphism is in entry topo") as u32,
        }
    }

    fn record_trap(&mut self, m: MorphismId, kind: u32) {
        let topo = self.task_site(m);
        self.line(format!("call void @mapal_par_trap(i64 {topo}, i32 {kind})"));
        self.runtime_write = true;
    }

    fn local_trap_site(&self, m: MorphismId) -> bool {
        let morph = self.ir.morphism(m).expect("morphism resolves");
        match morph.op {
            Operation::Div | Operation::Mod => {
                matches!(self.obj_ty(morph.target), Ty::Int { .. })
            }
            Operation::Index => !self.bp.proven(m),
            Operation::Update => true,
            _ => false,
        }
    }

    fn emit_watermark(&mut self, m: MorphismId) {
        let topo = self.task_site(m);
        self.line(format!("call void @mapal_par_watermark(i64 {topo})"));
        self.runtime_write = true;
    }

    fn bulk_bounds(&self, n: u64) -> (String, String) {
        if self.split_range {
            ("%lo".into(), "%hi".into())
        } else {
            ("0".into(), n.to_string())
        }
    }

    // --- the walk ---------------------------------------------------------

    /// Configure by-ref inputs and Update slot aliases. Returns the lowered
    /// incoming argument type.
    fn prepare_storage(&mut self) -> Option<String> {
        let fd = self.ir.func(self.f).expect("func resolves");
        let in_ty = self.obj_ty(fd.input);
        let (bk, btext) = match fd.kind {
            FuncKind::MapBody | FuncKind::FoldBody => {
                let k = self.body_captures();
                (k, lower_body_input_ty(&in_ty, k))
            }
            FuncKind::Named => (u32::MAX, lower_named_input_ty(&in_ty)),
        };
        if let Some(text) = btext {
            self.byref = Some((fd.input, bk, text));
            if bk == u32::MAX && matches!(&in_ty, Ty::Array { .. }) {
                self.ptr_resident.insert(fd.input, ());
            }
            let owned_ids: Vec<ObjectId> = self
                .ir
                .objects()
                .filter(|(id, _)| self.ir.try_owner(*id) == Some(self.f))
                .map(|(id, _)| id)
                .collect();
            for id in owned_ids {
                for &m in self.ir.in_edges(id) {
                    let morph = self.ir.morphism(m).expect("morphism resolves");
                    if let Operation::Proj { index } = morph.op
                        && morph.source == fd.input
                        && index < bk
                        && matches!(in_ty.component_ty(index), Some(Ty::Array { .. }))
                    {
                        self.ptr_resident.insert(id, ());
                    }
                }
            }
        }
        let in_llt = match &self.byref {
            Some((_, _, text)) => Some(text.clone()),
            None => lower_ty(&in_ty),
        };

        for &m in &fd.morphisms {
            if let Some(source) = self.update_in_place_source(m) {
                let target = self.ir.morphism(m).expect("morphism resolves").target;
                self.elided_updates.insert(target, ());
                self.update_aliases.insert(target, source);
            }
        }
        self.mark_elided_arrays(&fd.morphisms.clone());
        in_llt
    }

    /// Step 3b: find arrays no one will ever load, because every consumer
    /// rebuilds the element from its law (`elem_plan`).
    ///
    /// Deliberately narrow. An array qualifies only when **every** out-edge is a
    /// `Map`/`Fold` reading it directly as the mapped/folded array — the
    /// capture-free shape. A captured consumer reaches its array through a
    /// `Pair` product, and following that chain is more analysis than the win
    /// justifies today; those arrays keep their buffer. Conservative in the safe
    /// direction: a missed elision costs a store pass, a wrong one dereferences
    /// a field that does not exist.
    fn mark_elided_arrays(&mut self, morphisms: &[MorphismId]) {
        for &m in morphisms {
            let Some(morph) = self.ir.morphism(m) else {
                continue;
            };
            if !matches!(
                morph.op,
                Operation::Iota | Operation::Fill | Operation::Zip | Operation::Enumerate
            ) {
                continue;
            }
            let arr = morph.target;
            // The law must be one a consumer can actually build. `Apply` is
            // excluded because THIS backend declines it (see `APPLY_INLINE`)
            // and a declined `Apply` degrades to loading exactly this array.
            match self.elem.src(arr) {
                Some(ElemSrc::Index | ElemSrc::Broadcast { .. } | ElemSrc::Pair(..)) => {}
                _ => continue,
            }
            if self.ir.object(arr).map(|o| o.kind) != Some(ObjectKind::Temporary) {
                continue;
            }
            let consumers = self.ir.out_edges(arr);
            if consumers.is_empty() {
                continue;
            }
            let all_inline = consumers.iter().all(|&c| {
                let Some(cm) = self.ir.morphism(c) else {
                    return false;
                };
                match cm.op {
                    Operation::Map { captures: 0, .. } => cm.source == arr,
                    Operation::Fold { .. } | Operation::Map { .. } => false,
                    _ => false,
                }
            });
            if all_inline {
                self.elided_arrays.insert(arr, ());
            }
        }
    }

    fn owned_objects(&self) -> Vec<(ObjectId, ObjectKind, Ty)> {
        self.ir
            .objects()
            .filter(|(id, _)| self.ir.try_owner(*id) == Some(self.f))
            .map(|(id, obj)| (id, obj.kind, obj.ty.clone()))
            .collect()
    }

    fn slot_type(&self, id: ObjectId, ty: &Ty, in_llt: &Option<String>) -> Option<String> {
        if self.ptr_resident.contains_key(id) {
            Some("ptr".into())
        } else if Some(id) == self.byref.as_ref().map(|(input, _, _)| *input) {
            in_llt.clone()
        } else {
            self.lower_slot_ty(id, ty)
        }
    }

    fn allocate_local_slots(&mut self, in_llt: &Option<String>) {
        let mut ord = 0u32;
        for (id, kind, ty) in self.owned_objects() {
            if kind == ObjectKind::Constant {
                continue;
            }
            if self.elided_updates.contains_key(id) || self.elided_arrays.contains_key(id) {
                ord += 1;
                continue;
            }
            if let Some(llt) = self.slot_type(id, &ty, in_llt) {
                let name = format!("%o{ord}");
                self.slots.insert(id, name.clone());
                self.entry_alloc(&name, &llt, None);
            }
            ord += 1;
        }
    }

    fn build_frame_layout(&self, in_llt: &Option<String>, path_plan: &PathPlan) -> FrameLayout {
        let mut fields = SecondaryMap::new();
        let mut order = Vec::new();
        let mut ord = 0u32;
        for (id, kind, ty) in self.owned_objects() {
            if kind == ObjectKind::Constant {
                continue;
            }
            // step 3b: an array nobody loads needs no storage. Dropping the
            // FIELD is the part DCE cannot do for us — `%Frame` is one object
            // shared across every task, so an unread member still costs its
            // bytes in the allocation.
            if self.elided_updates.contains_key(id) || self.elided_arrays.contains_key(id) {
                ord += 1;
                continue;
            }
            if let Some(llt) = self.slot_type(id, &ty, in_llt) {
                let index = order.len() as u32;
                fields.insert(
                    id,
                    FrameField {
                        owner: id,
                        index,
                        ordinal: ord,
                        llt,
                    },
                );
                order.push(id);
            }
            ord += 1;
        }
        for (target, _) in self.update_aliases.iter() {
            let mut source = self.update_aliases[target];
            while let Some(&next) = self.update_aliases.get(source) {
                source = next;
            }
            let field = fields
                .get(source)
                .expect("elided Update source has a frame field")
                .clone();
            fields.insert(target, field);
        }
        let mut packed = SecondaryMap::new();
        if self.packing
            && let Some(tile_plan) = &self.tile_plan
        {
            for task in &path_plan.tasks {
                if let TaskKind::Split { site: m, .. } = &task.kind
                    && let Some(site) = tile_plan.sites.get(*m)
                    && packing_site(site)
                {
                    packed.insert(
                        *m,
                        PackedField {
                            index: (order.len() + packed.len()) as u32,
                            ordinal: packed.len() as u32,
                        },
                    );
                }
            }
        }
        FrameLayout {
            fields,
            order,
            packed,
        }
    }

    fn materialize_frame_slots(&mut self) {
        let order = self.frame.as_ref().expect("frame layout").order.clone();
        for o in order {
            self.slot(o).expect("frame field resolves");
        }
    }

    /// Emit the function body: prologue store, the topo walk, epilogue return.
    pub fn emit(mut self) -> String {
        let fd = self.ir.func(self.f).expect("func resolves");
        let ret_ty = self.obj_ty(fd.output);
        let fname = self.fnames[self.f].clone();

        // Only the entry function's frame is once-per-program (plan-s29
        // composition rule 4); the parallel entry goes through `emit_parallel`,
        // so reaching here as the entry IS the sequential flavor.
        self.heap_ok = self.f == self.ir.entry();

        let in_llt = self.prepare_storage();
        self.allocate_local_slots(&in_llt);

        // Prologue: store the incoming parameter into its slot.
        if let Some(t) = &in_llt
            && let Some(ps) = self.slot(fd.input)
        {
            self.line(format!("store {t} %arg, ptr {ps}"));
        }

        // The topo walk (DESIGN §2/§3).
        self.walk();

        // Epilogue: return the output slot (or void).
        let sig_ret = match lower_ty(&ret_ty) {
            Some(t) => {
                let os = self.slot(fd.output).expect("non-void return has a slot");
                let v = self.tmp();
                self.line(format!("{v} = load {t}, ptr {os}"));
                if self.perf_timing {
                    self.line("call void @mapal_perf_end()");
                }
                self.heap_teardown();
                self.line(format!("ret {t} {v}"));
                t
            }
            None => {
                if self.perf_timing {
                    self.line("call void @mapal_perf_end()");
                }
                self.heap_teardown();
                self.line("ret void");
                "void".to_string()
            }
        };

        // Truthful fn attributes (suggestions #7; `FnAttrs`): clean fns are
        // `readonly nounwind` (+ `willreturn` when the closure can't loop or
        // recurse); a clean fn's bare-`ptr` by-ref array parameter additionally
        // carries `noalias nocapture readonly` (the callee never writes through
        // it, never lets it escape, and — the single pointer argument — it
        // aliases nothing else the fn accesses). Unclean fns stay bare.
        let clean = self.attrs.clean(self.f) && !self.runtime_write && !self.perf_timing;
        let param = match &in_llt {
            Some(t) if clean && t == "ptr" && self.byref.is_some() => {
                format!("{t} noalias nocapture readonly %arg")
            }
            Some(t) => format!("{t} %arg"),
            None => String::new(),
        };
        let fn_attrs = if clean {
            if self.attrs.loopy(self.f) {
                " readonly nounwind"
            } else {
                " readonly nounwind willreturn"
            }
        } else {
            ""
        };
        let perf_begin = if self.perf_timing {
            "  call void @mapal_perf_begin()\n"
        } else {
            ""
        };
        format!(
            "define internal {sig_ret} @{fname}({param}){fn_attrs} {{\nentry:\n{}{}{}}}\n",
            perf_begin, self.allocas, self.body
        )
    }

    pub(crate) fn emit_parallel(
        ir: &'a CategoryIr,
        f: FuncId,
        fnames: &'a SecondaryMap<FuncId, String>,
        strings: &'a SecondaryMap<ObjectId, StrGlobal>,
        attrs: &'a FnAttrs,
        plan: &PathPlan,
        perf_timing: bool,
        tiling: bool,
        packing: bool,
        contract: bool,
        kc_nest: bool,
        profile: &'static TargetProfile,
    ) -> String {
        let mut host = FnEmit::new(
            ir, f, fnames, strings, attrs, tiling, packing, contract, kc_nest, profile,
        );
        host.perf_timing = perf_timing;
        // The host runs once per program, so its blocks may go to the arena.
        host.heap_ok = true;
        let fd = ir.func(f).expect("func resolves");
        let in_llt = host.prepare_storage();
        let frame = host.build_frame_layout(&in_llt, plan);
        host.frame = Some(frame.clone());
        // The parallel flavor packs every array into ONE `%Frame`, so THAT is
        // the block that blows the stack — heap-lower it as a unit. Tasks take
        // `ptr %frame` either way, and every field access is a `getelementptr
        // %Frame, ptr %frame, …`, so nothing below this line changes.
        let frame_bytes = llt_bytes(&frame.struct_llt());
        if host.heap_block(frame_bytes) {
            let align = llt_align(&frame.struct_llt());
            host.allocas.push_str(&format!(
                "  %frame = call ptr @mapal_rt_alloc(i64 {frame_bytes}, i64 {align})\n"
            ));
        } else {
            host.allocas.push_str("  %frame = alloca %Frame\n");
        }
        host.materialize_frame_slots();
        host.allocate_frame_packs();

        let topo = ir.topo_order(f);
        let mut assigned = SecondaryMap::new();
        let mut pinned = SecondaryMap::new();
        for (task_id, task) in plan.tasks.iter().enumerate() {
            let members: &[MorphismId] = match &task.kind {
                TaskKind::Split { site, .. } => std::slice::from_ref(site),
                TaskKind::Seq { morphisms } => morphisms,
            };
            for &m in members {
                assigned.insert(m, ());
            }
            if task.pinned {
                let first = *members.first().expect("task has a member");
                let first_topo = topo
                    .iter()
                    .position(|&candidate| candidate == first)
                    .expect("task member is in topo") as u32;
                pinned.insert(
                    first,
                    PinnedEmit {
                        task: task_id,
                        topo: first_topo,
                        len: task.deps.len(),
                    },
                );
            }
        }
        let mut checkpoints = SecondaryMap::new();
        for (ordinal, checkpoint) in plan
            .checkpoints
            .iter()
            .filter(|checkpoint| checkpoint.topo != u32::MAX)
            .enumerate()
        {
            let site = topo[checkpoint.topo as usize];
            let injection = checkpoint_injection(ir, site, &assigned, &topo);
            checkpoints.insert(
                injection,
                CheckpointEmit {
                    ordinal,
                    topo: checkpoint.topo,
                    len: checkpoint.wait.len(),
                },
            );
        }
        let mut pre_loop: SecondaryMap<MorphismId, Vec<CheckpointEmit>> = SecondaryMap::new();
        for scc in ir.loop_structure(f) {
            let mut objects: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
            for &o in &scc.objects {
                objects.insert(o, ());
            }
            let mut members: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
            for &m in &topo {
                let morph = ir.morphism(m).expect("morphism resolves");
                if objects.contains_key(morph.source) || objects.contains_key(morph.target) {
                    members.insert(m, ());
                }
            }
            for &merge in &scc.merges {
                if let Some(plan) = ir.loop_plan(f, merge) {
                    for &m in plan
                        .decide_order
                        .iter()
                        .chain(&plan.advance_order)
                        .chain(&plan.exits)
                    {
                        members.insert(m, ());
                    }
                }
            }
            // A checkpoint site inside this loop ⟹ the loop is effectful (a
            // print is token-bearing), hence host-emitted; hoist its wait+check
            // to the loop's first LoopEnter.
            let first_enter = topo.iter().copied().find(|&m| {
                members.contains_key(m)
                    && matches!(
                        ir.morphism(m).expect("morphism resolves").op,
                        Operation::LoopEnter
                    )
            });
            let Some(first_enter) = first_enter else {
                continue;
            };
            for (ordinal, checkpoint) in plan
                .checkpoints
                .iter()
                .filter(|checkpoint| checkpoint.topo != u32::MAX)
                .enumerate()
            {
                if members.contains_key(topo[checkpoint.topo as usize]) {
                    let emit = CheckpointEmit {
                        ordinal,
                        topo: checkpoint.topo,
                        len: checkpoint.wait.len(),
                    };
                    if let Some(list) = pre_loop.get_mut(first_enter) {
                        list.push(emit);
                    } else {
                        pre_loop.insert(first_enter, vec![emit]);
                    }
                }
            }
        }
        host.host = Some(HostEmit {
            checkpoints,
            pinned,
            pre_loop,
        });

        if let Some(t) = &in_llt
            && let Some(slot) = host.slot(fd.input)
        {
            host.line(format!("store {t} %arg, ptr {slot}"));
        }
        host.line(format!(
            "%h = call ptr @mapal_par_begin(i32 {})",
            plan.tasks.len()
        ));
        for (task_id, task) in plan.tasks.iter().enumerate() {
            let (kind, n) = match &task.kind {
                // step 3b fallout: `path_plan` cut this task from the GRAPH,
                // where the producer is a real morphism making a real array.
                // The backend then decided nothing reads that array and emitted
                // no body. Registering it `Split` anyway would have the pool
                // slice a million-element range across every core to run a
                // function with zero instructions — free at MAPAL_PAR=1, one
                // dispatch per core plus a join at width. Register it as a
                // single unsplittable unit instead; the dep edges are untouched,
                // so dependents still wait on it and nothing renumbers.
                TaskKind::Split { site, .. }
                    if host
                        .ir
                        .morphism(*site)
                        .is_some_and(|m| host.elided_arrays.contains_key(m.target)) =>
                {
                    (0, 1)
                }
                TaskKind::Split { site, n }
                    if host
                        .tile_plan
                        .as_ref()
                        .and_then(|plan| plan.sites.get(*site))
                        .is_some_and(packing_site)
                        && host.packing =>
                {
                    (0, *n)
                }
                TaskKind::Split { n, .. } => (1, *n),
                TaskKind::Seq { morphisms } => (0, morphisms.len().max(1) as u64),
            };
            // plan-s32 step 2: the region's slice sizing, decided here because
            // both inputs are compile-time facts — the recorded reuse structure
            // and the profile's tile factors. The lane count is NOT an input;
            // the runtime multiplies `oversub` by however many lanes it has.
            let (min_slice, oversub) = match &task.kind {
                TaskKind::Split { site, .. } => host
                    .tile_plan
                    .as_ref()
                    .and_then(|plan| plan.sites.get(*site))
                    .map_or((0, 0), |site| host.slice_sizing(site)),
                TaskKind::Seq { .. } => (0, 0),
            };
            host.line(format!(
                "call void @mapal_par_task(ptr %h, i32 {task_id}, i32 {kind}, ptr @task{task_id}, i64 {n}, i32 {}, i64 {min_slice}, i32 {oversub}, i32 0)",
                task.rank
            ));
        }
        for (task_id, task) in plan.tasks.iter().enumerate() {
            if task.pinned {
                host.line(format!("call void @mapal_par_pin(ptr %h, i32 {task_id})"));
            }
        }
        for (after, task) in plan.tasks.iter().enumerate() {
            for &before in &task.deps {
                host.line(format!(
                    "call void @mapal_par_dep(ptr %h, i32 {before}, i32 {after})"
                ));
            }
        }
        host.line("call void @mapal_par_launch(ptr %h, ptr %frame)");
        host.walk_filtered(&assigned, false);
        host.line("call void @mapal_par_finish(ptr %h)");

        let ret_ty = host.obj_ty(fd.output);
        let sig_ret = match lower_ty(&ret_ty) {
            Some(t) => {
                let slot = host.slot(fd.output).expect("non-void return has a slot");
                let value = host.tmp();
                host.line(format!("{value} = load {t}, ptr {slot}"));
                if host.perf_timing {
                    host.line("call void @mapal_perf_end()");
                }
                host.heap_teardown();
                host.line(format!("ret {t} {value}"));
                t
            }
            None => {
                if host.perf_timing {
                    host.line("call void @mapal_perf_end()");
                }
                host.heap_teardown();
                host.line("ret void");
                "void".into()
            }
        };
        let param = in_llt
            .as_ref()
            .map(|t| format!("{t} %arg"))
            .unwrap_or_default();

        let mut out = frame.definition();
        out.push('\n');
        for (ordinal, checkpoint) in plan
            .checkpoints
            .iter()
            .filter(|checkpoint| checkpoint.topo != u32::MAX)
            .enumerate()
        {
            out.push_str(&wait_global(
                &format!("ckpt{ordinal}_entries"),
                &checkpoint.wait,
            ));
        }
        for (task_id, task) in plan.tasks.iter().enumerate() {
            if task.pinned {
                let wait = task
                    .deps
                    .iter()
                    .map(|&task| WaitEntry {
                        task,
                        threshold: None,
                    })
                    .collect::<Vec<_>>();
                out.push_str(&wait_global(&format!("pin{task_id}_entries"), &wait));
            }
        }
        if plan
            .checkpoints
            .iter()
            .any(|checkpoint| checkpoint.topo != u32::MAX)
            || plan.tasks.iter().any(|task| task.pinned)
        {
            out.push('\n');
        }

        for (task_id, task) in plan.tasks.iter().enumerate() {
            out.push_str(&Self::emit_task(
                ir, f, fnames, strings, attrs, &frame, task_id, task, tiling, packing, contract,
                kc_nest, profile,
            ));
            out.push('\n');
        }
        let perf_begin = if host.perf_timing {
            "  call void @mapal_perf_begin()\n"
        } else {
            ""
        };
        out.push_str(&format!(
            "define internal {sig_ret} @{}({param}) {{\nentry:\n{}{}{}{}{}\n",
            fnames[f], perf_begin, host.allocas, host.frame_geps, host.body, "}"
        ));
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_task(
        ir: &'a CategoryIr,
        f: FuncId,
        fnames: &'a SecondaryMap<FuncId, String>,
        strings: &'a SecondaryMap<ObjectId, StrGlobal>,
        attrs: &'a FnAttrs,
        frame: &FrameLayout,
        task_id: usize,
        task: &mapal_ir::Task,
        tiling: bool,
        packing: bool,
        contract: bool,
        kc_nest: bool,
        profile: &'static TargetProfile,
    ) -> String {
        if let TaskKind::Split { site: m, n } = &task.kind
            && let Some(site) = tiling
                .then(|| ir.tile_plan(f))
                .and_then(|plan| plan.sites.get(*m).cloned())
            && packing
            && packing_site(&site)
        {
            let mut slice = FnEmit::new(
                ir, f, fnames, strings, attrs, tiling, packing, contract, kc_nest, profile,
            );
            slice.guard_flavor = GuardFlavor::Task;
            slice.split_range = true;
            slice.prepare_storage();
            slice.frame = Some(frame.clone());
            let mut members = SecondaryMap::new();
            members.insert(*m, ());
            slice.walk_filtered(&members, true);
            slice.line("ret void");
            let slice_fn = format!(
                "define internal void @task{task_id}_slice(i64 %lo, i64 %hi, ptr %frame) {{\nentry:\n{}{}{}}}\n",
                slice.allocas, slice.frame_geps, slice.body
            );

            let mut emit = FnEmit::new(
                ir, f, fnames, strings, attrs, tiling, packing, contract, kc_nest, profile,
            );
            emit.guard_flavor = GuardFlavor::Task;
            emit.prepare_storage();
            emit.frame = Some(frame.clone());
            let source = ir.morphism(*m).expect("tile map resolves").source;
            let packed = emit.packed_buffer(*m, &site);
            emit.emit_pack_copy(source, &site, &packed);
            let handle = emit.tmp();
            emit.line(format!("{handle} = call ptr @mapal_par_begin(i32 1)"));
            // The packed flavor slices HERE, in the nested dispatch — the outer
            // task is the pack wrapper and is never split — so this is the call
            // the region's sizing belongs on.
            let (min_slice, oversub) = emit.slice_sizing(&site);
            emit.line(format!(
                "call void @mapal_par_task(ptr {handle}, i32 0, i32 1, ptr @task{task_id}_slice, i64 {n}, i32 {}, i64 {min_slice}, i32 {oversub}, i32 0)",
                task.rank
            ));
            emit.line(format!(
                "call void @mapal_par_launch(ptr {handle}, ptr %frame)"
            ));
            emit.line(format!("call void @mapal_par_finish(ptr {handle})"));
            emit.line("ret void");
            let wrapper_fn = format!(
                "define internal void @task{task_id}(i64 %lo, i64 %hi, ptr %frame) {{\nentry:\n{}{}{}}}\n",
                emit.allocas, emit.frame_geps, emit.body
            );
            return format!("{slice_fn}\n{wrapper_fn}");
        }

        let mut emit = FnEmit::new(
            ir, f, fnames, strings, attrs, tiling, packing, contract, kc_nest, profile,
        );
        emit.guard_flavor = GuardFlavor::Task;
        emit.split_range = matches!(task.kind, TaskKind::Split { .. });
        emit.watermark = match &task.kind {
            TaskKind::Split { .. } => false,
            TaskKind::Seq { morphisms } => !morphisms.iter().any(|&m| {
                matches!(
                    ir.morphism(m).expect("morphism resolves").op,
                    Operation::Fold { .. } | Operation::LoopEnter
                )
            }),
        };
        emit.prepare_storage();
        emit.frame = Some(frame.clone());

        let mut members = SecondaryMap::new();
        match &task.kind {
            TaskKind::Split { site, .. } => {
                members.insert(*site, ());
            }
            TaskKind::Seq { morphisms } => {
                for &m in morphisms {
                    members.insert(m, ());
                }
            }
        }
        emit.walk_filtered(&members, true);
        emit.line("ret void");
        format!(
            "define internal void @task{task_id}(i64 %lo, i64 %hi, ptr %frame) {{\nentry:\n{}{}{}}}\n",
            emit.allocas, emit.frame_geps, emit.body
        )
    }

    /// The capture count of the Map/Fold site whose body is `self.f` (unique —
    /// lower mints one fresh body fn per lambda site, ADR-0027). `0` when no
    /// site references the fn (a dead body), which is the identity lowering.
    fn body_captures(&self) -> u32 {
        for (_, m) in self.ir.morphisms() {
            match m.op {
                Operation::Map { body, captures } | Operation::Fold { body, captures }
                    if body == self.f =>
                {
                    return captures;
                }
                _ => {}
            }
        }
        0
    }

    fn walk(&mut self) {
        // Driver-owned morphisms: everything in a loop plan's decide/advance
        // cones, plus anything incident to an SCC object. Plan membership is the
        // precise rule — an exit-only payload chain (e.g. a computed exit value,
        // or an exit-arm Print) leaves the SCC but still belongs to the decide
        // cone; skipping by SCC incidence alone would re-emit it after the loop
        // (dead recompute for values, a DOUBLE side effect for Print).
        let mut in_scc: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        let mut owned: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
        for scc in self.ir.loop_structure(self.f) {
            for &mg in &scc.merges {
                if let Some(plan) = self.ir.loop_plan(self.f, mg) {
                    for &mo in plan.decide_order.iter().chain(plan.advance_order.iter()) {
                        owned.insert(mo, ());
                    }
                }
            }
            for o in scc.objects {
                in_scc.insert(o, ());
            }
        }

        for m in self.ir.topo_order(self.f) {
            let morph = self.ir.morphism(m).expect("morphism resolves");
            match morph.op {
                Operation::LoopEnter => {
                    if self.gated.contains_key(m) {
                        // plan-s40: arm-owned loop — its Phi drives it.
                        continue;
                    }
                    crate::loops::emit_loop(self, morph.target)
                }
                Operation::LoopBack | Operation::LoopExit => {}
                _ => {
                    if owned.contains_key(m)
                        || in_scc.contains_key(morph.source)
                        || in_scc.contains_key(morph.target)
                    {
                        continue; // driver-owned
                    }
                    if self.gated.contains_key(m) {
                        continue; // plan-s39: fired only from its Phi's branch
                    }
                    self.emit_morphism(m);
                }
            }
        }
    }

    fn walk_filtered(&mut self, members: &SecondaryMap<MorphismId, ()>, include_members: bool) {
        let mut in_scc: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        let mut owned: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
        for scc in self.ir.loop_structure(self.f) {
            for &merge in &scc.merges {
                if let Some(plan) = self.ir.loop_plan(self.f, merge) {
                    for &m in plan.decide_order.iter().chain(plan.advance_order.iter()) {
                        owned.insert(m, ());
                    }
                }
            }
            for o in scc.objects {
                in_scc.insert(o, ());
            }
        }

        for m in self.ir.topo_order(self.f) {
            if let Some(pin) = self
                .host
                .as_ref()
                .and_then(|host| host.pinned.get(m))
                .cloned()
            {
                self.line(format!(
                    "call void @mapal_par_wait(ptr %h, ptr @pin{}_entries, i32 {})",
                    pin.task, pin.len
                ));
                self.line(format!(
                    "call void @mapal_par_check(ptr %h, i64 {})",
                    pin.topo
                ));
                self.line(format!(
                    "call void @mapal_par_run_pinned(ptr %h, i32 {})",
                    pin.task
                ));
            }

            if members.contains_key(m) != include_members {
                continue;
            }
            let morph = self.ir.morphism(m).expect("morphism resolves");
            match morph.op {
                Operation::LoopEnter => {
                    if self.gated.contains_key(m) {
                        // plan-s40: arm-owned loop — its Phi drives it, and
                        // path_plan folded it into the Phi's Seq task, so no
                        // pre-loop checkpoint belongs here either.
                        continue;
                    }
                    if let Some(list) = self
                        .host
                        .as_ref()
                        .and_then(|host| host.pre_loop.get(m))
                        .cloned()
                    {
                        for c in &list {
                            self.line(format!(
                                "call void @mapal_par_wait(ptr %h, ptr @ckpt{}_entries, i32 {})",
                                c.ordinal, c.len
                            ));
                            self.line(format!(
                                "call void @mapal_par_check(ptr %h, i64 {})",
                                c.topo
                            ));
                        }
                    }
                    crate::loops::emit_loop(self, morph.target)
                }
                Operation::LoopBack | Operation::LoopExit => {}
                _ => {
                    if owned.contains_key(m)
                        || in_scc.contains_key(morph.source)
                        || in_scc.contains_key(morph.target)
                    {
                        continue;
                    }
                    if self.gated.contains_key(m) {
                        continue; // plan-s39: fired only from its Phi's branch
                    }
                    self.emit_morphism(m);
                }
            }
        }
    }

    fn emit_checkpoint(&mut self, m: MorphismId) {
        let Some(checkpoint) = self
            .host
            .as_ref()
            .and_then(|host| host.checkpoints.get(m))
            .cloned()
        else {
            return;
        };
        self.line(format!(
            "call void @mapal_par_wait(ptr %h, ptr @ckpt{}_entries, i32 {})",
            checkpoint.ordinal, checkpoint.len
        ));
        self.line(format!(
            "call void @mapal_par_check(ptr %h, i64 {})",
            checkpoint.topo
        ));
    }

    /// Emit one morphism (DESIGN §2 op table). Called by the straight-line walk
    /// and by the loop driver for decide/advance cones.
    pub(crate) fn emit_morphism(&mut self, m: MorphismId) {
        self.emit_checkpoint(m);
        let morph = self.ir.morphism(m).expect("morphism resolves");
        let op = morph.op;
        let source = morph.source;
        let target = morph.target;

        match op {
            Operation::Pair { slot, .. } => {
                if matches!(self.obj_ty(source), Ty::Array { .. }) {
                    let sllt = lower_ty(&self.obj_ty(source)).expect("array lowers");
                    let src = self
                        .array_operand_ptr(source, None)
                        .expect("Pair array source ptr");
                    let (dllt, dst) = self
                        .component_ptr(target, slot)
                        .expect("Pair array target ptr");
                    if dllt == "ptr" {
                        self.line(format!("store ptr {src}, ptr {dst}"));
                    } else {
                        self.emit_memcpy(&dst, &src, &sllt);
                    }
                } else if let Some((sllt, sval)) = self.load_whole(source)
                    && let Some((_, ptr)) = self.component_ptr(target, slot)
                {
                    self.line(format!("store {sllt} {sval}, ptr {ptr}"));
                }
            }
            Operation::Proj { index } => {
                if matches!(
                    self.obj_ty(source).component_ty(index),
                    Some(Ty::Array { .. })
                ) {
                    let src = self
                        .array_operand_ptr(source, Some(index))
                        .expect("Proj array source ptr");
                    let dst = self.slot(target).expect("Proj array target slot");
                    if self.ptr_resident.contains_key(target) {
                        self.line(format!("store ptr {src}, ptr {dst}"));
                    } else {
                        let llt = lower_ty(&self.obj_ty(target)).expect("Proj array lowers");
                        self.emit_memcpy(&dst, &src, &llt);
                    }
                } else if let Some((cllt, val)) = self.load_component(source, index) {
                    self.store_obj(target, &cllt, &val);
                }
            }
            Operation::Add | Operation::Sub | Operation::Mul | Operation::Div | Operation::Mod => {
                self.emit_arith(m, source, target, op);
            }
            Operation::Neg => {
                let (llt, val) = self.load_whole(source).expect("neg operand");
                let r = self.tmp();
                if is_float(&self.obj_ty(source)) {
                    self.line(format!("{r} = fneg {llt} {val}"));
                } else {
                    self.line(format!("{r} = sub {llt} 0, {val}"));
                }
                self.store_obj(target, &llt, &r);
            }
            Operation::Eq
            | Operation::Neq
            | Operation::Lt
            | Operation::Gt
            | Operation::Le
            | Operation::Ge => {
                self.emit_compare(source, target, op);
            }
            Operation::And | Operation::Or => {
                let (_, a) = self.load_component(source, 0).expect("logic a");
                let (_, b) = self.load_component(source, 1).expect("logic b");
                let iop = if op == Operation::And { "and" } else { "or" };
                let r = self.tmp();
                self.line(format!("{r} = {iop} i1 {a}, {b}"));
                self.store_obj(target, "i1", &r);
            }
            Operation::Not => {
                let (_, val) = self.load_whole(source).expect("not operand");
                let r = self.tmp();
                self.line(format!("{r} = xor i1 {val}, true"));
                self.store_obj(target, "i1", &r);
            }
            Operation::Widen => {
                let (sllt, val) = self.load_whole(source).expect("widen operand");
                let tllt = lower_ty(&self.obj_ty(target)).expect("widen target");
                let cvt = match (self.obj_ty(source), self.obj_ty(target)) {
                    (Ty::Int { bits: 32, .. }, Ty::Int { bits: 64, .. }) => "sext",
                    (Ty::Int { bits: 32, .. }, Ty::Float { .. }) => "sitofp",
                    (Ty::Float { bits: 32 }, Ty::Float { bits: 64 }) => "fpext",
                    _ => unreachable!("invalid Widen pair passed validation"),
                };
                let r = self.tmp();
                self.line(format!("{r} = {cvt} {sllt} {val} to {tllt}"));
                self.store_obj(target, &tllt, &r);
            }
            Operation::Phi => {
                if let Some(site) = self.gsites.get(m).cloned() {
                    // plan-s39: the condition picks the arm and only that
                    // arm's work runs. The cond's Pair edge (slot 2) is
                    // unconditional and already fired; each branch emits its
                    // arm's own-list, then lands the staged value in the
                    // target's slot.
                    let (_, c) = self.load_component(source, 2).expect("phi cond");
                    let bt = self.label();
                    let bf = self.label();
                    let bj = self.label();
                    self.line(format!("br i1 {c}, label %{bt}, label %{bf}"));
                    for (arm, blk, slot) in
                        [(&site.on_true, &bt, 0u32), (&site.on_false, &bf, 1u32)]
                    {
                        self.label_line(blk);
                        for &g in &arm.own {
                            let gm = self.ir.morphism(g).expect("morphism resolves");
                            if gm.op == Operation::LoopEnter {
                                // plan-s40: the handle stands for its whole
                                // loop unit — the driver CFG is emitted inside
                                // this branch.
                                crate::loops::emit_loop(self, gm.target);
                            } else {
                                self.emit_morphism(g);
                            }
                        }
                        if matches!(self.obj_ty(target), Ty::Array { .. }) {
                            let p = self
                                .array_operand_ptr(source, Some(slot))
                                .expect("phi arm array ptr");
                            let dst = self.slot(target).expect("phi array target slot");
                            let llt = lower_ty(&self.obj_ty(target)).expect("phi array lowers");
                            self.emit_memcpy(&dst, &p, &llt);
                        } else {
                            let (tllt, v) =
                                self.load_component(source, slot).expect("phi arm value");
                            self.store_obj(target, &tllt, &v);
                        }
                        self.line(format!("br label %{bj}"));
                    }
                    self.label_line(&bj);
                } else if matches!(self.obj_ty(target), Ty::Array { .. }) {
                    // Hand-built (non-builder) triple: strict select over both
                    // computed arms.
                    let t = self
                        .array_operand_ptr(source, Some(0))
                        .expect("phi then array ptr");
                    let e = self
                        .array_operand_ptr(source, Some(1))
                        .expect("phi else array ptr");
                    let (_, c) = self.load_component(source, 2).expect("phi cond");
                    let r = self.tmp();
                    self.line(format!("{r} = select i1 {c}, ptr {t}, ptr {e}"));
                    let dst = self.slot(target).expect("phi array target slot");
                    let llt = lower_ty(&self.obj_ty(target)).expect("phi array lowers");
                    self.emit_memcpy(&dst, &r, &llt);
                } else {
                    let (tllt, t) = self.load_component(source, 0).expect("phi then");
                    let (_, e) = self.load_component(source, 1).expect("phi else");
                    let (_, c) = self.load_component(source, 2).expect("phi cond");
                    let r = self.tmp();
                    self.line(format!("{r} = select i1 {c}, {tllt} {t}, {tllt} {e}"));
                    self.store_obj(target, &tllt, &r);
                }
            }
            Operation::Call(g) => self.emit_call(source, target, g),
            Operation::Map { body, captures } => self.emit_map(m, source, target, body, captures),
            Operation::Fold { body, captures } => self.emit_fold(source, target, body, captures),
            Operation::Index => self.emit_index(m, source, target),
            Operation::Update => self.emit_update(m, source, target),
            // step 3b: an array every consumer rebuilds from its law is never
            // read, so the store loop that fills it is dead. Skipping it here
            // (rather than trusting DCE) also drops its `%Frame` field, which
            // DCE cannot do — the frame is one object shared across tasks.
            Operation::Zip | Operation::Enumerate | Operation::Iota | Operation::Fill
                if self.elided_arrays.contains_key(target) => {}
            Operation::Zip => self.emit_zip(source, target),
            Operation::Enumerate => self.emit_enumerate(source, target),
            Operation::Iota => self.emit_iota(source, target),
            Operation::Fill => self.emit_fill(source, target),
            Operation::Print { newline } => self.emit_print(source, newline),
            Operation::TimeMs => self.emit_time_ms(target),
            Operation::Output => {
                if matches!(self.obj_ty(source), Ty::Array { .. }) {
                    let src = self
                        .array_operand_ptr(source, None)
                        .expect("Output array source ptr");
                    let dst = self.slot(target).expect("Output array target slot");
                    let llt = lower_ty(&self.obj_ty(source)).expect("Output array lowers");
                    self.emit_memcpy(&dst, &src, &llt);
                } else if let Some((llt, val)) = self.load_whole(source) {
                    self.store_obj(target, &llt, &val);
                }
            }
            Operation::LoopEnter | Operation::LoopBack | Operation::LoopExit => {
                unreachable!("loop ops are driver-owned")
            }
        }
        if self.watermark && self.local_trap_site(m) {
            self.emit_watermark(m);
        }
    }

    fn emit_arith(&mut self, m: MorphismId, source: ObjectId, target: ObjectId, op: Operation) {
        let opty = self
            .obj_ty(source)
            .component_ty(0)
            .cloned()
            .expect("arith ty");
        let (llt, a) = self.load_component(source, 0).expect("arith a");
        let (_, b) = self.load_component(source, 1).expect("arith b");

        if is_float(&opty) {
            let fop = match op {
                Operation::Add => "fadd",
                Operation::Sub => "fsub",
                Operation::Mul => "fmul",
                Operation::Div => "fdiv",
                Operation::Mod => "frem",
                _ => unreachable!(),
            };
            let r = self.tmp();
            self.line(format!("{r} = {fop} {llt} {a}, {b}"));
            self.store_obj(target, &llt, &r);
            return;
        }

        let signed = matches!(opty, Ty::Int { signed: true, .. });
        match op {
            Operation::Add | Operation::Sub | Operation::Mul => {
                let iop = match op {
                    Operation::Add => "add",
                    Operation::Sub => "sub",
                    Operation::Mul => "mul",
                    _ => unreachable!(),
                };
                let r = self.tmp();
                self.line(format!("{r} = {iop} {llt} {a}, {b}")); // no nsw/nuw (wraps, L1)
                self.store_obj(target, &llt, &r);
            }
            Operation::Div | Operation::Mod => {
                // #13 constant-divisor credit (S20): a literal non-zero divisor
                // makes the zero guard dead; a literal non-(−1) makes the
                // MIN/−1 guard dead (the oracle's behavior is identical — the
                // checks cannot fire).
                let dconst = const_int_operand(self.ir, source, 1);
                let zero_dead = matches!(dconst, Some(v) if v != 0);
                let min_dead = matches!(dconst, Some(v) if v != -1);
                if !zero_dead && !matches!(self.guard_flavor, GuardFlavor::Host) {
                    self.emit_task_div(m, target, op, &llt, &a, &b, signed, min_dead);
                    return;
                }
                if !zero_dead {
                    // Zero guard → mapal_trap(div_zero).
                    let z = self.tmp();
                    self.line(format!("{z} = icmp eq {llt} {b}, 0"));
                    self.trap_if(&z, 0);
                }

                if signed && !min_dead {
                    // MIN/-1 guard: Div ⇒ MIN, Mod ⇒ 0 (wrapping_div/rem parity).
                    let min = int_min(&llt);
                    let m1 = self.tmp();
                    self.line(format!("{m1} = icmp eq {llt} {b}, -1"));
                    let ismin = self.tmp();
                    self.line(format!("{ismin} = icmp eq {llt} {a}, {min}"));
                    let ov = self.tmp();
                    self.line(format!("{ov} = and i1 {m1}, {ismin}"));
                    let lov = self.label();
                    let lnorm = self.label();
                    let ldone = self.label();
                    self.line(format!("br i1 {ov}, label %{lov}, label %{lnorm}"));
                    self.label_line(&lov);
                    let ovval = if op == Operation::Div { min } else { "0" };
                    self.store_obj(target, &llt, ovval);
                    self.line(format!("br label %{ldone}"));
                    self.label_line(&lnorm);
                    let sop = if op == Operation::Div { "sdiv" } else { "srem" };
                    let r = self.tmp();
                    self.line(format!("{r} = {sop} {llt} {a}, {b}"));
                    self.store_obj(target, &llt, &r);
                    self.line(format!("br label %{ldone}"));
                    self.label_line(&ldone);
                } else {
                    let sop = match (signed, op) {
                        (true, Operation::Div) => "sdiv",
                        (true, Operation::Mod) => "srem",
                        (false, Operation::Div) => "udiv",
                        (false, Operation::Mod) => "urem",
                        _ => unreachable!(),
                    };
                    let r = self.tmp();
                    self.line(format!("{r} = {sop} {llt} {a}, {b}"));
                    self.store_obj(target, &llt, &r);
                }
            }
            _ => unreachable!(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_task_div(
        &mut self,
        m: MorphismId,
        target: ObjectId,
        op: Operation,
        llt: &str,
        a: &str,
        b: &str,
        signed: bool,
        min_dead: bool,
    ) {
        let zero = self.tmp();
        self.line(format!("{zero} = icmp eq {llt} {b}, 0"));
        let bad = self.label();
        let good = self.label();
        let done = self.label();
        self.line(format!("br i1 {zero}, label %{bad}, label %{good}"));
        self.label_line(&bad);
        self.record_trap(m, 0);
        self.line(format!("br label %{done}"));
        self.label_line(&good);

        let (good_value, good_block) = if signed && !min_dead {
            let min = int_min(llt);
            let minus_one = self.tmp();
            self.line(format!("{minus_one} = icmp eq {llt} {b}, -1"));
            let is_min = self.tmp();
            self.line(format!("{is_min} = icmp eq {llt} {a}, {min}"));
            let overflow = self.tmp();
            self.line(format!("{overflow} = and i1 {minus_one}, {is_min}"));
            let wrap = self.label();
            let normal = self.label();
            let wrapped = self.label();
            self.line(format!("br i1 {overflow}, label %{wrap}, label %{normal}"));
            self.label_line(&wrap);
            self.line(format!("br label %{wrapped}"));
            self.label_line(&normal);
            let real = self.tmp();
            let instruction = if op == Operation::Div { "sdiv" } else { "srem" };
            self.line(format!("{real} = {instruction} {llt} {a}, {b}"));
            self.line(format!("br label %{wrapped}"));
            self.label_line(&wrapped);
            let value = self.tmp();
            let wrap_value = if op == Operation::Div { min } else { "0" };
            self.line(format!(
                "{value} = phi {llt} [{wrap_value}, %{wrap}], [{real}, %{normal}]"
            ));
            (value, wrapped)
        } else {
            let instruction = match (signed, op) {
                (true, Operation::Div) => "sdiv",
                (true, Operation::Mod) => "srem",
                (false, Operation::Div) => "udiv",
                (false, Operation::Mod) => "urem",
                _ => unreachable!(),
            };
            let value = self.tmp();
            self.line(format!("{value} = {instruction} {llt} {a}, {b}"));
            (value, good)
        };
        self.line(format!("br label %{done}"));
        self.label_line(&done);
        let value = self.tmp();
        self.line(format!(
            "{value} = phi {llt} [0, %{bad}], [{good_value}, %{good_block}]"
        ));
        self.store_obj(target, llt, &value);
    }

    fn emit_compare(&mut self, source: ObjectId, target: ObjectId, op: Operation) {
        let opty = self
            .obj_ty(source)
            .component_ty(0)
            .cloned()
            .expect("cmp ty");
        let (llt, a) = self.load_component(source, 0).expect("cmp a");
        let (_, b) = self.load_component(source, 1).expect("cmp b");
        let r = self.tmp();
        if is_float(&opty) {
            let pred = match op {
                Operation::Eq => "oeq",
                Operation::Neq => "une",
                Operation::Lt => "olt",
                Operation::Gt => "ogt",
                Operation::Le => "ole",
                Operation::Ge => "oge",
                _ => unreachable!(),
            };
            self.line(format!("{r} = fcmp {pred} {llt} {a}, {b}"));
        } else {
            let signed = matches!(opty, Ty::Int { signed: true, .. });
            let pred = match op {
                Operation::Eq => "eq",
                Operation::Neq => "ne",
                Operation::Lt => sign_pred(signed, "slt", "ult"),
                Operation::Gt => sign_pred(signed, "sgt", "ugt"),
                Operation::Le => sign_pred(signed, "sle", "ule"),
                Operation::Ge => sign_pred(signed, "sge", "uge"),
                _ => unreachable!(),
            };
            self.line(format!("{r} = icmp {pred} {llt} {a}, {b}"));
        }
        self.store_obj(target, "i1", &r);
    }

    /// A Named call (BL5 amendment, suggestions #8): top-level Array
    /// (components of the) argument go **by reference** — array parameters are
    /// observably read-only (Mapal value semantics; functional `Update` copies
    /// to a fresh alloca), so the address is observably the inline array, and
    /// no array bytes cross the call boundary. The lowering is per-signature
    /// (`lower_named_input_ty`), so every call site agrees with the callee's
    /// `FnEmit` by construction — host paths and body fns alike. Scalar-only
    /// arguments keep the by-value form byte-identical.
    fn emit_call(&mut self, source: ObjectId, target: ObjectId, g: FuncId) {
        let callee = self.fnames[g].clone();
        let cfd = self.ir.func(g).expect("callee");
        let in_ty = self.obj_ty(cfd.input);
        let out_ty = self.obj_ty(cfd.output);
        let arg = match lower_named_input_ty(&in_ty) {
            None => String::new(),
            Some(_) if !has_top_array(&in_ty) => {
                let (llt, val) = self.load_whole(source).expect("call arg");
                format!("{llt} {val}")
            }
            Some(arg_llt) if arg_llt == "ptr" => {
                // A single surviving array argument: the bare source's address,
                // or the one array component's.
                let addr = if matches!(&in_ty, Ty::Array { .. }) {
                    self.array_operand_ptr(source, None)
                } else {
                    let k = (0u32..)
                        .find(|&k| matches!(in_ty.component_ty(k), Some(Ty::Array { .. })))
                        .expect("the surviving array component");
                    self.array_operand_ptr(source, Some(k))
                };
                format!("ptr {}", addr.expect("call array arg"))
            }
            Some(arg_llt) => {
                // The product argument, assembled in scratch: Array components
                // store their address, everything else its value — the
                // `body_call_arg` template with every component a "capture".
                self.body_call_arg(source, product_arity(&in_ty), &in_ty, &arg_llt, &[])
            }
        };
        match lower_ty(&out_ty) {
            None => self.line(format!("call void @{callee}({arg})")),
            Some(rty) => {
                let r = self.tmp();
                self.line(format!("{r} = call {rty} @{callee}({arg})"));
                self.store_obj(target, &rty, &r);
            }
        }
    }

    fn emit_print(&mut self, source: ObjectId, newline: bool) {
        let nl = if newline { "true" } else { "false" };
        let pty = self
            .obj_ty(source)
            .component_ty(1)
            .cloned()
            .expect("print printable");
        if pty == Ty::Str {
            // Str comes only from a literal (I9s): the slot-1 Pair source is a
            // Str constant with a private global.
            let p_obj = self.pair_source(source, 1).expect("str print source");
            let g = self.strings.get(p_obj).expect("str global");
            let len = g.bytes.len();
            let name = g.name.clone();
            self.line(format!(
                "call void @mapal_print_str(ptr {name}, i64 {len}, i1 zeroext {nl})"
            ));
            return;
        }
        let (func, ze, tystr) = print_dispatch(&pty);
        let (_, val) = self.load_component(source, 1).expect("print value");
        // Param attr goes *after* the type in a call arg (`i8 zeroext %v`), like
        // the trailing newline `i1 zeroext` — attr-before-type is invalid LLVM.
        let ze = if ze { "zeroext " } else { "" };
        self.line(format!(
            "call void @{func}({tystr} {ze}{val}, i1 zeroext {nl})"
        ));
    }

    /// `time` (plan-time-builtin): the monotonic clock read. The source token
    /// is ordering-only and erases, and the `(IoToken, f64)` target residual-
    /// lowers to the bare `double` (ty.rs `lower_ty`: a one-component residual
    /// IS its component) — so the call result is the target object's value and
    /// no pair is materialized. Emission position in the block IS the ordering
    /// the token models.
    fn emit_time_ms(&mut self, target: ObjectId) {
        let r = self.tmp();
        self.line(format!("{r} = call double @mapal_time_ms()"));
        self.store_obj(target, "double", &r);
    }

    /// The source object of the `Pair{slot==k}` edge feeding aggregate `agg`.
    fn pair_source(&self, agg: ObjectId, k: u32) -> Option<ObjectId> {
        for &m in self.ir.in_edges(agg) {
            let morph = self.ir.morphism(m).expect("morphism");
            if let Operation::Pair { slot, .. } = morph.op
                && slot == k
            {
                return Some(morph.source);
            }
        }
        None
    }

    /// Rule 4's in-place legality for the `Update` morphism `m` (plan-last-use
    /// §2; suggestions #2): the source array object, when the plan proves it
    /// `dead_after` this Update — every use ranked at/before it under rule 1's
    /// decide < `LoopExit` < advance < `LoopBack`, ¬escapes, ¬carried — so the
    /// whole-array memcpy may be skipped and the target may SHARE the source's
    /// slot (the element store lands in place; in the loop-carried matmul4
    /// shape the write goes straight into the merge's own storage and the
    /// back-edge copy becomes an identity). A ptr-resident (by-ref) source is
    /// never eligible — the store would land in caller memory (the plan's
    /// rule 2 already marks borrowed inputs escaping; this check is the
    /// emitted-text-level second line). `None` keeps the fresh-alloca copy.
    fn update_in_place_source(&self, m: MorphismId) -> Option<ObjectId> {
        let morph = self.ir.morphism(m).expect("morphism resolves");
        if morph.op != Operation::Update {
            return None;
        }
        let arr = self.pair_source(morph.source, 0)?;
        if self.ptr_resident.contains_key(arr)
            || self.ir.object(arr).expect("object resolves").kind == ObjectKind::Constant
        {
            return None;
        }
        let idx = self.lup.position(m)?;
        self.lup.dead_after(arr, idx).then_some(arr)
    }

    fn emit_index(&mut self, m: MorphismId, source: ObjectId, target: ObjectId) {
        let src_ty = self.obj_ty(source);
        let arr_ty = src_ty.component_ty(0).cloned().expect("index array");
        let (elem_llt, size) = array_parts(&arr_ty);
        let arr_llt = lower_ty(&arr_ty).expect("array lowers");
        let arr_ptr = self
            .array_operand_ptr(source, Some(0))
            .expect("index array ptr");
        let idx_ty = src_ty.component_ty(1).cloned().expect("index i");
        let i64idx = self.load_index(source, 1, &idx_ty);
        // Guard elision (S20 `bounds_proof`; the vectorization unlock): the
        // plan proves the index statically inside `[0, size)` — the trap is
        // dead, so a proven `Index` emits just the GEP+load. Everything
        // unproven keeps the two-sided guard byte-identical.
        if !self.bp.proven(m) {
            if !matches!(self.guard_flavor, GuardFlavor::Host) {
                let oob = self.index_oob(&i64idx, size);
                let bad = self.label();
                let good = self.label();
                let done = self.label();
                self.line(format!("br i1 {oob}, label %{bad}, label %{good}"));
                self.label_line(&bad);
                self.record_trap(m, 1);
                self.line(format!("br label %{done}"));
                self.label_line(&good);
                let ep = self.tmp();
                self.line(format!(
                    "{ep} = getelementptr {arr_llt}, ptr {arr_ptr}, i64 0, i64 {i64idx}"
                ));
                let loaded = self.tmp();
                self.line(format!("{loaded} = load {elem_llt}, ptr {ep}"));
                self.line(format!("br label %{done}"));
                self.label_line(&done);
                let value = self.tmp();
                self.line(format!(
                    "{value} = phi {elem_llt} [zeroinitializer, %{bad}], [{loaded}, %{good}]"
                ));
                self.store_obj(target, &elem_llt, &value);
                return;
            }
            self.guard_index(&i64idx, size);
        }
        let ep = self.tmp();
        self.line(format!(
            "{ep} = getelementptr {arr_llt}, ptr {arr_ptr}, i64 0, i64 {i64idx}"
        ));
        let v = self.tmp();
        self.line(format!("{v} = load {elem_llt}, ptr {ep}"));
        self.store_obj(target, &elem_llt, &v);
    }

    fn emit_update(&mut self, m: MorphismId, source: ObjectId, target: ObjectId) {
        let src_ty = self.obj_ty(source);
        let arr_ty = src_ty.component_ty(0).cloned().expect("update array");
        let (_elem_llt, size) = array_parts(&arr_ty);
        let arr_llt = lower_ty(&arr_ty).expect("array lowers");
        let idx_ty = src_ty.component_ty(1).cloned().expect("update i");
        // Last-use elision (suggestions #2; plan-last-use §2 rule 4): a dead
        // source shares its slot with the target — the memcpy is skipped and
        // the element store below lands in place. The copy path emits
        // byte-identical text to before.
        let (tgt_slot, copy_from) = match self.update_in_place_source(m) {
            Some(arr) => {
                let slot = self.slot(arr).expect("in-place update source slot");
                self.slots.insert(target, slot.clone());
                (slot, None)
            }
            None => (
                self.slot(target).expect("update target slot"),
                Some(
                    self.array_operand_ptr(source, Some(0))
                        .expect("update src ptr"),
                ),
            ),
        };
        let i64idx = self.load_index(source, 1, &idx_ty);
        if !matches!(self.guard_flavor, GuardFlavor::Host) {
            if let Some(src_arr_ptr) = copy_from {
                self.line(format!(
                    "call void @llvm.memcpy.p0.p0.i64(ptr {tgt_slot}, ptr {src_arr_ptr}, i64 ptrtoint (ptr getelementptr ({arr_llt}, ptr null, i64 1) to i64), i1 false)"
                ));
            }
            let oob = self.index_oob(&i64idx, size);
            let bad = self.label();
            let good = self.label();
            let done = self.label();
            self.line(format!("br i1 {oob}, label %{bad}, label %{good}"));
            self.label_line(&bad);
            self.record_trap(m, 1);
            self.line(format!("br label %{done}"));
            self.label_line(&good);
            let ep = self.tmp();
            self.line(format!(
                "{ep} = getelementptr {arr_llt}, ptr {tgt_slot}, i64 0, i64 {i64idx}"
            ));
            let (vllt, val) = self.load_component(source, 2).expect("update value");
            self.line(format!("store {vllt} {val}, ptr {ep}"));
            self.line(format!("br label %{done}"));
            self.label_line(&done);
            return;
        }
        self.guard_index(&i64idx, size);
        if let Some(src_arr_ptr) = copy_from {
            // memcpy source array → target (fresh array; ADR-0021). Size via the
            // gep-null sizeof constant expr (handles element padding).
            self.line(format!(
                "call void @llvm.memcpy.p0.p0.i64(ptr {tgt_slot}, ptr {src_arr_ptr}, i64 ptrtoint (ptr getelementptr ({arr_llt}, ptr null, i64 1) to i64), i1 false)"
            ));
        }
        let ep = self.tmp();
        self.line(format!(
            "{ep} = getelementptr {arr_llt}, ptr {tgt_slot}, i64 0, i64 {i64idx}"
        ));
        let (vllt, val) = self.load_component(source, 2).expect("update value");
        self.line(format!("store {vllt} {val}, ptr {ep}"));
    }

    /// Load index component `k`, zero/sign-extended to i64 per its ty (S13
    /// type-directed rule: u8 zext, signed sext).
    fn load_index(&mut self, agg: ObjectId, k: u32, idx_ty: &Ty) -> String {
        let (illt, idx) = self.load_component(agg, k).expect("index operand");
        if illt == "i64" {
            return idx;
        }
        let ext = if matches!(idx_ty, Ty::Int { signed: false, .. }) {
            "zext"
        } else {
            "sext"
        };
        let e = self.tmp();
        self.line(format!("{e} = {ext} {illt} {idx} to i64"));
        e
    }

    /// Trap when the i64 index is out of `[0, size)`. Unsigned (u8) indices skip
    /// the lower bound (they zero-extend, never negative — S13).
    fn guard_index(&mut self, i64idx: &str, size: u64) {
        // The extension already erased signedness; but the operand's original
        // signedness decided zext vs sext. A zero-extended value is ≥ 0, so the
        // signed two-sided check is always correct on the i64 form.
        let oob = self.index_oob(i64idx, size);
        self.trap_if(&oob, 1);
    }

    fn index_oob(&mut self, i64idx: &str, size: u64) -> String {
        let lo = self.tmp();
        self.line(format!("{lo} = icmp slt i64 {i64idx}, 0"));
        let hi = self.tmp();
        self.line(format!("{hi} = icmp sge i64 {i64idx}, {size}"));
        let oob = self.tmp();
        self.line(format!("{oob} = or i1 {lo}, {hi}"));
        oob
    }

    /// Assemble a capturing map/fold body's call operand (ADR-0027): the
    /// `(c₁…cₖ, rest…)` product in a fresh scratch — capture components `0..k`
    /// loaded from the op's source product (the broadcast edges), then the
    /// per-iteration `rest` components (`elem` for map; `acc, elem` for fold).
    /// Returns the `{llt} {val}` call operand. Erasure applies as usual: an
    /// erased component has no representation (`field_store` remaps via
    /// `erased_index`; a lowered capture is never erased — L1605).
    ///
    /// An Array capture travels **by reference** (suggestions #6): its scratch
    /// field is a `ptr` (matching the body fn's by-ref signature) holding the
    /// array's address — the forwarded capture pointer when the `Pair` feeder
    /// is itself ptr-resident (the transitive fold-in-map case), the source
    /// product's component address otherwise. No array bytes move per call.
    /// (`emit_call` reuses this template for a Named call's whole argument —
    /// every component a "capture", no `rest` — suggestions #8.)
    fn body_call_arg(
        &mut self,
        source: ObjectId,
        captures: u32,
        arg_ty: &Ty,
        arg_llt: &str,
        rest: &[(&str, &str)],
    ) -> String {
        let buf = self.scratch(arg_llt);
        for i in 0..captures {
            if matches!(arg_ty.component_ty(i), Some(Ty::Array { .. })) {
                let addr = self
                    .array_operand_ptr(source, Some(i))
                    .expect("body capture addr");
                self.field_store(&buf, arg_ty, arg_llt, i, "ptr", &addr);
            } else {
                let (cllt, cval) = self.load_component(source, i).expect("body capture");
                self.field_store(&buf, arg_ty, arg_llt, i, &cllt, &cval);
            }
        }
        for (j, &(rllt, rval)) in rest.iter().enumerate() {
            self.field_store(&buf, arg_ty, arg_llt, captures + j as u32, rllt, rval);
        }
        let whole = self.tmp();
        self.line(format!("{whole} = load {arg_llt}, ptr {buf}"));
        format!("{arg_llt} {whole}")
    }

    fn emit_tile_index(
        &mut self,
        mut index: Option<String>,
        terms: &[(u64, &str)],
    ) -> Option<String> {
        for &(coefficient, variable) in terms {
            if coefficient == 0 {
                continue;
            }
            let term = if coefficient == 1 {
                variable.to_owned()
            } else {
                let scaled = self.tmp();
                self.line(format!("{scaled} = mul i64 {variable}, {coefficient}"));
                scaled
            };
            index = Some(match index {
                Some(base) => {
                    let sum = self.tmp();
                    self.line(format!("{sum} = add i64 {base}, {term}"));
                    sum
                }
                None => term,
            });
        }
        index
    }

    fn emit_tiled_map(
        &mut self,
        source: ObjectId,
        target: ObjectId,
        site: &TileSite,
        packed: Option<PackedBuffer>,
    ) {
        // S28 conv rung: k-split sites (conv2d's (k÷div, k%div) window taps)
        // take the unrolled micro-kernel — every tap offset is compile-time.
        if conv_site(site) {
            self.emit_tiled_map_conv(source, target, site);
            return;
        }

        // S28 window rung: 1-D sliding-window sites (FIR) take TI register
        // blocks over the lane axis — one scalar `a` load per k shared across
        // TI subrows, constant TJ everywhere on the main path. Non-window 1-D
        // sites keep the rung-1 nest byte-for-byte (the negative control).
        if window1d_site(site) {
            self.emit_tiled_map_blocked_1d(source, target, site);
            return;
        }

        // S26 rung 2 gate: TI register blocking cashes the record's
        // row-invariance fact (`b.ci == 0`) on multi-row sites. Every other
        // site (1-D FIR/attention-O has `rows == 1`) keeps the rung-1 nest.
        if site.rows > 1 && site.b.ci == 0 {
            self.emit_tiled_map_blocked(source, target, site, packed);
            return;
        }

        let source_ty = self.obj_ty(source);
        let a_ty = source_ty
            .component_ty(site.a.slot)
            .cloned()
            .expect("tile a array");
        let b_ty = source_ty
            .component_ty(site.b.slot)
            .cloned()
            .expect("tile b array");
        let a_llt = lower_ty(&a_ty).expect("tile a lowers");
        let b_llt = lower_ty(&b_ty).expect("tile b lowers");
        let out_ty = self.obj_ty(target);
        let out_llt = lower_ty(&out_ty).expect("tile output lowers");
        let (_, n) = array_parts(&out_ty);
        debug_assert_eq!(n, site.rows * site.c);
        let elem_llt = lower_ty(&site.elem).expect("tile element lowers");
        let seed = const_literal(&site.seed);
        let mul_op = if is_float(&site.elem) { "fmul" } else { "mul" };
        let add_op = if is_float(&site.elem) { "fadd" } else { "add" };
        let tile_j = self.profile.tile_j(&site.elem);
        let contract_flag = if self.contract && is_float(&site.elem) {
            " contract"
        } else {
            ""
        };

        let a_ptr = self
            .array_operand_ptr(source, Some(site.a.slot))
            .expect("tile a ptr");
        let b_ptr = self
            .array_operand_ptr(source, Some(site.b.slot))
            .expect("tile b ptr");
        let out_ptr = self.slot(target).expect("tile output slot");
        let acc_llt = format!("[{tile_j} x {elem_llt}]");
        let acc = self.scratch(&acc_llt);
        let i_ctr = self.scratch("i64");
        let j_ctr = self.scratch("i64");
        let k_ctr = self.scratch("i64");
        let lane_ctr = self.scratch("i64");
        let (lo, hi) = self.bulk_bounds(n);

        let i_lo = self.tmp();
        self.line(format!("{i_lo} = udiv i64 {lo}, {}", site.c));
        let hi_biased = self.tmp();
        self.line(format!("{hi_biased} = add i64 {hi}, {}", site.c - 1));
        let i_hi = self.tmp();
        self.line(format!("{i_hi} = udiv i64 {hi_biased}, {}", site.c));
        self.line(format!("store i64 {i_lo}, ptr {i_ctr}"));

        let (i_head, i_body, i_done) = (self.label(), self.label(), self.label());
        let (j_head, j_body, j_done) = (self.label(), self.label(), self.label());
        let (seed_head, seed_body, seed_done) = (self.label(), self.label(), self.label());
        let (k_head, k_body, k_done) = (self.label(), self.label(), self.label());
        let (inner_head, inner_body, inner_done) = (self.label(), self.label(), self.label());
        let (store_head, store_body, store_done) = (self.label(), self.label(), self.label());

        self.line(format!("br label %{i_head}"));
        self.label_line(&i_head);
        let i = self.tmp();
        self.line(format!("{i} = load i64, ptr {i_ctr}"));
        let rows_done = self.tmp();
        self.line(format!("{rows_done} = icmp uge i64 {i}, {i_hi}"));
        self.line(format!(
            "br i1 {rows_done}, label %{i_done}, label %{i_body}"
        ));

        self.label_line(&i_body);
        let row0 = self.tmp();
        self.line(format!("{row0} = mul i64 {i}, {}", site.c));
        let jw_lo_raw = self.tmp();
        self.line(format!("{jw_lo_raw} = sub i64 {lo}, {row0}"));
        let jw_lo_negative = self.tmp();
        self.line(format!("{jw_lo_negative} = icmp slt i64 {jw_lo_raw}, 0"));
        let jw_lo = self.tmp();
        self.line(format!(
            "{jw_lo} = select i1 {jw_lo_negative}, i64 0, i64 {jw_lo_raw}"
        ));
        let jw_hi_raw = self.tmp();
        self.line(format!("{jw_hi_raw} = sub i64 {hi}, {row0}"));
        let jw_hi_past_c = self.tmp();
        self.line(format!(
            "{jw_hi_past_c} = icmp sgt i64 {jw_hi_raw}, {}",
            site.c
        ));
        let jw_hi = self.tmp();
        self.line(format!(
            "{jw_hi} = select i1 {jw_hi_past_c}, i64 {}, i64 {jw_hi_raw}",
            site.c
        ));
        let a_row = self.emit_tile_index(
            (site.a.base != 0).then(|| site.a.base.to_string()),
            &[(site.a.ci, i.as_str())],
        );
        let b_row = self.emit_tile_index(
            (site.b.base != 0).then(|| site.b.base.to_string()),
            &[(site.b.ci, i.as_str())],
        );
        self.line(format!("store i64 {jw_lo}, ptr {j_ctr}"));
        self.line(format!("br label %{j_head}"));

        self.label_line(&j_head);
        let j0 = self.tmp();
        self.line(format!("{j0} = load i64, ptr {j_ctr}"));
        let columns_done = self.tmp();
        self.line(format!("{columns_done} = icmp uge i64 {j0}, {jw_hi}"));
        self.line(format!(
            "br i1 {columns_done}, label %{j_done}, label %{j_body}"
        ));

        self.label_line(&j_body);
        let remaining = self.tmp();
        self.line(format!("{remaining} = sub i64 {jw_hi}, {j0}"));
        let partial = self.tmp();
        self.line(format!("{partial} = icmp ult i64 {remaining}, {tile_j}"));
        let tj = self.tmp();
        self.line(format!(
            "{tj} = select i1 {partial}, i64 {remaining}, i64 {tile_j}"
        ));
        self.line(format!("store i64 0, ptr {lane_ctr}"));
        self.line(format!("br label %{seed_head}"));

        self.label_line(&seed_head);
        let seed_lane = self.tmp();
        self.line(format!("{seed_lane} = load i64, ptr {lane_ctr}"));
        let seed_done_cond = self.tmp();
        self.line(format!("{seed_done_cond} = icmp uge i64 {seed_lane}, {tj}"));
        self.line(format!(
            "br i1 {seed_done_cond}, label %{seed_done}, label %{seed_body}"
        ));
        self.label_line(&seed_body);
        let seed_ptr = self.tmp();
        self.line(format!(
            "{seed_ptr} = getelementptr {acc_llt}, ptr {acc}, i64 0, i64 {seed_lane}"
        ));
        self.line(format!("store {elem_llt} {seed}, ptr {seed_ptr}"));
        let seed_lane_next = self.tmp();
        self.line(format!("{seed_lane_next} = add i64 {seed_lane}, 1"));
        self.line(format!("store i64 {seed_lane_next}, ptr {lane_ctr}"));
        self.line(format!("br label %{seed_head}"));

        self.label_line(&seed_done);
        self.line(format!("store i64 0, ptr {k_ctr}"));
        self.line(format!("br label %{k_head}"));

        self.label_line(&k_head);
        let kk = self.tmp();
        self.line(format!("{kk} = load i64, ptr {k_ctr}"));
        let depth_done = self.tmp();
        self.line(format!("{depth_done} = icmp uge i64 {kk}, {}", site.k));
        self.line(format!(
            "br i1 {depth_done}, label %{k_done}, label %{k_body}"
        ));

        self.label_line(&k_body);
        let a_index = self
            .emit_tile_index(a_row.clone(), &[(site.a.ck, kk.as_str())])
            .unwrap_or_else(|| "0".to_owned());
        let a_elem_ptr = self.tmp();
        self.line(format!(
            "{a_elem_ptr} = getelementptr {a_llt}, ptr {a_ptr}, i64 0, i64 {a_index}"
        ));
        let a_value = self.tmp();
        self.line(format!("{a_value} = load {elem_llt}, ptr {a_elem_ptr}"));
        let b_start = packed.is_none().then(|| {
            self.emit_tile_index(b_row.clone(), &[(site.b.ck, kk.as_str()), (1, j0.as_str())])
                .expect("tile b has lane term")
        });
        self.line(format!("store i64 0, ptr {lane_ctr}"));
        self.line(format!("br label %{inner_head}"));

        self.label_line(&inner_head);
        let lane = self.tmp();
        self.line(format!("{lane} = load i64, ptr {lane_ctr}"));
        let inner_done_cond = self.tmp();
        self.line(format!("{inner_done_cond} = icmp uge i64 {lane}, {tj}"));
        self.line(format!(
            "br i1 {inner_done_cond}, label %{inner_done}, label %{inner_body}"
        ));

        self.label_line(&inner_body);
        let (b_arr_llt, b_base, b_index) = if let Some(packed) = &packed {
            let j = self.tmp();
            self.line(format!("{j} = add i64 {j0}, {lane}"));
            let jt = self.tmp();
            self.line(format!("{jt} = udiv i64 {j}, {tile_j}"));
            let panel_lane = self.tmp();
            self.line(format!("{panel_lane} = urem i64 {j}, {tile_j}"));
            let panel_base = self.tmp();
            self.line(format!("{panel_base} = mul i64 {jt}, {}", site.k * tile_j));
            let k_base = self.tmp();
            self.line(format!("{k_base} = mul i64 {kk}, {tile_j}"));
            let row = self.tmp();
            self.line(format!("{row} = add i64 {panel_base}, {k_base}"));
            let index = self.tmp();
            self.line(format!("{index} = add i64 {row}, {panel_lane}"));
            (packed.llt.as_str(), packed.ptr.as_str(), index)
        } else {
            let index = self.tmp();
            self.line(format!(
                "{index} = add i64 {}, {lane}",
                b_start.as_ref().expect("unpacked b start")
            ));
            (b_llt.as_str(), b_ptr.as_str(), index)
        };
        let b_elem_ptr = self.tmp();
        self.line(format!(
            "{b_elem_ptr} = getelementptr {b_arr_llt}, ptr {b_base}, i64 0, i64 {b_index}"
        ));
        let b_value = self.tmp();
        self.line(format!("{b_value} = load {elem_llt}, ptr {b_elem_ptr}"));
        let product = self.tmp();
        let (mul_lhs, mul_rhs) = if site.mul_a_first {
            (&a_value, &b_value)
        } else {
            (&b_value, &a_value)
        };
        self.line(format!(
            "{product} = {mul_op}{contract_flag} {elem_llt} {mul_lhs}, {mul_rhs}"
        ));
        let acc_ptr = self.tmp();
        self.line(format!(
            "{acc_ptr} = getelementptr {acc_llt}, ptr {acc}, i64 0, i64 {lane}"
        ));
        let acc_value = self.tmp();
        self.line(format!("{acc_value} = load {elem_llt}, ptr {acc_ptr}"));
        let sum = self.tmp();
        let (add_lhs, add_rhs) = if site.add_acc_first {
            (&acc_value, &product)
        } else {
            (&product, &acc_value)
        };
        self.line(format!(
            "{sum} = {add_op}{contract_flag} {elem_llt} {add_lhs}, {add_rhs}"
        ));
        self.line(format!("store {elem_llt} {sum}, ptr {acc_ptr}"));
        let lane_next = self.tmp();
        self.line(format!("{lane_next} = add i64 {lane}, 1"));
        self.line(format!("store i64 {lane_next}, ptr {lane_ctr}"));
        self.line(format!("br label %{inner_head}"));

        self.label_line(&inner_done);
        let kk_next = self.tmp();
        self.line(format!("{kk_next} = add i64 {kk}, 1"));
        self.line(format!("store i64 {kk_next}, ptr {k_ctr}"));
        self.line(format!("br label %{k_head}"));

        self.label_line(&k_done);
        let out_start = self.tmp();
        self.line(format!("{out_start} = add i64 {row0}, {j0}"));
        self.line(format!("store i64 0, ptr {lane_ctr}"));
        self.line(format!("br label %{store_head}"));

        self.label_line(&store_head);
        let store_lane = self.tmp();
        self.line(format!("{store_lane} = load i64, ptr {lane_ctr}"));
        let stores_done = self.tmp();
        self.line(format!("{stores_done} = icmp uge i64 {store_lane}, {tj}"));
        self.line(format!(
            "br i1 {stores_done}, label %{store_done}, label %{store_body}"
        ));

        self.label_line(&store_body);
        let final_acc_ptr = self.tmp();
        self.line(format!(
            "{final_acc_ptr} = getelementptr {acc_llt}, ptr {acc}, i64 0, i64 {store_lane}"
        ));
        let final_value = self.tmp();
        self.line(format!(
            "{final_value} = load {elem_llt}, ptr {final_acc_ptr}"
        ));
        let out_index = self.tmp();
        self.line(format!("{out_index} = add i64 {out_start}, {store_lane}"));
        let out_elem_ptr = self.tmp();
        self.line(format!(
            "{out_elem_ptr} = getelementptr {out_llt}, ptr {out_ptr}, i64 0, i64 {out_index}"
        ));
        self.line(format!(
            "store {elem_llt} {final_value}, ptr {out_elem_ptr}"
        ));
        let store_lane_next = self.tmp();
        self.line(format!("{store_lane_next} = add i64 {store_lane}, 1"));
        self.line(format!("store i64 {store_lane_next}, ptr {lane_ctr}"));
        self.line(format!("br label %{store_head}"));

        self.label_line(&store_done);
        let j0_next = self.tmp();
        self.line(format!("{j0_next} = add i64 {j0}, {tile_j}"));
        self.line(format!("store i64 {j0_next}, ptr {j_ctr}"));
        self.line(format!("br label %{j_head}"));

        self.label_line(&j_done);
        let i_next = self.tmp();
        self.line(format!("{i_next} = add i64 {i}, 1"));
        self.line(format!("store i64 {i_next}, ptr {i_ctr}"));
        self.line(format!("br label %{i_head}"));
        self.label_line(&i_done);
    }

    /// The gated tiled nest: TI register blocking + fixed-TJ main/remainder
    /// splitting. Packed sites put j panels outside the unchanged
    /// head/interior/tail i regions; unpacked sites retain the i-outer S26
    /// order byte-for-byte. A block [i, i+TI) is legal only where every
    /// subrow's lane window is the whole [0, C), so boundary and tail rows use
    /// TI=1 and are never masked. Per cell the chain stays k-ascending.
    fn emit_tiled_map_blocked(
        &mut self,
        source: ObjectId,
        target: ObjectId,
        site: &TileSite,
        packed: Option<PackedBuffer>,
    ) {
        let source_ty = self.obj_ty(source);
        let a_ty = source_ty
            .component_ty(site.a.slot)
            .cloned()
            .expect("tile a array");
        let b_ty = source_ty
            .component_ty(site.b.slot)
            .cloned()
            .expect("tile b array");
        let a_llt = lower_ty(&a_ty).expect("tile a lowers");
        let b_llt = lower_ty(&b_ty).expect("tile b lowers");
        let out_ty = self.obj_ty(target);
        let out_llt = lower_ty(&out_ty).expect("tile output lowers");
        let (_, n) = array_parts(&out_ty);
        debug_assert_eq!(n, site.rows * site.c);
        let elem_llt = lower_ty(&site.elem).expect("tile element lowers");
        let mul_op = if is_float(&site.elem) { "fmul" } else { "mul" };
        let add_op = if is_float(&site.elem) { "fadd" } else { "add" };
        let tile_j = self.profile.tile_j(&site.elem);
        let tile_i = self.profile.tile_i();
        // The k-panel depth is derived from half of L2 at this element width,
        // so the KC gate below closes by DERIVATION on a machine whose L2 is
        // deep enough to hold the panel the nest exists to avoid re-reading —
        // S29/S30's measured verdict, deduced instead of hardcoded default-off.
        let tile_kc = self.profile.tile_kc(&site.elem);

        let a_ptr = self
            .array_operand_ptr(source, Some(site.a.slot))
            .expect("tile a ptr");
        let b_ptr = self
            .array_operand_ptr(source, Some(site.b.slot))
            .expect("tile b ptr");
        let out_ptr = self.slot(target).expect("tile output slot");
        // One j-tile of accumulators (TI subrows × TJ lanes), for BOTH nests.
        // The KC rung parks its partial sums in `out` at every panel end (the
        // (jc, kc, ic) order runs other i-blocks between two panels of the same
        // block, so nothing survives in scratch across a panel) — so only the
        // j-tile currently being computed is ever live, exactly as in the
        // j-outer nest. A TI×NC block would be 32× dead space.
        let kc_nest = self.kc_nest && packed.is_some() && site.k > tile_kc;
        let acc_lanes = tile_i * tile_j;
        let acc_llt = format!("[{acc_lanes} x {elem_llt}]");
        let acc = self.scratch(&acc_llt);
        // The a-panel pack scratch: TI strided source rows × one k-panel,
        // copied contiguous (align 64) per (i-block, kc) visit.
        let apack = kc_nest.then(|| {
            let llt = format!("[{} x {elem_llt}]", tile_i * tile_kc);
            let ptr = format!("%s{}", self.fresh());
            self.allocas
                .push_str(&format!("  {ptr} = alloca {llt}, align 64\n"));
            PackedBuffer { ptr, llt }
        });
        let i_ctr = self.scratch("i64");
        let j_ctr = self.scratch("i64");
        let k_ctr = self.scratch("i64");
        let lane_ctr = self.scratch("i64");
        let ctx = TileCtx {
            acc,
            acc_llt,
            elem_llt,
            seed: const_literal(&site.seed),
            mul_op,
            add_op,
            a_ptr,
            b_ptr,
            out_ptr,
            a_llt,
            b_llt,
            out_llt,
            k_ctr,
            lane_ctr,
            tile_j,
            tile_i,
            tile_kc,
            packed,
            contract_flag: if self.contract && is_float(&site.elem) {
                " contract"
            } else {
                ""
            },
        };
        let (lo, hi) = self.bulk_bounds(n);

        let i_lo = self.tmp();
        self.line(format!("{i_lo} = udiv i64 {lo}, {}", site.c));
        let hi_biased = self.tmp();
        self.line(format!("{hi_biased} = add i64 {hi}, {}", site.c - 1));
        let i_hi = self.tmp();
        self.line(format!("{i_hi} = udiv i64 {hi_biased}, {}", site.c));
        // Interior full-window rows are [ceil(lo/C), floor(hi/C)): row i has
        // the whole lane window iff lo - i*C <= 0 and hi - i*C >= C.
        let lo_biased = self.tmp();
        self.line(format!("{lo_biased} = add i64 {lo}, {}", site.c - 1));
        let i_fw_lo = self.tmp();
        self.line(format!("{i_fw_lo} = udiv i64 {lo_biased}, {}", site.c));
        let i_fw_hi = self.tmp();
        self.line(format!("{i_fw_hi} = udiv i64 {hi}, {}", site.c));

        if ctx.packed.is_some() {
            if let Some(apack) = &apack {
                self.emit_tile_packed_kc(
                    site, &ctx, &i_ctr, &j_ctr, &lo, &hi, &i_lo, &i_hi, &i_fw_lo, &i_fw_hi, apack,
                );
            } else {
                self.emit_tile_packed_j_outer(
                    site, &ctx, &i_ctr, &j_ctr, &lo, &hi, &i_lo, &i_hi, &i_fw_lo, &i_fw_hi,
                );
            }
            return;
        }
        self.emit_tile_i_regions(
            site, &ctx, &i_ctr, &j_ctr, &lo, &hi, &i_lo, &i_hi, &i_fw_lo, &i_fw_hi, None,
        );
    }

    /// The 1-D sliding-window (FIR) nest: the rung-2 dual. Full blocks step
    /// TI·TJ lanes with no masking — per block, a per-subrow seed splat, the k
    /// loop (×2 unrolled when K is even, the trio's shape) doing ONE scalar
    /// `a` load per k shared across every subrow's constant-TJ lane loop
    /// (`a` is the invariant read, `b` slides), then per-subrow stores. The
    /// [lo, hi) window needs no [0, C) clip: `rows == 1` collapses the row
    /// loop, so the task range IS the window (split slices partition [0, C);
    /// the seq flavor is [0, C)). The sub-block remainder is the TI=1
    /// `emit_tile_j_split` discipline: constant-TJ main tiles, one runtime-`tj`
    /// tile. Per cell the fold chain stays k-ascending (the R1 invariant).
    fn emit_tiled_map_blocked_1d(&mut self, source: ObjectId, target: ObjectId, site: &TileSite) {
        let source_ty = self.obj_ty(source);
        let a_ty = source_ty
            .component_ty(site.a.slot)
            .cloned()
            .expect("tile a array");
        let b_ty = source_ty
            .component_ty(site.b.slot)
            .cloned()
            .expect("tile b array");
        let a_llt = lower_ty(&a_ty).expect("tile a lowers");
        let b_llt = lower_ty(&b_ty).expect("tile b lowers");
        let out_ty = self.obj_ty(target);
        let out_llt = lower_ty(&out_ty).expect("tile output lowers");
        let (_, n) = array_parts(&out_ty);
        debug_assert_eq!(n, site.rows * site.c);
        let elem_llt = lower_ty(&site.elem).expect("tile element lowers");
        let mul_op = if is_float(&site.elem) { "fmul" } else { "mul" };
        let add_op = if is_float(&site.elem) { "fadd" } else { "add" };
        let tile_j = self.profile.tile_j(&site.elem);

        let a_ptr = self
            .array_operand_ptr(source, Some(site.a.slot))
            .expect("tile a ptr");
        let b_ptr = self
            .array_operand_ptr(source, Some(site.b.slot))
            .expect("tile b ptr");
        let out_ptr = self.slot(target).expect("tile output slot");
        let acc_llt = format!("[{} x {elem_llt}]", WINDOW_SUBROWS * tile_j);
        let acc = self.scratch(&acc_llt);
        let j_ctr = self.scratch("i64");
        let k_ctr = self.scratch("i64");
        let lane_ctr = self.scratch("i64");
        let ctx = TileCtx {
            acc,
            acc_llt,
            elem_llt,
            seed: const_literal(&site.seed),
            mul_op,
            add_op,
            a_ptr,
            b_ptr,
            out_ptr,
            a_llt,
            b_llt,
            out_llt,
            k_ctr,
            lane_ctr,
            tile_j,
            // The window rung blocks LANES, not rows (see `WINDOW_SUBROWS`).
            tile_i: WINDOW_SUBROWS,
            tile_kc: self.profile.tile_kc(&site.elem),
            packed: None,
            contract_flag: if self.contract && is_float(&site.elem) {
                " contract"
            } else {
                ""
            },
        };
        let (lo, hi) = self.bulk_bounds(n);

        // Full blocks [jb, jb + TI·TJ) while jb + TI·TJ <= hi — never masked.
        let (blk_head, blk_body, blk_done) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {lo}, ptr {j_ctr}"));
        self.line(format!("br label %{blk_head}"));
        self.label_line(&blk_head);
        let jb = self.tmp();
        self.line(format!("{jb} = load i64, ptr {j_ctr}"));
        let jb_end = self.tmp();
        self.line(format!(
            "{jb_end} = add i64 {jb}, {}",
            WINDOW_SUBROWS * tile_j
        ));
        let block_fits = self.tmp();
        self.line(format!("{block_fits} = icmp ule i64 {jb_end}, {hi}"));
        self.line(format!(
            "br i1 {block_fits}, label %{blk_body}, label %{blk_done}"
        ));
        self.label_line(&blk_body);
        let a_base = (site.a.base != 0).then(|| site.a.base.to_string());
        self.emit_tile_window_block(site, &ctx, &jb, &a_base);
        self.line(format!("store i64 {jb_end}, ptr {j_ctr}"));
        self.line(format!("br label %{blk_head}"));
        self.label_line(&blk_done);

        // Remainder (< TI·TJ lanes): the TI=1 constant-TJ main + runtime-`tj`
        // split, entered with j_ctr at the block loop's exit value. The j
        // split's own checks make an empty remainder a no-op.
        let jw_rem_lo = self.tmp();
        self.line(format!("{jw_rem_lo} = load i64, ptr {j_ctr}"));
        let b_row = (site.b.base != 0).then(|| site.b.base.to_string());
        self.emit_tile_j_split(site, &ctx, &j_ctr, &jw_rem_lo, &hi, "0", &[a_base], &b_row);
    }

    /// One full TI·TJ block of the window nest at `jb`: subrow r's lanes live
    /// at acc offset r·TJ, so the seed and store lane loops are the trio's
    /// per-subrow discipline with the constant TJ bound. The k loop unrolls
    /// ×2 iff K is even (odd K keeps the plain single-k loop) in the trio's
    /// shape; per k, `emit_tile_window_step` shares ONE scalar `a` load
    /// across all TI subrows. Per cell the chain stays k-ascending.
    fn emit_tile_window_block(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        jb: &str,
        a_base: &Option<String>,
    ) {
        // Seed splat: one constant-TJ lane loop per subrow.
        for r in 0..ctx.tile_i {
            let (seed_head, seed_body, seed_done) = (self.label(), self.label(), self.label());
            self.line(format!("store i64 0, ptr {}", ctx.lane_ctr));
            self.line(format!("br label %{seed_head}"));
            self.label_line(&seed_head);
            let seed_lane = self.tmp();
            self.line(format!("{seed_lane} = load i64, ptr {}", ctx.lane_ctr));
            let seed_done_cond = self.tmp();
            self.line(format!(
                "{seed_done_cond} = icmp uge i64 {seed_lane}, {}",
                ctx.tile_j
            ));
            self.line(format!(
                "br i1 {seed_done_cond}, label %{seed_done}, label %{seed_body}"
            ));
            self.label_line(&seed_body);
            let acc_lane = if r == 0 {
                seed_lane.clone()
            } else {
                let offset = self.tmp();
                self.line(format!(
                    "{offset} = add i64 {seed_lane}, {}",
                    r * ctx.tile_j
                ));
                offset
            };
            let seed_ptr = self.tmp();
            self.line(format!(
                "{seed_ptr} = getelementptr {}, ptr {}, i64 0, i64 {acc_lane}",
                ctx.acc_llt, ctx.acc
            ));
            self.line(format!(
                "store {} {}, ptr {seed_ptr}",
                ctx.elem_llt, ctx.seed
            ));
            let seed_lane_next = self.tmp();
            self.line(format!("{seed_lane_next} = add i64 {seed_lane}, 1"));
            self.line(format!("store i64 {seed_lane_next}, ptr {}", ctx.lane_ctr));
            self.line(format!("br label %{seed_head}"));
            self.label_line(&seed_done);
        }

        let unroll = site.k % 2 == 0;
        let (k_head, k_body, k_done) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 0, ptr {}", ctx.k_ctr));
        self.line(format!("br label %{k_head}"));
        self.label_line(&k_head);
        let kk = self.tmp();
        self.line(format!("{kk} = load i64, ptr {}", ctx.k_ctr));
        if unroll {
            let (k_tail_check, k_tail) = (self.label(), self.label());
            let kk1 = self.tmp();
            self.line(format!("{kk1} = add i64 {kk}, 1"));
            let pair = self.tmp();
            self.line(format!("{pair} = icmp ult i64 {kk1}, {}", site.k));
            self.line(format!(
                "br i1 {pair}, label %{k_body}, label %{k_tail_check}"
            ));
            self.label_line(&k_body);
            self.emit_tile_window_step(site, ctx, jb, a_base, &kk);
            self.emit_tile_window_step(site, ctx, jb, a_base, &kk1);
            let kk2 = self.tmp();
            self.line(format!("{kk2} = add i64 {kk}, 2"));
            self.line(format!("store i64 {kk2}, ptr {}", ctx.k_ctr));
            self.line(format!("br label %{k_head}"));

            self.label_line(&k_tail_check);
            let tail = self.tmp();
            self.line(format!("{tail} = icmp ult i64 {kk}, {}", site.k));
            self.line(format!("br i1 {tail}, label %{k_tail}, label %{k_done}"));
            self.label_line(&k_tail);
            self.emit_tile_window_step(site, ctx, jb, a_base, &kk);
            self.line(format!("br label %{k_done}"));
        } else {
            let depth_done = self.tmp();
            self.line(format!("{depth_done} = icmp uge i64 {kk}, {}", site.k));
            self.line(format!(
                "br i1 {depth_done}, label %{k_done}, label %{k_body}"
            ));
            self.label_line(&k_body);
            self.emit_tile_window_step(site, ctx, jb, a_base, &kk);
            let kk_next = self.tmp();
            self.line(format!("{kk_next} = add i64 {kk}, 1"));
            self.line(format!("store i64 {kk_next}, ptr {}", ctx.k_ctr));
            self.line(format!("br label %{k_head}"));
        }
        self.label_line(&k_done);

        // Stores: one constant-TJ lane loop per subrow at out[jb + r·TJ + lane].
        for r in 0..ctx.tile_i {
            let out_base_r = if r == 0 {
                jb.to_owned()
            } else {
                let shifted = self.tmp();
                self.line(format!("{shifted} = add i64 {jb}, {}", r * ctx.tile_j));
                shifted
            };
            let (store_head, store_body, store_done) = (self.label(), self.label(), self.label());
            self.line(format!("store i64 0, ptr {}", ctx.lane_ctr));
            self.line(format!("br label %{store_head}"));
            self.label_line(&store_head);
            let store_lane = self.tmp();
            self.line(format!("{store_lane} = load i64, ptr {}", ctx.lane_ctr));
            let stores_done = self.tmp();
            self.line(format!(
                "{stores_done} = icmp uge i64 {store_lane}, {}",
                ctx.tile_j
            ));
            self.line(format!(
                "br i1 {stores_done}, label %{store_done}, label %{store_body}"
            ));
            self.label_line(&store_body);
            let acc_lane = if r == 0 {
                store_lane.clone()
            } else {
                let offset = self.tmp();
                self.line(format!(
                    "{offset} = add i64 {store_lane}, {}",
                    r * ctx.tile_j
                ));
                offset
            };
            let final_acc_ptr = self.tmp();
            self.line(format!(
                "{final_acc_ptr} = getelementptr {}, ptr {}, i64 0, i64 {acc_lane}",
                ctx.acc_llt, ctx.acc
            ));
            let final_value = self.tmp();
            self.line(format!(
                "{final_value} = load {}, ptr {final_acc_ptr}",
                ctx.elem_llt
            ));
            let out_index = self.tmp();
            self.line(format!("{out_index} = add i64 {out_base_r}, {store_lane}"));
            let out_elem_ptr = self.tmp();
            self.line(format!(
                "{out_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {out_index}",
                ctx.out_llt, ctx.out_ptr
            ));
            self.line(format!(
                "store {} {final_value}, ptr {out_elem_ptr}",
                ctx.elem_llt
            ));
            let store_lane_next = self.tmp();
            self.line(format!("{store_lane_next} = add i64 {store_lane}, 1"));
            self.line(format!("store i64 {store_lane_next}, ptr {}", ctx.lane_ctr));
            self.line(format!("br label %{store_head}"));
            self.label_line(&store_done);
        }
    }

    /// One k step of a full window block: ONE scalar `a` load
    /// (`a.base + a.ck·k`) shared across subrows; subrow r's constant-TJ lane
    /// loop FMAs `b[b.base + b.ck·k + jb + r·TJ + lane]` into
    /// `acc[r·TJ + lane]`, respecting the recorded operand orders.
    fn emit_tile_window_step(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        jb: &str,
        a_base: &Option<String>,
        kk: &str,
    ) {
        let a_index = self
            .emit_tile_index(a_base.clone(), &[(site.a.ck, kk)])
            .unwrap_or_else(|| "0".to_owned());
        let a_elem_ptr = self.tmp();
        self.line(format!(
            "{a_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {a_index}",
            ctx.a_llt, ctx.a_ptr
        ));
        let a_value = self.tmp();
        self.line(format!(
            "{a_value} = load {}, ptr {a_elem_ptr}",
            ctx.elem_llt
        ));
        for r in 0..ctx.tile_i {
            let b_base_r = site.b.base + r * ctx.tile_j;
            let b_start = self
                .emit_tile_index(
                    (b_base_r != 0).then(|| b_base_r.to_string()),
                    &[(site.b.ck, kk), (1, jb)],
                )
                .expect("window b has lane term");
            let (head, body, done) = (self.label(), self.label(), self.label());
            self.line(format!("store i64 0, ptr {}", ctx.lane_ctr));
            self.line(format!("br label %{head}"));
            self.label_line(&head);
            let lane = self.tmp();
            self.line(format!("{lane} = load i64, ptr {}", ctx.lane_ctr));
            let all_lanes = self.tmp();
            self.line(format!("{all_lanes} = icmp uge i64 {lane}, {}", ctx.tile_j));
            self.line(format!("br i1 {all_lanes}, label %{done}, label %{body}"));
            self.label_line(&body);
            let index = self.tmp();
            self.line(format!("{index} = add i64 {b_start}, {lane}"));
            let b_elem_ptr = self.tmp();
            self.line(format!(
                "{b_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {index}",
                ctx.b_llt, ctx.b_ptr
            ));
            let b_value = self.tmp();
            self.line(format!(
                "{b_value} = load {}, ptr {b_elem_ptr}",
                ctx.elem_llt
            ));
            let product = self.tmp();
            let (mul_lhs, mul_rhs) = if site.mul_a_first {
                (&a_value, &b_value)
            } else {
                (&b_value, &a_value)
            };
            self.line(format!(
                "{product} = {}{} {} {mul_lhs}, {mul_rhs}",
                ctx.mul_op, ctx.contract_flag, ctx.elem_llt
            ));
            let acc_lane = if r == 0 {
                lane.clone()
            } else {
                let offset = self.tmp();
                self.line(format!("{offset} = add i64 {lane}, {}", r * ctx.tile_j));
                offset
            };
            let acc_ptr = self.tmp();
            self.line(format!(
                "{acc_ptr} = getelementptr {}, ptr {}, i64 0, i64 {acc_lane}",
                ctx.acc_llt, ctx.acc
            ));
            let acc_value = self.tmp();
            self.line(format!(
                "{acc_value} = load {}, ptr {acc_ptr}",
                ctx.elem_llt
            ));
            let sum = self.tmp();
            let (add_lhs, add_rhs) = if site.add_acc_first {
                (&acc_value, &product)
            } else {
                (&product, &acc_value)
            };
            self.line(format!(
                "{sum} = {}{} {} {add_lhs}, {add_rhs}",
                ctx.add_op, ctx.contract_flag, ctx.elem_llt
            ));
            self.line(format!("store {} {sum}, ptr {acc_ptr}", ctx.elem_llt));
            let lane_next = self.tmp();
            self.line(format!("{lane_next} = add i64 {lane}, 1"));
            self.line(format!("store i64 {lane_next}, ptr {}", ctx.lane_ctr));
            self.line(format!("br label %{head}"));
            self.label_line(&done);
        }
    }

    /// The conv micro-kernel: cashes the k-split record. The fold's
    /// `(k÷div, k%div)` decomposition makes every tap offset compile-time —
    /// per (row `i`, j-tile) the (kq, kr) tap nest is fully unrolled (kq
    /// outer, kr inner IS k-ascending — the R1 invariant) and the body's
    /// div/mod vanish from the emission. Rows and j-tiles keep the rung-1
    /// idiom: the slice's row range with the signed per-row jw clip,
    /// constant-TJ main tiles, one runtime-`tj` remainder tile — never
    /// masked. TI=1 (row blocking is a recorded ceiling, not this rung).
    fn emit_tiled_map_conv(&mut self, source: ObjectId, target: ObjectId, site: &TileSite) {
        let source_ty = self.obj_ty(source);
        let a_ty = source_ty
            .component_ty(site.a.slot)
            .cloned()
            .expect("tile a array");
        let b_ty = source_ty
            .component_ty(site.b.slot)
            .cloned()
            .expect("tile b array");
        let a_llt = lower_ty(&a_ty).expect("tile a lowers");
        let b_llt = lower_ty(&b_ty).expect("tile b lowers");
        let out_ty = self.obj_ty(target);
        let out_llt = lower_ty(&out_ty).expect("tile output lowers");
        let (_, n) = array_parts(&out_ty);
        debug_assert_eq!(n, site.rows * site.c);
        let elem_llt = lower_ty(&site.elem).expect("tile element lowers");
        let tile_j = self.profile.tile_j(&site.elem);

        let a_ptr = self
            .array_operand_ptr(source, Some(site.a.slot))
            .expect("tile a ptr");
        let b_ptr = self
            .array_operand_ptr(source, Some(site.b.slot))
            .expect("tile b ptr");
        let out_ptr = self.slot(target).expect("tile output slot");
        let acc_llt = format!("[{tile_j} x {elem_llt}]");
        let acc = self.scratch(&acc_llt);
        let i_ctr = self.scratch("i64");
        let j_ctr = self.scratch("i64");
        let lane_ctr = self.scratch("i64");
        let ctx = ConvTileCtx {
            tile_j,
            acc,
            acc_llt,
            elem_llt,
            seed: const_literal(&site.seed),
            mul_op: if is_float(&site.elem) { "fmul" } else { "mul" },
            add_op: if is_float(&site.elem) { "fadd" } else { "add" },
            a_ptr,
            b_ptr,
            out_ptr,
            a_llt,
            b_llt,
            out_llt,
            lane_ctr,
            contract_flag: if self.contract && is_float(&site.elem) {
                " contract"
            } else {
                ""
            },
        };
        let (lo, hi) = self.bulk_bounds(n);

        let i_lo = self.tmp();
        self.line(format!("{i_lo} = udiv i64 {lo}, {}", site.c));
        let hi_biased = self.tmp();
        self.line(format!("{hi_biased} = add i64 {hi}, {}", site.c - 1));
        let i_hi = self.tmp();
        self.line(format!("{i_hi} = udiv i64 {hi_biased}, {}", site.c));
        self.line(format!("store i64 {i_lo}, ptr {i_ctr}"));

        // plan-s31-deduced-blocking item 4. Row blocking is applied because the
        // RECORD says this read slides — `i_reuse(b) == Sliding{q}` — not
        // because the site is conv2d; it is the same predicate the matmul rung
        // uses at q = 0 (`Invariant`). Interior full-window rows run TI at a
        // time; head and tail rows keep the TI=1 path, the rung-2 i split.
        let ti = self.profile.tile_i();
        let sliding = match crate::reuse::i_reuse(site, &site.b) {
            crate::reuse::Reuse::Sliding { q } if site.rows > 1 && ti > 1 => Some(q),
            _ => None,
        };
        if let Some(q) = sliding {
            // Interior rows are [ceil(lo/C), floor(hi/C)): the rows whose whole
            // lane window [0, C) lies inside the task range, so no jw clip.
            let lo_biased = self.tmp();
            self.line(format!("{lo_biased} = add i64 {lo}, {}", site.c - 1));
            let i_fw_lo = self.tmp();
            self.line(format!("{i_fw_lo} = udiv i64 {lo_biased}, {}", site.c));
            let i_fw_hi = self.tmp();
            self.line(format!("{i_fw_hi} = udiv i64 {hi}, {}", site.c));
            let fw_past_end = self.tmp();
            self.line(format!("{fw_past_end} = icmp ugt i64 {i_fw_lo}, {i_hi}"));
            let head_end = self.tmp();
            self.line(format!(
                "{head_end} = select i1 {fw_past_end}, i64 {i_hi}, i64 {i_fw_lo}"
            ));
            // One counter through all three regions: each loop resumes where
            // the previous left it, so no region can skip or repeat a row.
            self.emit_conv_row_range(site, &ctx, &i_ctr, &j_ctr, &head_end, &lo, &hi);
            self.emit_conv_blocked_range(site, &ctx, &i_ctr, &j_ctr, &i_fw_hi, ti, q);
            self.emit_conv_row_range(site, &ctx, &i_ctr, &j_ctr, &i_hi, &lo, &hi);
            return;
        }

        self.emit_conv_row_range(site, &ctx, &i_ctr, &j_ctr, &i_hi, &lo, &hi);
    }

    /// The TI=1 conv row loop over `[*i_ctr, to)`, leaving `i_ctr` at `to` —
    /// the S28 body verbatim. The head and tail regions of a blocked nest and
    /// the whole nest of an unblocked site are the same code.
    #[allow(clippy::too_many_arguments)]
    fn emit_conv_row_range(
        &mut self,
        site: &TileSite,
        ctx: &ConvTileCtx,
        i_ctr: &str,
        j_ctr: &str,
        to: &str,
        lo: &str,
        hi: &str,
    ) {
        let tile_j = ctx.tile_j;
        let i_hi = to;

        let (i_head, i_body, i_done) = (self.label(), self.label(), self.label());
        self.line(format!("br label %{i_head}"));
        self.label_line(&i_head);
        let i = self.tmp();
        self.line(format!("{i} = load i64, ptr {i_ctr}"));
        let rows_done = self.tmp();
        self.line(format!("{rows_done} = icmp uge i64 {i}, {i_hi}"));
        self.line(format!(
            "br i1 {rows_done}, label %{i_done}, label %{i_body}"
        ));

        self.label_line(&i_body);
        let row0 = self.tmp();
        self.line(format!("{row0} = mul i64 {i}, {}", site.c));
        let jw_lo_raw = self.tmp();
        self.line(format!("{jw_lo_raw} = sub i64 {lo}, {row0}"));
        let jw_lo_negative = self.tmp();
        self.line(format!("{jw_lo_negative} = icmp slt i64 {jw_lo_raw}, 0"));
        let jw_lo = self.tmp();
        self.line(format!(
            "{jw_lo} = select i1 {jw_lo_negative}, i64 0, i64 {jw_lo_raw}"
        ));
        let jw_hi_raw = self.tmp();
        self.line(format!("{jw_hi_raw} = sub i64 {hi}, {row0}"));
        let jw_hi_past_c = self.tmp();
        self.line(format!(
            "{jw_hi_past_c} = icmp sgt i64 {jw_hi_raw}, {}",
            site.c
        ));
        let jw_hi = self.tmp();
        self.line(format!(
            "{jw_hi} = select i1 {jw_hi_past_c}, i64 {}, i64 {jw_hi_raw}",
            site.c
        ));
        let a_row = self.emit_tile_index(
            (site.a.base != 0).then(|| site.a.base.to_string()),
            &[(site.a.ci, i.as_str())],
        );
        let b_row = self.emit_tile_index(
            (site.b.base != 0).then(|| site.b.base.to_string()),
            &[(site.b.ci, i.as_str())],
        );

        // The fixed-TJ j split: constant-TJ main tiles while
        // `j0 + TILE_J <= jw_hi`, then one runtime-`tj` remainder tile,
        // entered only when lanes remain (the emit_tile_j_split discipline).
        let (j_head, j_main, j_rem_check, j_rem, j_done) = (
            self.label(),
            self.label(),
            self.label(),
            self.label(),
            self.label(),
        );
        self.line(format!("store i64 {jw_lo}, ptr {j_ctr}"));
        self.line(format!("br label %{j_head}"));
        self.label_line(&j_head);
        let j0 = self.tmp();
        self.line(format!("{j0} = load i64, ptr {j_ctr}"));
        let j0_full = self.tmp();
        self.line(format!("{j0_full} = add i64 {j0}, {tile_j}"));
        let full_tile = self.tmp();
        self.line(format!("{full_tile} = icmp ule i64 {j0_full}, {jw_hi}"));
        self.line(format!(
            "br i1 {full_tile}, label %{j_main}, label %{j_rem_check}"
        ));
        self.label_line(&j_main);
        let lane_full = tile_j.to_string();
        self.emit_tile_conv_tile(site, &ctx, &j0, &row0, &a_row, &b_row, &lane_full);
        let j0_next = self.tmp();
        self.line(format!("{j0_next} = add i64 {j0}, {tile_j}"));
        self.line(format!("store i64 {j0_next}, ptr {j_ctr}"));
        self.line(format!("br label %{j_head}"));
        self.label_line(&j_rem_check);
        let rem_exists = self.tmp();
        self.line(format!("{rem_exists} = icmp ult i64 {j0}, {jw_hi}"));
        self.line(format!(
            "br i1 {rem_exists}, label %{j_rem}, label %{j_done}"
        ));
        self.label_line(&j_rem);
        let remaining = self.tmp();
        self.line(format!("{remaining} = sub i64 {jw_hi}, {j0}"));
        let partial = self.tmp();
        self.line(format!("{partial} = icmp ult i64 {remaining}, {tile_j}"));
        let tj = self.tmp();
        self.line(format!(
            "{tj} = select i1 {partial}, i64 {remaining}, i64 {tile_j}"
        ));
        self.emit_tile_conv_tile(site, &ctx, &j0, &row0, &a_row, &b_row, &tj);
        self.line(format!("br label %{j_done}"));
        self.label_line(&j_done);

        let i_next = self.tmp();
        self.line(format!("{i_next} = add i64 {i}, 1"));
        self.line(format!("store i64 {i_next}, ptr {i_ctr}"));
        self.line(format!("br label %{i_head}"));
        self.label_line(&i_done);
    }

    /// One j-tile body of the conv nest at (i, j0): seed splat, the fully
    /// unrolled (kq, kr) tap nest, stores. `bound` is the literal TJ on the
    /// main path, the runtime `tj` on the remainder tile. Per tap
    /// `k_tap = kq·div + kr` (compile-time): ONE `a` load at
    /// `a.base + a.ci·i + a.ck·k_tap` (a constant index for conv's broadcast
    /// w, hoisted per row in general), then one lane loop reading
    /// `b[b_row + (cq·kq + cr·kr) + j0 + lane]` — the parenthesized tap
    /// offset folds to a compile-time constant — FMA into the acc vector,
    /// respecting the recorded operand orders.
    /// The region's slice sizing (plan-s32 step 2): a **floor** on slice size
    /// and a per-lane over-decomposition factor. Both are compile-time facts;
    /// the lane count is deliberately not one, so the runtime supplies it.
    ///
    /// **The floor is a coherence constraint, not a preference.** A slice
    /// holding fewer than `TI` output rows cannot run the register-blocked
    /// kernel at all — every piece falls onto the TI=1 fallback. Measured cost
    /// of crossing it: matmul1024 2.45 ms → 17.97 ms at 2 rows per slice, and
    /// matmul512 0.436 → 2.41. This is the granularity nest being coupled: the
    /// slice must contain at least the block the tile rung is built from.
    ///
    /// **The factor comes from the reuse structure**, the same `i_reuse` that
    /// drives row blocking one level down. A row-invariant read (`ci == 0`,
    /// matmul's `b`) pays nothing at a slice boundary, so over-decomposing is
    /// free and gives work stealing something to steal — without it a dispatch
    /// is one piece per lane and a fast lane cannot help a slow one. A sliding
    /// read (conv2d's `b`) re-pays its window overlap at every boundary, so it
    /// keeps one piece per lane. Measured, sweeping slice size at 14 lanes:
    /// matmul512 0.750 → 0.429 and matmul1024 3.627 → 2.452 with
    /// over-decomposition, while conv2d degrades monotonically with it.
    fn slice_sizing(&self, site: &TileSite) -> (u64, u32) {
        if site.rows <= 1 || site.c == 0 {
            return (0, 0);
        }
        let floor = self.profile.tile_i().saturating_mul(site.c);
        // OVER-DECOMPOSITION IS NOT SHIPPED YET, and the reason is recorded
        // rather than hidden. Forcing slice size directly with the MAPAL_SLICE
        // lever, over-decomposing an `Invariant` site is worth 1.46-1.78x
        // (matmul512 0.750 -> 0.429, matmul1024 3.627 -> 2.452 at 14 lanes).
        // Routing the SAME slice counts through this deduction instead made
        // matmul1024 34% WORSE (3.58 -> 4.80) while matmul512 gained only 10%.
        // The difference is not the count, so it is something about the nested
        // dispatch a packed site performs — the outer task packs and then opens
        // its own run, and that path is not the one the lever exercised.
        // Until that is explained, `1` reproduces today's slicing exactly and
        // the floor below is the only behaviour change (plan-s32 §2.6).
        let oversub = match crate::reuse::i_reuse(site, &site.b) {
            crate::reuse::Reuse::Invariant => 4,
            _ => 1,
        };
        (floor, oversub)
    }

    /// TI interior rows at a time (plan-s31-deduced-blocking item 4). Entered
    /// only where the record says the sliding read shares across rows, and only
    /// for full-window rows, so there is no per-row jw clip inside.
    ///
    /// Leaves `i_ctr` at the first row it did not take, so the tail range
    /// resumes from it.
    #[allow(clippy::too_many_arguments)]
    fn emit_conv_blocked_range(
        &mut self,
        site: &TileSite,
        ctx: &ConvTileCtx,
        i_ctr: &str,
        j_ctr: &str,
        to: &str,
        ti: u64,
        q: u64,
    ) {
        let tile_j = ctx.tile_j;
        let (head, body, done) = (self.label(), self.label(), self.label());
        self.line(format!("br label %{head}"));
        self.label_line(&head);
        let i = self.tmp();
        self.line(format!("{i} = load i64, ptr {i_ctr}"));
        let i_end = self.tmp();
        self.line(format!("{i_end} = add i64 {i}, {ti}"));
        let fits = self.tmp();
        self.line(format!("{fits} = icmp ule i64 {i_end}, {to}"));
        self.line(format!("br i1 {fits}, label %{body}, label %{done}"));

        self.label_line(&body);
        let row0 = self.tmp();
        self.line(format!("{row0} = mul i64 {i}, {}", site.c));
        // Per-row read bases: row i+r sits `ci·r` past the block's own.
        let mut a_rows = Vec::with_capacity(ti as usize);
        let mut b_rows = Vec::with_capacity(ti as usize);
        for (coeff, base, rows) in [
            (site.a.ci, site.a.base, &mut a_rows),
            (site.b.ci, site.b.base, &mut b_rows),
        ] {
            let block = self.emit_tile_index(
                (base != 0).then(|| base.to_string()),
                &[(coeff, i.as_str())],
            );
            for r in 0..ti {
                let off = coeff * r;
                if off == 0 {
                    rows.push(block.clone());
                } else {
                    let prev = block.clone().unwrap_or_else(|| "0".to_owned());
                    let shifted = self.tmp();
                    self.line(format!("{shifted} = add i64 {prev}, {off}"));
                    rows.push(Some(shifted));
                }
            }
        }

        // Constant-TJ main tiles across the full window [0, C).
        let (j_head, j_body, j_done) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 0, ptr {j_ctr}"));
        self.line(format!("br label %{j_head}"));
        self.label_line(&j_head);
        let j0 = self.tmp();
        self.line(format!("{j0} = load i64, ptr {j_ctr}"));
        let j0_full = self.tmp();
        self.line(format!("{j0_full} = add i64 {j0}, {tile_j}"));
        let full = self.tmp();
        self.line(format!("{full} = icmp ule i64 {j0_full}, {}", site.c));
        self.line(format!("br i1 {full}, label %{j_body}, label %{j_done}"));
        self.label_line(&j_body);
        self.emit_conv_block_tile(site, ctx, &j0, &row0, &a_rows, &b_rows[0], ti, q);
        let j0_next = self.tmp();
        self.line(format!("{j0_next} = add i64 {j0}, {tile_j}"));
        self.line(format!("store i64 {j0_next}, ptr {j_ctr}"));
        self.line(format!("br label %{j_head}"));
        self.label_line(&j_done);

        // Remainder lanes (< TJ): TI separate TI=1 tiles on the scalar path —
        // blocking buys nothing on a partial tile and the shared code is the
        // negative control.
        let j_rem = self.tmp();
        self.line(format!("{j_rem} = load i64, ptr {j_ctr}"));
        let has_rem = self.tmp();
        self.line(format!("{has_rem} = icmp ult i64 {j_rem}, {}", site.c));
        let (rem_body, rem_done) = (self.label(), self.label());
        self.line(format!(
            "br i1 {has_rem}, label %{rem_body}, label %{rem_done}"
        ));
        self.label_line(&rem_body);
        let rem_len = self.tmp();
        self.line(format!("{rem_len} = sub i64 {}, {j_rem}", site.c));
        for r in 0..ti {
            let row0_r = if r == 0 {
                row0.clone()
            } else {
                let t = self.tmp();
                self.line(format!("{t} = add i64 {row0}, {}", r * site.c));
                t
            };
            let (a_r, b_r) = (a_rows[r as usize].clone(), b_rows[r as usize].clone());
            self.emit_tile_conv_tile(site, ctx, &j_rem, &row0_r, &a_r, &b_r, &rem_len);
        }
        self.line(format!("br label %{rem_done}"));
        self.label_line(&rem_done);

        self.line(format!("store i64 {i_end}, ptr {i_ctr}"));
        self.line(format!("br label %{head}"));
        self.label_line(&done);
    }

    /// One TI×TJ block of the conv nest: TI `<TJ x elem>` accumulators, and the
    /// taps **hoisted once per block** rather than re-emitted per row.
    ///
    /// The union of tap-rows a block touches is `(TI−1)·q + k/div` — six image
    /// rows for four output rows at `q = 1`, `k/div = 3` — against `TI · k/div`
    /// = twelve unblocked. Each is loaded ONCE, into one vector register, and
    /// consumed by every row that uses it. Emitting TI copies of the tap nest
    /// instead would put the matching loads in different basic blocks separated
    /// by aliasing stores, which is the GVN situation S29 recorded failing;
    /// this is plan composition rule 4, and it is why the loop nests row
    /// INSIDE tap rather than outside.
    ///
    /// R1 holds: for a fixed row `r`, `kq = kqp − q·r` rises with `kqp` and
    /// `kr` rises within it, so the per-cell chain is still k-ascending, with
    /// the recorded operand orders untouched.
    #[allow(clippy::too_many_arguments)]
    fn emit_conv_block_tile(
        &mut self,
        site: &TileSite,
        ctx: &ConvTileCtx,
        j0: &str,
        row0: &str,
        a_rows: &[Option<String>],
        b_row: &Option<String>,
        ti: u64,
        q: u64,
    ) {
        let vllt = vec_llt(&ctx.elem_llt, ctx.tile_j);
        let align = llt_align(&ctx.elem_llt);
        let ks = *site.b.ksplit.as_ref().expect("conv site records ksplit");
        let kq_rows = site.k / ks.div;
        // The emitter and the reuse query must agree on how many distinct
        // tap-rows this block touches; if they ever diverge, one of them is
        // wrong about what blocking buys.
        debug_assert_eq!(
            (ti - 1) * q + kq_rows,
            crate::reuse::distinct_runs(site, &site.b, ti),
            "block tap-row union must match the deduced reuse"
        );

        let b_tile = self
            .emit_tile_index(b_row.clone(), &[(1, j0)])
            .expect("conv b has lane term");
        let seed = ctx.seed.clone();
        let mut accs = Vec::with_capacity(ti as usize);
        for _ in 0..ti {
            let acc = self.emit_splat(&ctx.elem_llt, ctx.tile_j, &seed);
            accs.push(acc);
        }

        for kqp in 0..((ti - 1) * q + kq_rows) {
            for kr in 0..ks.div {
                let users: Vec<u64> = (0..ti)
                    .filter(|r| kqp >= q * r && kqp - q * r < kq_rows)
                    .collect();
                if users.is_empty() {
                    continue;
                }
                let tap_off = ks.cq * kqp + ks.cr * kr;
                let b_start = if tap_off == 0 {
                    b_tile.clone()
                } else {
                    let shifted = self.tmp();
                    self.line(format!("{shifted} = add i64 {b_tile}, {tap_off}"));
                    shifted
                };
                let b_elem_ptr = self.tmp();
                self.line(format!(
                    "{b_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {b_start}",
                    ctx.b_llt, ctx.b_ptr
                ));
                let b_vec = self.tmp();
                self.line(format!(
                    "{b_vec} = load {vllt}, ptr {b_elem_ptr}, align {align}"
                ));

                for r in users {
                    let k_tap = ((kqp - q * r) * ks.div + kr).to_string();
                    let a_index = self
                        .emit_tile_index(a_rows[r as usize].clone(), &[(site.a.ck, k_tap.as_str())])
                        .unwrap_or_else(|| "0".to_owned());
                    let a_elem_ptr = self.tmp();
                    self.line(format!(
                        "{a_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {a_index}",
                        ctx.a_llt, ctx.a_ptr
                    ));
                    let a_value = self.tmp();
                    self.line(format!(
                        "{a_value} = load {}, ptr {a_elem_ptr}",
                        ctx.elem_llt
                    ));
                    let a_vec = self.emit_splat(&ctx.elem_llt, ctx.tile_j, &a_value);
                    let product = self.tmp();
                    let (mul_lhs, mul_rhs) = if site.mul_a_first {
                        (a_vec.clone(), b_vec.clone())
                    } else {
                        (b_vec.clone(), a_vec.clone())
                    };
                    self.line(format!(
                        "{product} = {}{} {vllt} {mul_lhs}, {mul_rhs}",
                        ctx.mul_op, ctx.contract_flag
                    ));
                    let sum = self.tmp();
                    let acc = accs[r as usize].clone();
                    let (add_lhs, add_rhs) = if site.add_acc_first {
                        (acc, product.clone())
                    } else {
                        (product.clone(), acc)
                    };
                    self.line(format!(
                        "{sum} = {}{} {vllt} {add_lhs}, {add_rhs}",
                        ctx.add_op, ctx.contract_flag
                    ));
                    accs[r as usize] = sum;
                }
            }
        }

        for r in 0..ti {
            let out_start = self.tmp();
            self.line(format!("{out_start} = add i64 {row0}, {}", r * site.c));
            let out_index = self.tmp();
            self.line(format!("{out_index} = add i64 {out_start}, {j0}"));
            let out_elem_ptr = self.tmp();
            self.line(format!(
                "{out_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {out_index}",
                ctx.out_llt, ctx.out_ptr
            ));
            self.line(format!(
                "store {vllt} {}, ptr {out_elem_ptr}, align {align}",
                accs[r as usize]
            ));
        }
    }

    /// The constant-TJ main tile as `<TJ x elem>` SSA values — plan-s31-
    /// deduced-blocking work item 2, the S30 accumulator carve-out applied to
    /// the conv rung.
    ///
    /// Conv has **no runtime k loop** (the `(kq, kr)` taps are unrolled at
    /// emission), so the accumulator needs no `phi` at all: it is a straight
    /// chain of SSA values, one `fadd` per tap. That removes what the memory
    /// form spends per (tap, lane) — a `getelementptr`, a `load` and a `store`
    /// of accumulator state — plus the whole seed and store lane loops, leaving
    /// one splat, one vector load per tap, and one vector store.
    ///
    /// Bit-exact against the scalar form by the same argument as S30: SIMD
    /// lanes are independent, so lane j of the result is exactly the scalar
    /// chain's value for lane j, and the tap order and both recorded operand
    /// orders are preserved. Alignment is the ELEMENT's, never the vector
    /// type's ABI alignment — `j0` is arbitrary (S30 composition rule 3).
    ///
    /// The remainder tile (runtime `tj`) and every boundary row keep the memory
    /// form, exactly as the matmul rung's carve-out does.
    fn emit_tile_conv_tile_vec(
        &mut self,
        site: &TileSite,
        ctx: &ConvTileCtx,
        j0: &str,
        row0: &str,
        a_row: &Option<String>,
        b_row: &Option<String>,
    ) {
        let vllt = vec_llt(&ctx.elem_llt, ctx.tile_j);
        let align = llt_align(&ctx.elem_llt);
        let ksplit = site.b.ksplit.as_ref().expect("conv site records ksplit");
        debug_assert_eq!(site.k % ksplit.div, 0, "rectangular window (rule 2)");

        // Hoist b_row + j0 once; each tap adds its compile-time offset.
        let b_tile = self
            .emit_tile_index(b_row.clone(), &[(1, j0)])
            .expect("conv b has lane term");
        let seed = ctx.seed.clone();
        let mut acc = self.emit_splat(&ctx.elem_llt, ctx.tile_j, &seed);

        for kq in 0..(site.k / ksplit.div) {
            for kr in 0..ksplit.div {
                let k_tap = (kq * ksplit.div + kr).to_string();
                let a_index = self
                    .emit_tile_index(a_row.clone(), &[(site.a.ck, k_tap.as_str())])
                    .unwrap_or_else(|| "0".to_owned());
                let a_elem_ptr = self.tmp();
                self.line(format!(
                    "{a_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {a_index}",
                    ctx.a_llt, ctx.a_ptr
                ));
                let a_value = self.tmp();
                self.line(format!(
                    "{a_value} = load {}, ptr {a_elem_ptr}",
                    ctx.elem_llt
                ));
                let a_vec = self.emit_splat(&ctx.elem_llt, ctx.tile_j, &a_value);

                let tap_off = ksplit.cq * kq + ksplit.cr * kr;
                let b_start = if tap_off == 0 {
                    b_tile.clone()
                } else {
                    let shifted = self.tmp();
                    self.line(format!("{shifted} = add i64 {b_tile}, {tap_off}"));
                    shifted
                };
                let b_elem_ptr = self.tmp();
                self.line(format!(
                    "{b_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {b_start}",
                    ctx.b_llt, ctx.b_ptr
                ));
                let b_vec = self.tmp();
                self.line(format!(
                    "{b_vec} = load {vllt}, ptr {b_elem_ptr}, align {align}"
                ));

                let product = self.tmp();
                let (mul_lhs, mul_rhs) = if site.mul_a_first {
                    (a_vec.clone(), b_vec.clone())
                } else {
                    (b_vec.clone(), a_vec.clone())
                };
                self.line(format!(
                    "{product} = {}{} {vllt} {mul_lhs}, {mul_rhs}",
                    ctx.mul_op, ctx.contract_flag
                ));
                let sum = self.tmp();
                let (add_lhs, add_rhs) = if site.add_acc_first {
                    (acc.clone(), product.clone())
                } else {
                    (product.clone(), acc.clone())
                };
                self.line(format!(
                    "{sum} = {}{} {vllt} {add_lhs}, {add_rhs}",
                    ctx.add_op, ctx.contract_flag
                ));
                acc = sum;
            }
        }

        // One contiguous vector store: out[row0 + j0 .. + TJ).
        let out_start = self.tmp();
        self.line(format!("{out_start} = add i64 {row0}, {j0}"));
        let out_elem_ptr = self.tmp();
        self.line(format!(
            "{out_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {out_start}",
            ctx.out_llt, ctx.out_ptr
        ));
        self.line(format!(
            "store {vllt} {acc}, ptr {out_elem_ptr}, align {align}"
        ));
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_tile_conv_tile(
        &mut self,
        site: &TileSite,
        ctx: &ConvTileCtx,
        j0: &str,
        row0: &str,
        a_row: &Option<String>,
        b_row: &Option<String>,
        bound: &str,
    ) {
        // plan-s31 work item 2: the constant-TJ main tile runs on SSA vector
        // values (no accumulator memory at all); the runtime-`tj` remainder
        // keeps the form below — the S30 carve-out, same shape.
        if bound.parse::<u64>().ok() == Some(ctx.tile_j) {
            self.emit_tile_conv_tile_vec(site, ctx, j0, row0, a_row, b_row);
            return;
        }

        // Seed splat: acc[lane] = seed over [0, bound).
        let (seed_head, seed_body, seed_done) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 0, ptr {}", ctx.lane_ctr));
        self.line(format!("br label %{seed_head}"));
        self.label_line(&seed_head);
        let seed_lane = self.tmp();
        self.line(format!("{seed_lane} = load i64, ptr {}", ctx.lane_ctr));
        let seed_done_cond = self.tmp();
        self.line(format!(
            "{seed_done_cond} = icmp uge i64 {seed_lane}, {bound}"
        ));
        self.line(format!(
            "br i1 {seed_done_cond}, label %{seed_done}, label %{seed_body}"
        ));
        self.label_line(&seed_body);
        let seed_ptr = self.tmp();
        self.line(format!(
            "{seed_ptr} = getelementptr {}, ptr {}, i64 0, i64 {seed_lane}",
            ctx.acc_llt, ctx.acc
        ));
        self.line(format!(
            "store {} {}, ptr {seed_ptr}",
            ctx.elem_llt, ctx.seed
        ));
        let seed_lane_next = self.tmp();
        self.line(format!("{seed_lane_next} = add i64 {seed_lane}, 1"));
        self.line(format!("store i64 {seed_lane_next}, ptr {}", ctx.lane_ctr));
        self.line(format!("br label %{seed_head}"));
        self.label_line(&seed_done);

        let ksplit = site.b.ksplit.as_ref().expect("conv site records ksplit");
        debug_assert_eq!(site.k % ksplit.div, 0, "rectangular window (rule 2)");
        // Hoist b_row + j0 once per tile; each tap adds its compile-time
        // offset (`cq·kq + cr·kr`) — the div/mod pair constant-folds.
        let b_tile = self
            .emit_tile_index(b_row.clone(), &[(1, j0)])
            .expect("conv b has lane term");
        for kq in 0..(site.k / ksplit.div) {
            for kr in 0..ksplit.div {
                let k_tap = (kq * ksplit.div + kr).to_string();
                let a_index = self
                    .emit_tile_index(a_row.clone(), &[(site.a.ck, k_tap.as_str())])
                    .unwrap_or_else(|| "0".to_owned());
                let a_elem_ptr = self.tmp();
                self.line(format!(
                    "{a_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {a_index}",
                    ctx.a_llt, ctx.a_ptr
                ));
                let a_value = self.tmp();
                self.line(format!(
                    "{a_value} = load {}, ptr {a_elem_ptr}",
                    ctx.elem_llt
                ));
                let tap_off = ksplit.cq * kq + ksplit.cr * kr;
                let b_start = if tap_off == 0 {
                    b_tile.clone()
                } else {
                    let shifted = self.tmp();
                    self.line(format!("{shifted} = add i64 {b_tile}, {tap_off}"));
                    shifted
                };
                let (head, body, done) = (self.label(), self.label(), self.label());
                self.line(format!("store i64 0, ptr {}", ctx.lane_ctr));
                self.line(format!("br label %{head}"));
                self.label_line(&head);
                let lane = self.tmp();
                self.line(format!("{lane} = load i64, ptr {}", ctx.lane_ctr));
                let all_lanes = self.tmp();
                self.line(format!("{all_lanes} = icmp uge i64 {lane}, {bound}"));
                self.line(format!("br i1 {all_lanes}, label %{done}, label %{body}"));
                self.label_line(&body);
                let index = self.tmp();
                self.line(format!("{index} = add i64 {b_start}, {lane}"));
                let b_elem_ptr = self.tmp();
                self.line(format!(
                    "{b_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {index}",
                    ctx.b_llt, ctx.b_ptr
                ));
                let b_value = self.tmp();
                self.line(format!(
                    "{b_value} = load {}, ptr {b_elem_ptr}",
                    ctx.elem_llt
                ));
                let product = self.tmp();
                let (mul_lhs, mul_rhs) = if site.mul_a_first {
                    (&a_value, &b_value)
                } else {
                    (&b_value, &a_value)
                };
                self.line(format!(
                    "{product} = {}{} {} {mul_lhs}, {mul_rhs}",
                    ctx.mul_op, ctx.contract_flag, ctx.elem_llt
                ));
                let acc_ptr = self.tmp();
                self.line(format!(
                    "{acc_ptr} = getelementptr {}, ptr {}, i64 0, i64 {lane}",
                    ctx.acc_llt, ctx.acc
                ));
                let acc_value = self.tmp();
                self.line(format!(
                    "{acc_value} = load {}, ptr {acc_ptr}",
                    ctx.elem_llt
                ));
                let sum = self.tmp();
                let (add_lhs, add_rhs) = if site.add_acc_first {
                    (&acc_value, &product)
                } else {
                    (&product, &acc_value)
                };
                self.line(format!(
                    "{sum} = {}{} {} {add_lhs}, {add_rhs}",
                    ctx.add_op, ctx.contract_flag, ctx.elem_llt
                ));
                self.line(format!("store {} {sum}, ptr {acc_ptr}", ctx.elem_llt));
                let lane_next = self.tmp();
                self.line(format!("{lane_next} = add i64 {lane}, 1"));
                self.line(format!("store i64 {lane_next}, ptr {}", ctx.lane_ctr));
                self.line(format!("br label %{head}"));
                self.label_line(&done);
            }
        }

        // Stores: out[row0 + j0 + lane] = acc[lane] over [0, bound).
        let out_start = self.tmp();
        self.line(format!("{out_start} = add i64 {row0}, {j0}"));
        let (store_head, store_body, store_done) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 0, ptr {}", ctx.lane_ctr));
        self.line(format!("br label %{store_head}"));
        self.label_line(&store_head);
        let store_lane = self.tmp();
        self.line(format!("{store_lane} = load i64, ptr {}", ctx.lane_ctr));
        let stores_done = self.tmp();
        self.line(format!(
            "{stores_done} = icmp uge i64 {store_lane}, {bound}"
        ));
        self.line(format!(
            "br i1 {stores_done}, label %{store_done}, label %{store_body}"
        ));
        self.label_line(&store_body);
        let final_acc_ptr = self.tmp();
        self.line(format!(
            "{final_acc_ptr} = getelementptr {}, ptr {}, i64 0, i64 {store_lane}",
            ctx.acc_llt, ctx.acc
        ));
        let final_value = self.tmp();
        self.line(format!(
            "{final_value} = load {}, ptr {final_acc_ptr}",
            ctx.elem_llt
        ));
        let out_index = self.tmp();
        self.line(format!("{out_index} = add i64 {out_start}, {store_lane}"));
        let out_elem_ptr = self.tmp();
        self.line(format!(
            "{out_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {out_index}",
            ctx.out_llt, ctx.out_ptr
        ));
        self.line(format!(
            "store {} {final_value}, ptr {out_elem_ptr}",
            ctx.elem_llt
        ));
        let store_lane_next = self.tmp();
        self.line(format!("{store_lane_next} = add i64 {store_lane}, 1"));
        self.line(format!("store i64 {store_lane_next}, ptr {}", ctx.lane_ctr));
        self.line(format!("br label %{store_head}"));
        self.label_line(&store_done);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_tile_packed_j_outer(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        i_ctr: &str,
        j_ctr: &str,
        lo: &str,
        hi: &str,
        i_lo: &str,
        i_hi: &str,
        i_fw_lo: &str,
        i_fw_hi: &str,
    ) {
        let (j_head, j_main, j_rem_check, j_rem, j_done) = (
            self.label(),
            self.label(),
            self.label(),
            self.label(),
            self.label(),
        );
        self.line(format!("store i64 0, ptr {j_ctr}"));
        self.line(format!("br label %{j_head}"));
        self.label_line(&j_head);
        let j0 = self.tmp();
        self.line(format!("{j0} = load i64, ptr {j_ctr}"));
        let j0_full = self.tmp();
        self.line(format!("{j0_full} = add i64 {j0}, {}", ctx.tile_j));
        let full_tile = self.tmp();
        self.line(format!("{full_tile} = icmp ule i64 {j0_full}, {}", site.c));
        self.line(format!(
            "br i1 {full_tile}, label %{j_main}, label %{j_rem_check}"
        ));

        self.label_line(&j_main);
        let panel_base = self.emit_tile_panel_base(site, ctx, &j0);
        let lane_full = ctx.tile_j.to_string();
        self.emit_tile_i_regions(
            site,
            ctx,
            i_ctr,
            j_ctr,
            lo,
            hi,
            i_lo,
            i_hi,
            i_fw_lo,
            i_fw_hi,
            Some((&j0, &lane_full, true, &panel_base)),
        );
        let j0_next = self.tmp();
        self.line(format!("{j0_next} = add i64 {j0}, {}", ctx.tile_j));
        self.line(format!("store i64 {j0_next}, ptr {j_ctr}"));
        self.line(format!("br label %{j_head}"));

        self.label_line(&j_rem_check);
        let rem_exists = self.tmp();
        self.line(format!("{rem_exists} = icmp ult i64 {j0}, {}", site.c));
        self.line(format!(
            "br i1 {rem_exists}, label %{j_rem}, label %{j_done}"
        ));
        self.label_line(&j_rem);
        let remaining = self.tmp();
        self.line(format!("{remaining} = sub i64 {}, {j0}", site.c));
        let partial = self.tmp();
        self.line(format!(
            "{partial} = icmp ult i64 {remaining}, {}",
            ctx.tile_j
        ));
        let tj = self.tmp();
        self.line(format!(
            "{tj} = select i1 {partial}, i64 {remaining}, i64 {}",
            ctx.tile_j
        ));
        let panel_base = self.emit_tile_panel_base(site, ctx, &j0);
        self.emit_tile_i_regions(
            site,
            ctx,
            i_ctr,
            j_ctr,
            lo,
            hi,
            i_lo,
            i_hi,
            i_fw_lo,
            i_fw_hi,
            Some((&j0, &tj, false, &panel_base)),
        );
        self.line(format!("br label %{j_done}"));
        self.label_line(&j_done);
    }

    fn emit_tile_panel_base(&mut self, site: &TileSite, ctx: &TileCtx, j0: &str) -> String {
        let jt = self.tmp();
        self.line(format!("{jt} = udiv i64 {j0}, {}", ctx.tile_j));
        let panel_base = self.tmp();
        self.line(format!(
            "{panel_base} = mul i64 {jt}, {}",
            site.k * ctx.tile_j
        ));
        panel_base
    }

    /// The KC nest (packed sites with K > TILE_KC): j-blocks of NC lanes
    /// outer, k-panels of TILE_KC next, the existing head/interior/tail i
    /// regions innermost — the OpenBLAS (jc, kc, ic) order. Per (i-block, kc)
    /// the block's a rows are packed into the contiguous apack scratch, then
    /// the block's j-tiles run the kernel acc[r*TJ + lane] += apack[r][k-kc] *
    /// packed[jt][k][lane]. The leverage: a is re-read once per jb block
    /// (C/NC per element vs the j-outer nest's C/TJ — ÷NC/TJ = 32×, e.g. 16 GB
    /// → 512 MB @4096 f32); the (kc, jb) b window is TILE_KC×NC×elem = 256 KB
    /// (L2-resident across the i sweep), each (kc, jt) slice 8 KB (L1).
    /// Partial sums are **parked in `out`**: every j-tile spills its acc at
    /// the panel end and reloads it at the next panel (the kc==0 panel seeds
    /// instead — the peeled first panel; the gate guarantees ≥2 panels).
    /// Parking is what the (jc, kc, ic) order costs: other i-blocks run
    /// between two panels of the same block, so nothing can stay resident in
    /// scratch — which is also why acc is one j-tile wide, not NC. The
    /// spill/reload is value-preserving and each cell's chain stays
    /// k-ascending, so per-cell the nest is bit-exact vs the j-outer order (R1).
    #[allow(clippy::too_many_arguments)]
    fn emit_tile_packed_kc(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        i_ctr: &str,
        j_ctr: &str,
        lo: &str,
        hi: &str,
        i_lo: &str,
        i_hi: &str,
        i_fw_lo: &str,
        i_fw_hi: &str,
        apack: &PackedBuffer,
    ) {
        let nc = self.profile.nc(&site.elem);
        let jb_ctr = self.scratch("i64");
        let kc_ctr = self.scratch("i64");
        let (jb_head, jb_body, jb_done) = (self.label(), self.label(), self.label());
        let (kc_head, kc_body, kc_done) = (self.label(), self.label(), self.label());

        self.line(format!("store i64 0, ptr {jb_ctr}"));
        self.line(format!("br label %{jb_head}"));
        self.label_line(&jb_head);
        let jb0 = self.tmp();
        self.line(format!("{jb0} = load i64, ptr {jb_ctr}"));
        let jb_all = self.tmp();
        self.line(format!("{jb_all} = icmp uge i64 {jb0}, {}", site.c));
        self.line(format!(
            "br i1 {jb_all}, label %{jb_done}, label %{jb_body}"
        ));
        self.label_line(&jb_body);
        // The block's lane window [jb0, jb_end): NC wide, runtime-short on the
        // last block (min select, the jt-outer remainder discipline).
        let jb_plus = self.tmp();
        self.line(format!("{jb_plus} = add i64 {jb0}, {nc}"));
        let jb_over = self.tmp();
        self.line(format!("{jb_over} = icmp ugt i64 {jb_plus}, {}", site.c));
        let jb_end = self.tmp();
        self.line(format!(
            "{jb_end} = select i1 {jb_over}, i64 {}, i64 {jb_plus}",
            site.c
        ));

        // The peeled kc == 0 panel: seed splat + compute + spill.
        let k_hi0 = ctx.tile_kc.to_string();
        self.emit_tile_kc_i_regions(
            site, ctx, i_ctr, j_ctr, lo, hi, i_lo, i_hi, i_fw_lo, i_fw_hi, apack, &jb0, &jb_end,
            "0", &k_hi0, true,
        );

        // Panels [TILE_KC, K): reload + compute + spill; the last panel is
        // runtime-short (k_hi = min(kc + TILE_KC, K)).
        self.line(format!("store i64 {}, ptr {kc_ctr}", ctx.tile_kc));
        self.line(format!("br label %{kc_head}"));
        self.label_line(&kc_head);
        let kc = self.tmp();
        self.line(format!("{kc} = load i64, ptr {kc_ctr}"));
        let kc_all = self.tmp();
        self.line(format!("{kc_all} = icmp uge i64 {kc}, {}", site.k));
        self.line(format!(
            "br i1 {kc_all}, label %{kc_done}, label %{kc_body}"
        ));
        self.label_line(&kc_body);
        let kc_plus = self.tmp();
        self.line(format!("{kc_plus} = add i64 {kc}, {}", ctx.tile_kc));
        let kc_over = self.tmp();
        self.line(format!("{kc_over} = icmp ugt i64 {kc_plus}, {}", site.k));
        let k_hi = self.tmp();
        self.line(format!(
            "{k_hi} = select i1 {kc_over}, i64 {}, i64 {kc_plus}",
            site.k
        ));
        self.emit_tile_kc_i_regions(
            site, ctx, i_ctr, j_ctr, lo, hi, i_lo, i_hi, i_fw_lo, i_fw_hi, apack, &jb0, &jb_end,
            &kc, &k_hi, false,
        );
        let kc_next = self.tmp();
        self.line(format!("{kc_next} = add i64 {kc}, {}", ctx.tile_kc));
        self.line(format!("store i64 {kc_next}, ptr {kc_ctr}"));
        self.line(format!("br label %{kc_head}"));
        self.label_line(&kc_done);

        let jb_next = self.tmp();
        self.line(format!("{jb_next} = add i64 {jb0}, {nc}"));
        self.line(format!("store i64 {jb_next}, ptr {jb_ctr}"));
        self.line(format!("br label %{jb_head}"));
        self.label_line(&jb_done);
    }

    /// One kc panel's i sweep for the KC nest: the same head/interior/tail
    /// row regions as the j-outer nest, each (i-block, panel) visit packing
    /// its a rows and running the jb block's j-tiles. `first` selects the
    /// trio's seed (kc == 0) vs reload (later panels) first phase.
    #[allow(clippy::too_many_arguments)]
    fn emit_tile_kc_i_regions(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        i_ctr: &str,
        j_ctr: &str,
        lo: &str,
        hi: &str,
        i_lo: &str,
        i_hi: &str,
        i_fw_lo: &str,
        i_fw_hi: &str,
        apack: &PackedBuffer,
        jb0: &str,
        jb_end: &str,
        k_lo: &str,
        k_hi: &str,
        first: bool,
    ) {
        // Head boundary rows (a task range's clipped first row), TI=1.
        let (head_i_head, head_i_body, head_i_done) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {i_lo}, ptr {i_ctr}"));
        self.line(format!("br label %{head_i_head}"));
        self.label_line(&head_i_head);
        let i = self.tmp();
        self.line(format!("{i} = load i64, ptr {i_ctr}"));
        let head_done = self.tmp();
        self.line(format!("{head_done} = icmp uge i64 {i}, {i_fw_lo}"));
        self.line(format!(
            "br i1 {head_done}, label %{head_i_done}, label %{head_i_body}"
        ));
        self.label_line(&head_i_body);
        self.emit_tile_kc_boundary_row(
            site, ctx, lo, hi, &i, j_ctr, apack, jb0, jb_end, k_lo, k_hi, first,
        );
        let i_next = self.tmp();
        self.line(format!("{i_next} = add i64 {i}, 1"));
        self.line(format!("store i64 {i_next}, ptr {i_ctr}"));
        self.line(format!("br label %{head_i_head}"));
        self.label_line(&head_i_done);

        // TI-blocked main over interior full-window rows: pack the block's
        // four a rows for this panel, then run the jb block's j-tiles with
        // acc[r*TJ + lane] (b.ci == 0 — the cashed row-invariance — keeps one
        // packed-b load per (k, lane) shared across the subrows).
        let (blk_i_head, blk_i_body, blk_i_done) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {i_fw_lo}, ptr {i_ctr}"));
        self.line(format!("br label %{blk_i_head}"));
        self.label_line(&blk_i_head);
        let i_blk = self.tmp();
        self.line(format!("{i_blk} = load i64, ptr {i_ctr}"));
        let i_blk_end = self.tmp();
        self.line(format!("{i_blk_end} = add i64 {i_blk}, {}", ctx.tile_i));
        let block_fits = self.tmp();
        self.line(format!(
            "{block_fits} = icmp ule i64 {i_blk_end}, {i_fw_hi}"
        ));
        self.line(format!(
            "br i1 {block_fits}, label %{blk_i_body}, label %{blk_i_done}"
        ));
        self.label_line(&blk_i_body);
        let row0 = self.tmp();
        self.line(format!("{row0} = mul i64 {i_blk}, {}", site.c));
        let mut a_rows = Vec::with_capacity(ctx.tile_i as usize);
        for r in 0..ctx.tile_i {
            let base_r = site.a.base + site.a.ci * r;
            a_rows.push(self.emit_tile_index(
                (base_r != 0).then(|| base_r.to_string()),
                &[(site.a.ci, i_blk.as_str())],
            ));
        }
        self.emit_tile_kc_apack(site, ctx, apack, &a_rows, k_lo, k_hi);
        self.emit_tile_kc_j_split(
            site, ctx, j_ctr, jb0, jb_end, &row0, ctx.tile_i, apack, k_lo, k_hi, first,
        );
        self.line(format!("store i64 {i_blk_end}, ptr {i_ctr}"));
        self.line(format!("br label %{blk_i_head}"));
        self.label_line(&blk_i_done);

        // Tail rows (rows % TILE_I) plus a task range's clipped last row, TI=1.
        let (tail_i_head, tail_i_body, tail_i_done) = (self.label(), self.label(), self.label());
        self.line(format!("br label %{tail_i_head}"));
        self.label_line(&tail_i_head);
        let i = self.tmp();
        self.line(format!("{i} = load i64, ptr {i_ctr}"));
        let tail_done = self.tmp();
        self.line(format!("{tail_done} = icmp uge i64 {i}, {i_hi}"));
        self.line(format!(
            "br i1 {tail_done}, label %{tail_i_done}, label %{tail_i_body}"
        ));
        self.label_line(&tail_i_body);
        self.emit_tile_kc_boundary_row(
            site, ctx, lo, hi, &i, j_ctr, apack, jb0, jb_end, k_lo, k_hi, first,
        );
        let i_next = self.tmp();
        self.line(format!("{i_next} = add i64 {i}, 1"));
        self.line(format!("store i64 {i_next}, ptr {i_ctr}"));
        self.line(format!("br label %{tail_i_head}"));
        self.label_line(&tail_i_done);
    }

    /// One TI=1 boundary row for the KC nest: the rung-1 signed jw clip, the
    /// one-row a-panel pack, then the jb block's j-tiles — each tile clipped
    /// against the row's window and skipped when empty (the jt-outer
    /// boundary discipline, one level in).
    #[allow(clippy::too_many_arguments)]
    fn emit_tile_kc_boundary_row(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        lo: &str,
        hi: &str,
        i: &str,
        j_ctr: &str,
        apack: &PackedBuffer,
        jb0: &str,
        jb_end: &str,
        k_lo: &str,
        k_hi: &str,
        first: bool,
    ) {
        let row0 = self.tmp();
        self.line(format!("{row0} = mul i64 {i}, {}", site.c));
        let jw_lo_raw = self.tmp();
        self.line(format!("{jw_lo_raw} = sub i64 {lo}, {row0}"));
        let jw_lo_negative = self.tmp();
        self.line(format!("{jw_lo_negative} = icmp slt i64 {jw_lo_raw}, 0"));
        let jw_lo = self.tmp();
        self.line(format!(
            "{jw_lo} = select i1 {jw_lo_negative}, i64 0, i64 {jw_lo_raw}"
        ));
        let jw_hi_raw = self.tmp();
        self.line(format!("{jw_hi_raw} = sub i64 {hi}, {row0}"));
        let jw_hi_past_c = self.tmp();
        self.line(format!(
            "{jw_hi_past_c} = icmp sgt i64 {jw_hi_raw}, {}",
            site.c
        ));
        let jw_hi = self.tmp();
        self.line(format!(
            "{jw_hi} = select i1 {jw_hi_past_c}, i64 {}, i64 {jw_hi_raw}",
            site.c
        ));
        let a_row = self.emit_tile_index(
            (site.a.base != 0).then(|| site.a.base.to_string()),
            &[(site.a.ci, i)],
        );
        self.emit_tile_kc_apack(site, ctx, apack, &[a_row], k_lo, k_hi);

        let (j_head, j_main, j_rem_check, j_rem, j_done) = (
            self.label(),
            self.label(),
            self.label(),
            self.label(),
            self.label(),
        );
        self.line(format!("store i64 {jb0}, ptr {j_ctr}"));
        self.line(format!("br label %{j_head}"));
        self.label_line(&j_head);
        let j0 = self.tmp();
        self.line(format!("{j0} = load i64, ptr {j_ctr}"));
        let j0_full = self.tmp();
        self.line(format!("{j0_full} = add i64 {j0}, {}", ctx.tile_j));
        let full_tile = self.tmp();
        self.line(format!("{full_tile} = icmp ule i64 {j0_full}, {jb_end}"));
        self.line(format!(
            "br i1 {full_tile}, label %{j_main}, label %{j_rem_check}"
        ));
        self.label_line(&j_main);
        let lane_full = ctx.tile_j.to_string();
        self.emit_tile_kc_boundary_tile(
            site, ctx, &row0, &jw_lo, &jw_hi, &j0, &lane_full, true, apack, k_lo, k_hi, first,
        );
        let j0_next = self.tmp();
        self.line(format!("{j0_next} = add i64 {j0}, {}", ctx.tile_j));
        self.line(format!("store i64 {j0_next}, ptr {j_ctr}"));
        self.line(format!("br label %{j_head}"));

        self.label_line(&j_rem_check);
        let rem_exists = self.tmp();
        self.line(format!("{rem_exists} = icmp ult i64 {j0}, {jb_end}"));
        self.line(format!(
            "br i1 {rem_exists}, label %{j_rem}, label %{j_done}"
        ));
        self.label_line(&j_rem);
        let tj = self.tmp();
        self.line(format!("{tj} = sub i64 {jb_end}, {j0}"));
        self.emit_tile_kc_boundary_tile(
            site, ctx, &row0, &jw_lo, &jw_hi, &j0, &tj, false, apack, k_lo, k_hi, first,
        );
        self.line(format!("br label %{j_done}"));
        self.label_line(&j_done);
    }

    /// One window-clipped j-tile of a boundary row: the clip [tile_lo,
    /// clipped_hi) of the tile against the row's jw window, the empty-tile
    /// skip, then the trio over the live lanes (out/acc addressed from
    /// tile_lo, the packed-b lane from panel_lane0 — the jt-outer boundary
    /// trio unchanged).
    #[allow(clippy::too_many_arguments)]
    fn emit_tile_kc_boundary_tile(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        row0: &str,
        jw_lo: &str,
        jw_hi: &str,
        j0: &str,
        bound: &str,
        main: bool,
        apack: &PackedBuffer,
        k_lo: &str,
        k_hi: &str,
        first: bool,
    ) {
        let panel_base = self.emit_tile_panel_base(site, ctx, j0);
        let tile_hi = self.tmp();
        self.line(format!("{tile_hi} = add i64 {j0}, {bound}"));
        let starts_before_tile = self.tmp();
        self.line(format!("{starts_before_tile} = icmp ult i64 {jw_lo}, {j0}"));
        let tile_lo = self.tmp();
        self.line(format!(
            "{tile_lo} = select i1 {starts_before_tile}, i64 {j0}, i64 {jw_lo}"
        ));
        let ends_after_tile = self.tmp();
        self.line(format!(
            "{ends_after_tile} = icmp ugt i64 {jw_hi}, {tile_hi}"
        ));
        let clipped_hi = self.tmp();
        self.line(format!(
            "{clipped_hi} = select i1 {ends_after_tile}, i64 {tile_hi}, i64 {jw_hi}"
        ));
        let has_lanes = self.tmp();
        self.line(format!(
            "{has_lanes} = icmp ult i64 {tile_lo}, {clipped_hi}"
        ));
        let (body, next) = (self.label(), self.label());
        self.line(format!("br i1 {has_lanes}, label %{body}, label %{next}"));
        self.label_line(&body);
        let lanes = self.tmp();
        self.line(format!("{lanes} = sub i64 {clipped_hi}, {tile_lo}"));
        let panel_lane0 = self.tmp();
        self.line(format!("{panel_lane0} = sub i64 {tile_lo}, {j0}"));
        // The acc block is one j-tile wide and the tile's partial sums are
        // parked in `out` at each panel end, so every tile computes at base 0.
        let acc_base = "0".to_owned();
        self.emit_tile_kc_trio(
            site,
            ctx,
            apack,
            &tile_lo,
            &acc_base,
            row0,
            1,
            &lanes,
            main,
            &panel_base,
            Some(&panel_lane0),
            k_lo,
            k_hi,
            first,
        );
        self.line(format!("br label %{next}"));
        self.label_line(&next);
    }

    /// The jb block's j-tiles for one interior TI-block at one kc panel:
    /// constant-TJ main tiles while `j0 + TILE_J <= jb_end`, then one
    /// remainder tile at the runtime `tj = jb_end - j0` (only the last,
    /// runtime-short jb block can have one) — the jt-outer split with the
    /// block end for the row end.
    #[allow(clippy::too_many_arguments)]
    fn emit_tile_kc_j_split(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        j_ctr: &str,
        jb0: &str,
        jb_end: &str,
        row0: &str,
        rows: u64,
        apack: &PackedBuffer,
        k_lo: &str,
        k_hi: &str,
        first: bool,
    ) {
        let (j_head, j_main, j_rem_check, j_rem, j_done) = (
            self.label(),
            self.label(),
            self.label(),
            self.label(),
            self.label(),
        );
        self.line(format!("store i64 {jb0}, ptr {j_ctr}"));
        self.line(format!("br label %{j_head}"));
        self.label_line(&j_head);
        let j0 = self.tmp();
        self.line(format!("{j0} = load i64, ptr {j_ctr}"));
        let j0_full = self.tmp();
        self.line(format!("{j0_full} = add i64 {j0}, {}", ctx.tile_j));
        let full_tile = self.tmp();
        self.line(format!("{full_tile} = icmp ule i64 {j0_full}, {jb_end}"));
        self.line(format!(
            "br i1 {full_tile}, label %{j_main}, label %{j_rem_check}"
        ));
        self.label_line(&j_main);
        let panel_base = self.emit_tile_panel_base(site, ctx, &j0);
        // The acc block is one j-tile wide and the tile's partial sums are
        // parked in `out` at each panel end, so every tile computes at base 0.
        let acc_base = "0".to_owned();
        let lane_full = ctx.tile_j.to_string();
        self.emit_tile_kc_trio(
            site,
            ctx,
            apack,
            &j0,
            &acc_base,
            row0,
            rows,
            &lane_full,
            true,
            &panel_base,
            None,
            k_lo,
            k_hi,
            first,
        );
        let j0_next = self.tmp();
        self.line(format!("{j0_next} = add i64 {j0}, {}", ctx.tile_j));
        self.line(format!("store i64 {j0_next}, ptr {j_ctr}"));
        self.line(format!("br label %{j_head}"));

        self.label_line(&j_rem_check);
        let rem_exists = self.tmp();
        self.line(format!("{rem_exists} = icmp ult i64 {j0}, {jb_end}"));
        self.line(format!(
            "br i1 {rem_exists}, label %{j_rem}, label %{j_done}"
        ));
        self.label_line(&j_rem);
        let tj = self.tmp();
        self.line(format!("{tj} = sub i64 {jb_end}, {j0}"));
        let panel_base = self.emit_tile_panel_base(site, ctx, &j0);
        // The acc block is one j-tile wide and the tile's partial sums are
        // parked in `out` at each panel end, so every tile computes at base 0.
        let acc_base = "0".to_owned();
        self.emit_tile_kc_trio(
            site,
            ctx,
            apack,
            &j0,
            &acc_base,
            row0,
            rows,
            &tj,
            false,
            &panel_base,
            None,
            k_lo,
            k_hi,
            first,
        );
        self.line(format!("br label %{j_done}"));
        self.label_line(&j_done);
    }

    /// The a-panel pack for one i-block at one kc panel: subrow r's source
    /// row (`a.base + a.ci·(i+r)`, hoisted in `a_rows`) is copied over
    /// [k_lo, k_hi) into apack[r*TILE_KC + (k - k_lo)] — strided source rows
    /// made contiguous and 64-aligned, so the kernel's a loads walk L1 lines
    /// sequentially instead of re-reading the strided rows once per j-tile.
    fn emit_tile_kc_apack(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        apack: &PackedBuffer,
        a_rows: &[Option<String>],
        k_lo: &str,
        k_hi: &str,
    ) {
        for (r, a_row) in a_rows.iter().enumerate() {
            let (head, body, done) = (self.label(), self.label(), self.label());
            self.line(format!("store i64 {k_lo}, ptr {}", ctx.k_ctr));
            self.line(format!("br label %{head}"));
            self.label_line(&head);
            let kk = self.tmp();
            self.line(format!("{kk} = load i64, ptr {}", ctx.k_ctr));
            let all_k = self.tmp();
            self.line(format!("{all_k} = icmp uge i64 {kk}, {k_hi}"));
            self.line(format!("br i1 {all_k}, label %{done}, label %{body}"));
            self.label_line(&body);
            let a_index = self
                .emit_tile_index(a_row.clone(), &[(site.a.ck, kk.as_str())])
                .unwrap_or_else(|| "0".to_owned());
            let a_elem_ptr = self.tmp();
            self.line(format!(
                "{a_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {a_index}",
                ctx.a_llt, ctx.a_ptr
            ));
            let value = self.tmp();
            self.line(format!("{value} = load {}, ptr {a_elem_ptr}", ctx.elem_llt));
            let koff = self.tmp();
            self.line(format!("{koff} = sub i64 {kk}, {k_lo}"));
            let apack_index = if r == 0 {
                koff
            } else {
                let index = self.tmp();
                self.line(format!(
                    "{index} = add i64 {koff}, {}",
                    r as u64 * ctx.tile_kc
                ));
                index
            };
            let apack_ptr = self.tmp();
            self.line(format!(
                "{apack_ptr} = getelementptr {}, ptr {}, i64 0, i64 {apack_index}",
                apack.llt, apack.ptr
            ));
            self.line(format!("store {} {value}, ptr {apack_ptr}", ctx.elem_llt));
            let kk_next = self.tmp();
            self.line(format!("{kk_next} = add i64 {kk}, 1"));
            self.line(format!("store i64 {kk_next}, ptr {}", ctx.k_ctr));
            self.line(format!("br label %{head}"));
            self.label_line(&done);
        }
    }

    /// The seed-or-reload / k-loop / spill-store trio for one j-tile of a
    /// `rows`-row i-block at one kc panel. Subrow r's lanes sit at acc
    /// offset `acc_base + r*NC + lane` (`acc_base` = the tile's lane start
    /// within the jb block). The first phase seeds acc (kc == 0) or reloads
    /// the spilled partial sums from `out` (later panels); the k loop runs
    /// the panel [k_lo, k_hi) with a reads from the packed a panel (one
    /// scalar apack load per (k, subrow)) and the shared packed-b load per
    /// (k, lane), ×2-unrolled on the full TI constant-width main path; the
    /// store phase ALWAYS spills acc back to `out` — the last panel's spill
    /// is the result store.
    #[allow(clippy::too_many_arguments)]
    fn emit_tile_kc_trio(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        apack: &PackedBuffer,
        j0: &str,
        acc_base: &str,
        row0: &str,
        rows: u64,
        bound: &str,
        main: bool,
        panel_base: &str,
        panel_lane0: Option<&str>,
        k_lo: &str,
        k_hi: &str,
        first: bool,
    ) {
        // plan-s30: same carve-out as the j-outer trio — the constant-width
        // main tile runs on phi-carried `<TJ x elem>` accumulators. The
        // reload/park still touch `out`, once per panel, outside the k loop.
        if main && rows == ctx.tile_i && bound.parse::<u64>().ok() == Some(ctx.tile_j) {
            self.emit_tile_kc_trio_vec(
                site, ctx, apack, j0, row0, rows, panel_base, k_lo, k_hi, first,
            );
            return;
        }

        // The acc row stride: one j-tile per subrow (see the acc allocation —
        // partial sums park in `out`, so acc is never NC-wide).
        let nc = ctx.tile_j;
        let no_b_row = None;
        let out_start = self.tmp();
        self.line(format!("{out_start} = add i64 {row0}, {j0}"));

        // Seed (kc == 0) or reload (later panels): one lane loop per subrow,
        // same flat-offset discipline as the j-outer seed splat.
        for r in 0..rows {
            let out_start_r = if first || r == 0 {
                out_start.clone()
            } else {
                let shifted = self.tmp();
                self.line(format!("{shifted} = add i64 {out_start}, {}", r * site.c));
                shifted
            };
            let (seed_head, seed_body, seed_done) = (self.label(), self.label(), self.label());
            self.line(format!("store i64 0, ptr {}", ctx.lane_ctr));
            self.line(format!("br label %{seed_head}"));
            self.label_line(&seed_head);
            let seed_lane = self.tmp();
            self.line(format!("{seed_lane} = load i64, ptr {}", ctx.lane_ctr));
            let seed_done_cond = self.tmp();
            self.line(format!(
                "{seed_done_cond} = icmp uge i64 {seed_lane}, {bound}"
            ));
            self.line(format!(
                "br i1 {seed_done_cond}, label %{seed_done}, label %{seed_body}"
            ));
            self.label_line(&seed_body);
            let acc_lane = self.emit_tile_kc_acc_lane(&seed_lane, acc_base, r, nc);
            let acc_ptr = self.tmp();
            self.line(format!(
                "{acc_ptr} = getelementptr {}, ptr {}, i64 0, i64 {acc_lane}",
                ctx.acc_llt, ctx.acc
            ));
            if first {
                self.line(format!(
                    "store {} {}, ptr {acc_ptr}",
                    ctx.elem_llt, ctx.seed
                ));
            } else {
                let out_index = self.tmp();
                self.line(format!("{out_index} = add i64 {out_start_r}, {seed_lane}"));
                let out_elem_ptr = self.tmp();
                self.line(format!(
                    "{out_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {out_index}",
                    ctx.out_llt, ctx.out_ptr
                ));
                let spilled = self.tmp();
                self.line(format!(
                    "{spilled} = load {}, ptr {out_elem_ptr}",
                    ctx.elem_llt
                ));
                self.line(format!("store {} {spilled}, ptr {acc_ptr}", ctx.elem_llt));
            }
            let seed_lane_next = self.tmp();
            self.line(format!("{seed_lane_next} = add i64 {seed_lane}, 1"));
            self.line(format!("store i64 {seed_lane_next}, ptr {}", ctx.lane_ctr));
            self.line(format!("br label %{seed_head}"));
            self.label_line(&seed_done);
        }

        // Only the full TI-blocked, constant-width body unrolls k. Boundary,
        // tail-row, and remainder bodies retain the single-k loop.
        let unroll = main && rows == ctx.tile_i;
        let (k_head, k_body, k_done) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {k_lo}, ptr {}", ctx.k_ctr));
        self.line(format!("br label %{k_head}"));
        self.label_line(&k_head);
        let kk = self.tmp();
        self.line(format!("{kk} = load i64, ptr {}", ctx.k_ctr));
        if unroll {
            let (k_tail_check, k_tail) = (self.label(), self.label());
            let kk1 = self.tmp();
            self.line(format!("{kk1} = add i64 {kk}, 1"));
            let pair = self.tmp();
            self.line(format!("{pair} = icmp ult i64 {kk1}, {k_hi}"));
            self.line(format!(
                "br i1 {pair}, label %{k_body}, label %{k_tail_check}"
            ));
            self.label_line(&k_body);
            let koff0 = self.tmp();
            self.line(format!("{koff0} = sub i64 {kk}, {k_lo}"));
            let koff1 = self.tmp();
            self.line(format!("{koff1} = sub i64 {kk1}, {k_lo}"));
            let a0 = self.emit_tile_kc_a_values(ctx, apack, rows, &koff0);
            let a1 = self.emit_tile_kc_a_values(ctx, apack, rows, &koff1);
            if let Some(packed) = &ctx.packed {
                let next_k = self.tmp();
                self.line(format!("{next_k} = add i64 {kk}, 2"));
                let next_offset = self.tmp();
                self.line(format!("{next_offset} = mul i64 {next_k}, {}", ctx.tile_j));
                let next_index = self.tmp();
                self.line(format!(
                    "{next_index} = add i64 {panel_base}, {next_offset}"
                ));
                let next_ptr = self.tmp();
                self.line(format!(
                    "{next_ptr} = getelementptr {}, ptr {}, i64 0, i64 {next_index}",
                    packed.llt, packed.ptr
                ));
                self.line(format!(
                    "call void @llvm.prefetch.p0(ptr {next_ptr}, i32 0, i32 3, i32 1)"
                ));
            }
            self.emit_tile_lane_loop(
                site,
                ctx,
                j0,
                &no_b_row,
                bound,
                Some(panel_base),
                panel_lane0,
                Some(acc_base),
                nc,
                &[(&kk, a0), (&kk1, a1)],
            );
            let kk2 = self.tmp();
            self.line(format!("{kk2} = add i64 {kk}, 2"));
            self.line(format!("store i64 {kk2}, ptr {}", ctx.k_ctr));
            self.line(format!("br label %{k_head}"));

            self.label_line(&k_tail_check);
            let tail = self.tmp();
            self.line(format!("{tail} = icmp ult i64 {kk}, {k_hi}"));
            self.line(format!("br i1 {tail}, label %{k_tail}, label %{k_done}"));
            self.label_line(&k_tail);
            let koff = self.tmp();
            self.line(format!("{koff} = sub i64 {kk}, {k_lo}"));
            let a = self.emit_tile_kc_a_values(ctx, apack, rows, &koff);
            self.emit_tile_lane_loop(
                site,
                ctx,
                j0,
                &no_b_row,
                bound,
                Some(panel_base),
                panel_lane0,
                Some(acc_base),
                nc,
                &[(&kk, a)],
            );
            self.line(format!("br label %{k_done}"));
        } else {
            let depth_done = self.tmp();
            self.line(format!("{depth_done} = icmp uge i64 {kk}, {k_hi}"));
            self.line(format!(
                "br i1 {depth_done}, label %{k_done}, label %{k_body}"
            ));
            self.label_line(&k_body);
            let koff = self.tmp();
            self.line(format!("{koff} = sub i64 {kk}, {k_lo}"));
            let a = self.emit_tile_kc_a_values(ctx, apack, rows, &koff);
            self.emit_tile_lane_loop(
                site,
                ctx,
                j0,
                &no_b_row,
                bound,
                Some(panel_base),
                panel_lane0,
                Some(acc_base),
                nc,
                &[(&kk, a)],
            );
            let kk_next = self.tmp();
            self.line(format!("{kk_next} = add i64 {kk}, 1"));
            self.line(format!("store i64 {kk_next}, ptr {}", ctx.k_ctr));
            self.line(format!("br label %{k_head}"));
        }
        self.label_line(&k_done);

        // Spill: one lane loop per subrow at out[(i+r)*C + j0 + lane] — every
        // panel (the acc parking across kc); the last panel's is the result.
        for r in 0..rows {
            let out_start_r = if r == 0 {
                out_start.clone()
            } else {
                let shifted = self.tmp();
                self.line(format!("{shifted} = add i64 {out_start}, {}", r * site.c));
                shifted
            };
            let (store_head, store_body, store_done) = (self.label(), self.label(), self.label());
            self.line(format!("store i64 0, ptr {}", ctx.lane_ctr));
            self.line(format!("br label %{store_head}"));
            self.label_line(&store_head);
            let store_lane = self.tmp();
            self.line(format!("{store_lane} = load i64, ptr {}", ctx.lane_ctr));
            let stores_done = self.tmp();
            self.line(format!(
                "{stores_done} = icmp uge i64 {store_lane}, {bound}"
            ));
            self.line(format!(
                "br i1 {stores_done}, label %{store_done}, label %{store_body}"
            ));
            self.label_line(&store_body);
            let acc_lane = self.emit_tile_kc_acc_lane(&store_lane, acc_base, r, nc);
            let final_acc_ptr = self.tmp();
            self.line(format!(
                "{final_acc_ptr} = getelementptr {}, ptr {}, i64 0, i64 {acc_lane}",
                ctx.acc_llt, ctx.acc
            ));
            let final_value = self.tmp();
            self.line(format!(
                "{final_value} = load {}, ptr {final_acc_ptr}",
                ctx.elem_llt
            ));
            let out_index = self.tmp();
            self.line(format!("{out_index} = add i64 {out_start_r}, {store_lane}"));
            let out_elem_ptr = self.tmp();
            self.line(format!(
                "{out_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {out_index}",
                ctx.out_llt, ctx.out_ptr
            ));
            self.line(format!(
                "store {} {final_value}, ptr {out_elem_ptr}",
                ctx.elem_llt
            ));
            let store_lane_next = self.tmp();
            self.line(format!("{store_lane_next} = add i64 {store_lane}, 1"));
            self.line(format!("store i64 {store_lane_next}, ptr {}", ctx.lane_ctr));
            self.line(format!("br label %{store_head}"));
            self.label_line(&store_done);
        }
    }

    /// The acc flat offset `acc_base + r*NC + lane` for one (subrow, lane) of
    /// a KC trio.
    fn emit_tile_kc_acc_lane(&mut self, lane: &str, acc_base: &str, r: u64, nc: u64) -> String {
        if r == 0 {
            let offset = self.tmp();
            self.line(format!("{offset} = add i64 {lane}, {acc_base}"));
            offset
        } else {
            let based = self.tmp();
            self.line(format!("{based} = add i64 {acc_base}, {}", r * nc));
            let offset = self.tmp();
            self.line(format!("{offset} = add i64 {lane}, {based}"));
            offset
        }
    }

    /// The kernel's a reads from the packed panel: subrow r's value for the
    /// current k is apack[r*TILE_KC + koff] with koff = k - k_lo.
    fn emit_tile_kc_a_values(
        &mut self,
        ctx: &TileCtx,
        apack: &PackedBuffer,
        rows: u64,
        koff: &str,
    ) -> Vec<String> {
        (0..rows)
            .map(|r| {
                let index = if r == 0 {
                    koff.to_owned()
                } else {
                    let offset = self.tmp();
                    self.line(format!("{offset} = add i64 {koff}, {}", r * ctx.tile_kc));
                    offset
                };
                let ptr = self.tmp();
                self.line(format!(
                    "{ptr} = getelementptr {}, ptr {}, i64 0, i64 {index}",
                    apack.llt, apack.ptr
                ));
                let value = self.tmp();
                self.line(format!("{value} = load {}, ptr {ptr}", ctx.elem_llt));
                value
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_tile_i_regions(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        i_ctr: &str,
        j_ctr: &str,
        lo: &str,
        hi: &str,
        i_lo: &str,
        i_hi: &str,
        i_fw_lo: &str,
        i_fw_hi: &str,
        j_tile: Option<(&str, &str, bool, &str)>,
    ) {
        // Head boundary rows (a task range's clipped first row), TI=1.
        let (head_i_head, head_i_body, head_i_done) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {i_lo}, ptr {i_ctr}"));
        self.line(format!("br label %{head_i_head}"));
        self.label_line(&head_i_head);
        let i = self.tmp();
        self.line(format!("{i} = load i64, ptr {i_ctr}"));
        let head_done = self.tmp();
        self.line(format!("{head_done} = icmp uge i64 {i}, {i_fw_lo}"));
        self.line(format!(
            "br i1 {head_done}, label %{head_i_done}, label %{head_i_body}"
        ));
        self.label_line(&head_i_body);
        if let Some((j0, bound, main, panel_base)) = j_tile {
            self.emit_tile_packed_boundary_row(site, ctx, lo, hi, &i, j0, bound, main, panel_base);
        } else {
            self.emit_tile_row_split_j(site, ctx, j_ctr, lo, hi, &i);
        }
        let i_next = self.tmp();
        self.line(format!("{i_next} = add i64 {i}, 1"));
        self.line(format!("store i64 {i_next}, ptr {i_ctr}"));
        self.line(format!("br label %{head_i_head}"));
        self.label_line(&head_i_done);

        // TI-blocked main over interior full-window rows: subrow r's
        // accumulators sit at acc offset r*TILE_J; one b load per (k, lane)
        // feeds all TILE_I chains (b.ci == 0 — the cashed row-invariance).
        let (blk_i_head, blk_i_body, blk_i_done) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {i_fw_lo}, ptr {i_ctr}"));
        self.line(format!("br label %{blk_i_head}"));
        self.label_line(&blk_i_head);
        let i_blk = self.tmp();
        self.line(format!("{i_blk} = load i64, ptr {i_ctr}"));
        let i_blk_end = self.tmp();
        self.line(format!("{i_blk_end} = add i64 {i_blk}, {}", ctx.tile_i));
        let block_fits = self.tmp();
        self.line(format!(
            "{block_fits} = icmp ule i64 {i_blk_end}, {i_fw_hi}"
        ));
        self.line(format!(
            "br i1 {block_fits}, label %{blk_i_body}, label %{blk_i_done}"
        ));
        self.label_line(&blk_i_body);
        let row0 = self.tmp();
        self.line(format!("{row0} = mul i64 {i_blk}, {}", site.c));
        let mut a_rows = Vec::with_capacity(ctx.tile_i as usize);
        for r in 0..ctx.tile_i {
            let base_r = site.a.base + site.a.ci * r;
            a_rows.push(self.emit_tile_index(
                (base_r != 0).then(|| base_r.to_string()),
                &[(site.a.ci, i_blk.as_str())],
            ));
        }
        let b_row = (site.b.base != 0).then(|| site.b.base.to_string());
        if let Some((j0, bound, main, panel_base)) = j_tile {
            self.emit_tile_trio(
                site,
                ctx,
                j0,
                &row0,
                &a_rows,
                &b_row,
                bound,
                main,
                Some(panel_base),
                None,
            );
        } else {
            self.emit_tile_j_split(
                site,
                ctx,
                j_ctr,
                "0",
                &site.c.to_string(),
                &row0,
                &a_rows,
                &b_row,
            );
        }
        self.line(format!("store i64 {i_blk_end}, ptr {i_ctr}"));
        self.line(format!("br label %{blk_i_head}"));
        self.label_line(&blk_i_done);

        // Tail rows (rows % TILE_I) plus a task range's clipped last row, TI=1.
        let (tail_i_head, tail_i_body, tail_i_done) = (self.label(), self.label(), self.label());
        self.line(format!("br label %{tail_i_head}"));
        self.label_line(&tail_i_head);
        let i = self.tmp();
        self.line(format!("{i} = load i64, ptr {i_ctr}"));
        let tail_done = self.tmp();
        self.line(format!("{tail_done} = icmp uge i64 {i}, {i_hi}"));
        self.line(format!(
            "br i1 {tail_done}, label %{tail_i_done}, label %{tail_i_body}"
        ));
        self.label_line(&tail_i_body);
        if let Some((j0, bound, main, panel_base)) = j_tile {
            self.emit_tile_packed_boundary_row(site, ctx, lo, hi, &i, j0, bound, main, panel_base);
        } else {
            self.emit_tile_row_split_j(site, ctx, j_ctr, lo, hi, &i);
        }
        let i_next = self.tmp();
        self.line(format!("{i_next} = add i64 {i}, 1"));
        self.line(format!("store i64 {i_next}, ptr {i_ctr}"));
        self.line(format!("br label %{tail_i_head}"));
        self.label_line(&tail_i_done);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_tile_packed_boundary_row(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        lo: &str,
        hi: &str,
        i: &str,
        j0: &str,
        bound: &str,
        main: bool,
        panel_base: &str,
    ) {
        let row0 = self.tmp();
        self.line(format!("{row0} = mul i64 {i}, {}", site.c));
        let jw_lo_raw = self.tmp();
        self.line(format!("{jw_lo_raw} = sub i64 {lo}, {row0}"));
        let jw_lo_negative = self.tmp();
        self.line(format!("{jw_lo_negative} = icmp slt i64 {jw_lo_raw}, 0"));
        let jw_lo = self.tmp();
        self.line(format!(
            "{jw_lo} = select i1 {jw_lo_negative}, i64 0, i64 {jw_lo_raw}"
        ));
        let jw_hi_raw = self.tmp();
        self.line(format!("{jw_hi_raw} = sub i64 {hi}, {row0}"));
        let jw_hi_past_c = self.tmp();
        self.line(format!(
            "{jw_hi_past_c} = icmp sgt i64 {jw_hi_raw}, {}",
            site.c
        ));
        let jw_hi = self.tmp();
        self.line(format!(
            "{jw_hi} = select i1 {jw_hi_past_c}, i64 {}, i64 {jw_hi_raw}",
            site.c
        ));

        let tile_hi = self.tmp();
        self.line(format!("{tile_hi} = add i64 {j0}, {bound}"));
        let starts_before_tile = self.tmp();
        self.line(format!("{starts_before_tile} = icmp ult i64 {jw_lo}, {j0}"));
        let tile_lo = self.tmp();
        self.line(format!(
            "{tile_lo} = select i1 {starts_before_tile}, i64 {j0}, i64 {jw_lo}"
        ));
        let ends_after_tile = self.tmp();
        self.line(format!(
            "{ends_after_tile} = icmp ugt i64 {jw_hi}, {tile_hi}"
        ));
        let clipped_hi = self.tmp();
        self.line(format!(
            "{clipped_hi} = select i1 {ends_after_tile}, i64 {tile_hi}, i64 {jw_hi}"
        ));
        let has_lanes = self.tmp();
        self.line(format!(
            "{has_lanes} = icmp ult i64 {tile_lo}, {clipped_hi}"
        ));
        let (body, done) = (self.label(), self.label());
        self.line(format!("br i1 {has_lanes}, label %{body}, label %{done}"));
        self.label_line(&body);
        let lanes = self.tmp();
        self.line(format!("{lanes} = sub i64 {clipped_hi}, {tile_lo}"));
        let panel_lane0 = self.tmp();
        self.line(format!("{panel_lane0} = sub i64 {tile_lo}, {j0}"));
        let a_row = self.emit_tile_index(
            (site.a.base != 0).then(|| site.a.base.to_string()),
            &[(site.a.ci, i)],
        );
        let b_row = (site.b.base != 0).then(|| site.b.base.to_string());
        self.emit_tile_trio(
            site,
            ctx,
            &tile_lo,
            &row0,
            &[a_row],
            &b_row,
            &lanes,
            main,
            Some(panel_base),
            Some(&panel_lane0),
        );
        self.line(format!("br label %{done}"));
        self.label_line(&done);
    }

    /// One TI=1 row body for the gated nest: the rung-1 clipped lane window
    /// (signed — `lo - i*C` goes negative) and hoisted row bases, then the
    /// fixed-TJ j split.
    fn emit_tile_row_split_j(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        j_ctr: &str,
        lo: &str,
        hi: &str,
        i: &str,
    ) {
        let row0 = self.tmp();
        self.line(format!("{row0} = mul i64 {i}, {}", site.c));
        let jw_lo_raw = self.tmp();
        self.line(format!("{jw_lo_raw} = sub i64 {lo}, {row0}"));
        let jw_lo_negative = self.tmp();
        self.line(format!("{jw_lo_negative} = icmp slt i64 {jw_lo_raw}, 0"));
        let jw_lo = self.tmp();
        self.line(format!(
            "{jw_lo} = select i1 {jw_lo_negative}, i64 0, i64 {jw_lo_raw}"
        ));
        let jw_hi_raw = self.tmp();
        self.line(format!("{jw_hi_raw} = sub i64 {hi}, {row0}"));
        let jw_hi_past_c = self.tmp();
        self.line(format!(
            "{jw_hi_past_c} = icmp sgt i64 {jw_hi_raw}, {}",
            site.c
        ));
        let jw_hi = self.tmp();
        self.line(format!(
            "{jw_hi} = select i1 {jw_hi_past_c}, i64 {}, i64 {jw_hi_raw}",
            site.c
        ));
        let a_row = self.emit_tile_index(
            (site.a.base != 0).then(|| site.a.base.to_string()),
            &[(site.a.ci, i)],
        );
        let b_row = self.emit_tile_index(
            (site.b.base != 0).then(|| site.b.base.to_string()),
            &[(site.b.ci, i)],
        );
        self.emit_tile_j_split(site, ctx, j_ctr, &jw_lo, &jw_hi, &row0, &[a_row], &b_row);
    }

    /// The fixed-TJ j loop over one row block: main tiles bounded by the
    /// compile-time `TILE_J` while `j0 + TILE_J <= jw_hi`, then one remainder
    /// tile at the runtime `tj` bound, entered only when `j0 < jw_hi` remains
    /// (task-grain splits make `jw_hi` runtime in general — the remainder
    /// path is never dead code).
    #[allow(clippy::too_many_arguments)]
    fn emit_tile_j_split(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        j_ctr: &str,
        jw_lo: &str,
        jw_hi: &str,
        row0: &str,
        a_rows: &[Option<String>],
        b_row: &Option<String>,
    ) {
        let (j_head, j_main, j_rem_check, j_rem, j_done) = (
            self.label(),
            self.label(),
            self.label(),
            self.label(),
            self.label(),
        );
        self.line(format!("store i64 {jw_lo}, ptr {j_ctr}"));
        self.line(format!("br label %{j_head}"));
        self.label_line(&j_head);
        let j0 = self.tmp();
        self.line(format!("{j0} = load i64, ptr {j_ctr}"));
        let j0_full = self.tmp();
        self.line(format!("{j0_full} = add i64 {j0}, {}", ctx.tile_j));
        let full_tile = self.tmp();
        self.line(format!("{full_tile} = icmp ule i64 {j0_full}, {jw_hi}"));
        self.line(format!(
            "br i1 {full_tile}, label %{j_main}, label %{j_rem_check}"
        ));
        self.label_line(&j_main);
        let lane_full = ctx.tile_j.to_string();
        self.emit_tile_trio(
            site, ctx, &j0, row0, a_rows, b_row, &lane_full, true, None, None,
        );
        let j0_next = self.tmp();
        self.line(format!("{j0_next} = add i64 {j0}, {}", ctx.tile_j));
        self.line(format!("store i64 {j0_next}, ptr {j_ctr}"));
        self.line(format!("br label %{j_head}"));
        self.label_line(&j_rem_check);
        let rem_exists = self.tmp();
        self.line(format!("{rem_exists} = icmp ult i64 {j0}, {jw_hi}"));
        self.line(format!(
            "br i1 {rem_exists}, label %{j_rem}, label %{j_done}"
        ));
        self.label_line(&j_rem);
        let remaining = self.tmp();
        self.line(format!("{remaining} = sub i64 {jw_hi}, {j0}"));
        let partial = self.tmp();
        self.line(format!(
            "{partial} = icmp ult i64 {remaining}, {}",
            ctx.tile_j
        ));
        let tj = self.tmp();
        self.line(format!(
            "{tj} = select i1 {partial}, i64 {remaining}, i64 {}",
            ctx.tile_j
        ));
        self.emit_tile_trio(site, ctx, &j0, row0, a_rows, b_row, &tj, false, None, None);
        self.line(format!("br label %{j_done}"));
        self.label_line(&j_done);
    }

    /// The seed-splat / k-loop / store lane-loop trio for one j-tile of an
    /// `a_rows.len()`-row block; subrow r's accumulators live at acc offset
    /// `r*TILE_J + lane`. `bound` is the lane trip count — the literal
    /// `TILE_J` on the main path, the runtime `tj` on the remainder path.
    /// `a_rows[r]` is subrow r's hoisted `a.base + a.ci*(i+r)` (None ⇒ 0),
    /// `b_row` the hoisted `b.base + b.ci*i`, `row0` the block's first row
    /// times C. The k loop bound stays the constant `site.k`; per k the body
    /// does one scalar a-load per subrow and ONE b load per lane, reused
    /// across every subrow's `mul`/`add` accumulator update.
    #[allow(clippy::too_many_arguments)]
    fn emit_tile_trio(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        j0: &str,
        row0: &str,
        a_rows: &[Option<String>],
        b_row: &Option<String>,
        bound: &str,
        main: bool,
        panel_base: Option<&str>,
        panel_lane0: Option<&str>,
    ) {
        let rows = a_rows.len() as u64;

        // plan-s30: the constant-width main tile carries its accumulators as
        // `<TJ x elem>` SSA phis instead of the acc scratch. Gated to exactly
        // the ×2-unrolled body (`main && rows == ctx.tile_i`, which is the only
        // caller shape whose lane count is the compile-time `TILE_J`) — every
        // remainder tile, boundary row, TI=1 rung and runtime-`tj` tile keeps
        // the memory form byte for byte, which is the negative control.
        if main
            && rows == ctx.tile_i
            && bound.parse::<u64>().ok() == Some(ctx.tile_j)
            && panel_lane0.is_none()
            && (ctx.packed.is_none() || panel_base.is_some())
        {
            self.emit_tile_trio_vec(site, ctx, j0, row0, a_rows, b_row, panel_base);
            return;
        }

        // Seed splat: one lane loop per subrow — subrow r's lanes are at acc
        // offset r*TILE_J + lane, so a flat rows*bound range would leave the
        // strided remainder lanes (bound < TILE_J) of subrows > 0 unseeded.
        for r in 0..rows {
            let (seed_head, seed_body, seed_done) = (self.label(), self.label(), self.label());
            self.line(format!("store i64 0, ptr {}", ctx.lane_ctr));
            self.line(format!("br label %{seed_head}"));
            self.label_line(&seed_head);
            let seed_lane = self.tmp();
            self.line(format!("{seed_lane} = load i64, ptr {}", ctx.lane_ctr));
            let seed_done_cond = self.tmp();
            self.line(format!(
                "{seed_done_cond} = icmp uge i64 {seed_lane}, {bound}"
            ));
            self.line(format!(
                "br i1 {seed_done_cond}, label %{seed_done}, label %{seed_body}"
            ));
            self.label_line(&seed_body);
            let acc_lane = if r == 0 {
                seed_lane.clone()
            } else {
                let offset = self.tmp();
                self.line(format!(
                    "{offset} = add i64 {seed_lane}, {}",
                    r * ctx.tile_j
                ));
                offset
            };
            let seed_ptr = self.tmp();
            self.line(format!(
                "{seed_ptr} = getelementptr {}, ptr {}, i64 0, i64 {acc_lane}",
                ctx.acc_llt, ctx.acc
            ));
            self.line(format!(
                "store {} {}, ptr {seed_ptr}",
                ctx.elem_llt, ctx.seed
            ));
            let seed_lane_next = self.tmp();
            self.line(format!("{seed_lane_next} = add i64 {seed_lane}, 1"));
            self.line(format!("store i64 {seed_lane_next}, ptr {}", ctx.lane_ctr));
            self.line(format!("br label %{seed_head}"));
            self.label_line(&seed_done);
        }

        // Only the full TI-blocked, constant-width body unrolls k. Boundary,
        // tail-row, and remainder bodies retain the single-k loop.
        let unroll = main && rows == ctx.tile_i;
        let (k_head, k_body, k_done) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 0, ptr {}", ctx.k_ctr));
        self.line(format!("br label %{k_head}"));
        self.label_line(&k_head);
        let kk = self.tmp();
        self.line(format!("{kk} = load i64, ptr {}", ctx.k_ctr));
        if unroll {
            let (k_tail_check, k_tail) = (self.label(), self.label());
            let kk1 = self.tmp();
            self.line(format!("{kk1} = add i64 {kk}, 1"));
            let pair = self.tmp();
            self.line(format!("{pair} = icmp ult i64 {kk1}, {}", site.k));
            self.line(format!(
                "br i1 {pair}, label %{k_body}, label %{k_tail_check}"
            ));
            self.label_line(&k_body);
            let a0 = self.emit_tile_a_values(site, ctx, a_rows, &kk);
            let a1 = self.emit_tile_a_values(site, ctx, a_rows, &kk1);
            if let (Some(packed), Some(panel_base)) = (&ctx.packed, panel_base) {
                let next_k = self.tmp();
                self.line(format!("{next_k} = add i64 {kk}, 2"));
                let next_offset = self.tmp();
                self.line(format!("{next_offset} = mul i64 {next_k}, {}", ctx.tile_j));
                let next_index = self.tmp();
                self.line(format!(
                    "{next_index} = add i64 {panel_base}, {next_offset}"
                ));
                let next_ptr = self.tmp();
                self.line(format!(
                    "{next_ptr} = getelementptr {}, ptr {}, i64 0, i64 {next_index}",
                    packed.llt, packed.ptr
                ));
                self.line(format!(
                    "call void @llvm.prefetch.p0(ptr {next_ptr}, i32 0, i32 3, i32 1)"
                ));
            }
            self.emit_tile_lane_loop(
                site,
                ctx,
                j0,
                b_row,
                bound,
                panel_base,
                panel_lane0,
                None,
                ctx.tile_j,
                &[(&kk, a0), (&kk1, a1)],
            );
            let kk2 = self.tmp();
            self.line(format!("{kk2} = add i64 {kk}, 2"));
            self.line(format!("store i64 {kk2}, ptr {}", ctx.k_ctr));
            self.line(format!("br label %{k_head}"));

            self.label_line(&k_tail_check);
            let tail = self.tmp();
            self.line(format!("{tail} = icmp ult i64 {kk}, {}", site.k));
            self.line(format!("br i1 {tail}, label %{k_tail}, label %{k_done}"));
            self.label_line(&k_tail);
            let a = self.emit_tile_a_values(site, ctx, a_rows, &kk);
            self.emit_tile_lane_loop(
                site,
                ctx,
                j0,
                b_row,
                bound,
                panel_base,
                panel_lane0,
                None,
                ctx.tile_j,
                &[(&kk, a)],
            );
            self.line(format!("br label %{k_done}"));
        } else {
            let depth_done = self.tmp();
            self.line(format!("{depth_done} = icmp uge i64 {kk}, {}", site.k));
            self.line(format!(
                "br i1 {depth_done}, label %{k_done}, label %{k_body}"
            ));
            self.label_line(&k_body);
            let a = self.emit_tile_a_values(site, ctx, a_rows, &kk);
            self.emit_tile_lane_loop(
                site,
                ctx,
                j0,
                b_row,
                bound,
                panel_base,
                panel_lane0,
                None,
                ctx.tile_j,
                &[(&kk, a)],
            );
            let kk_next = self.tmp();
            self.line(format!("{kk_next} = add i64 {kk}, 1"));
            self.line(format!("store i64 {kk_next}, ptr {}", ctx.k_ctr));
            self.line(format!("br label %{k_head}"));
        }
        self.label_line(&k_done);

        // Store: one lane loop per subrow at out[(i+r)*C + j0 + lane].
        let out_start = self.tmp();
        self.line(format!("{out_start} = add i64 {row0}, {j0}"));
        for r in 0..rows {
            let out_start_r = if r == 0 {
                out_start.clone()
            } else {
                let shifted = self.tmp();
                self.line(format!("{shifted} = add i64 {out_start}, {}", r * site.c));
                shifted
            };
            let (store_head, store_body, store_done) = (self.label(), self.label(), self.label());
            self.line(format!("store i64 0, ptr {}", ctx.lane_ctr));
            self.line(format!("br label %{store_head}"));
            self.label_line(&store_head);
            let store_lane = self.tmp();
            self.line(format!("{store_lane} = load i64, ptr {}", ctx.lane_ctr));
            let stores_done = self.tmp();
            self.line(format!(
                "{stores_done} = icmp uge i64 {store_lane}, {bound}"
            ));
            self.line(format!(
                "br i1 {stores_done}, label %{store_done}, label %{store_body}"
            ));
            self.label_line(&store_body);
            let acc_lane = if r == 0 {
                store_lane.clone()
            } else {
                let offset = self.tmp();
                self.line(format!(
                    "{offset} = add i64 {store_lane}, {}",
                    r * ctx.tile_j
                ));
                offset
            };
            let final_acc_ptr = self.tmp();
            self.line(format!(
                "{final_acc_ptr} = getelementptr {}, ptr {}, i64 0, i64 {acc_lane}",
                ctx.acc_llt, ctx.acc
            ));
            let final_value = self.tmp();
            self.line(format!(
                "{final_value} = load {}, ptr {final_acc_ptr}",
                ctx.elem_llt
            ));
            let out_index = self.tmp();
            self.line(format!("{out_index} = add i64 {out_start_r}, {store_lane}"));
            let out_elem_ptr = self.tmp();
            self.line(format!(
                "{out_elem_ptr} = getelementptr {}, ptr {}, i64 0, i64 {out_index}",
                ctx.out_llt, ctx.out_ptr
            ));
            self.line(format!(
                "store {} {final_value}, ptr {out_elem_ptr}",
                ctx.elem_llt
            ));
            let store_lane_next = self.tmp();
            self.line(format!("{store_lane_next} = add i64 {store_lane}, 1"));
            self.line(format!("store i64 {store_lane_next}, ptr {}", ctx.lane_ctr));
            self.line(format!("br label %{store_head}"));
            self.label_line(&store_done);
        }
    }

    fn emit_tile_a_values(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        a_rows: &[Option<String>],
        k: &str,
    ) -> Vec<String> {
        a_rows
            .iter()
            .map(|a_row| {
                let index = self
                    .emit_tile_index(a_row.clone(), &[(site.a.ck, k)])
                    .unwrap_or_else(|| "0".to_owned());
                let ptr = self.tmp();
                self.line(format!(
                    "{ptr} = getelementptr {}, ptr {}, i64 0, i64 {index}",
                    ctx.a_llt, ctx.a_ptr
                ));
                let value = self.tmp();
                self.line(format!("{value} = load {}, ptr {ptr}", ctx.elem_llt));
                value
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_tile_lane_loop(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        j0: &str,
        b_row: &Option<String>,
        bound: &str,
        panel_base: Option<&str>,
        panel_lane0: Option<&str>,
        acc_base: Option<&str>,
        acc_row_stride: u64,
        steps: &[(&str, Vec<String>)],
    ) {
        let (head, body, done) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 0, ptr {}", ctx.lane_ctr));
        self.line(format!("br label %{head}"));
        self.label_line(&head);
        let lane = self.tmp();
        self.line(format!("{lane} = load i64, ptr {}", ctx.lane_ctr));
        let all_lanes = self.tmp();
        self.line(format!("{all_lanes} = icmp uge i64 {lane}, {bound}"));
        self.line(format!("br i1 {all_lanes}, label %{done}, label %{body}"));
        self.label_line(&body);
        for (k, a_values) in steps {
            let b_value =
                self.emit_tile_b_value(site, ctx, j0, b_row, &lane, k, panel_base, panel_lane0);
            for (r, a_value) in a_values.iter().enumerate() {
                let product = self.tmp();
                let (mul_lhs, mul_rhs) = if site.mul_a_first {
                    (a_value, &b_value)
                } else {
                    (&b_value, a_value)
                };
                self.line(format!(
                    "{product} = {}{} {} {mul_lhs}, {mul_rhs}",
                    ctx.mul_op, ctx.contract_flag, ctx.elem_llt
                ));
                let acc_lane = match (acc_base, r) {
                    (None, 0) => lane.clone(),
                    (None, _) => {
                        let offset = self.tmp();
                        self.line(format!(
                            "{offset} = add i64 {lane}, {}",
                            r as u64 * acc_row_stride
                        ));
                        offset
                    }
                    (Some(base), 0) => {
                        let offset = self.tmp();
                        self.line(format!("{offset} = add i64 {lane}, {base}"));
                        offset
                    }
                    (Some(base), _) => {
                        let based = self.tmp();
                        self.line(format!(
                            "{based} = add i64 {base}, {}",
                            r as u64 * acc_row_stride
                        ));
                        let offset = self.tmp();
                        self.line(format!("{offset} = add i64 {lane}, {based}"));
                        offset
                    }
                };
                let acc_ptr = self.tmp();
                self.line(format!(
                    "{acc_ptr} = getelementptr {}, ptr {}, i64 0, i64 {acc_lane}",
                    ctx.acc_llt, ctx.acc
                ));
                let acc_value = self.tmp();
                self.line(format!(
                    "{acc_value} = load {}, ptr {acc_ptr}",
                    ctx.elem_llt
                ));
                let sum = self.tmp();
                let (add_lhs, add_rhs) = if site.add_acc_first {
                    (&acc_value, &product)
                } else {
                    (&product, &acc_value)
                };
                self.line(format!(
                    "{sum} = {}{} {} {add_lhs}, {add_rhs}",
                    ctx.add_op, ctx.contract_flag, ctx.elem_llt
                ));
                self.line(format!("store {} {sum}, ptr {acc_ptr}", ctx.elem_llt));
            }
        }
        let lane_next = self.tmp();
        self.line(format!("{lane_next} = add i64 {lane}, 1"));
        self.line(format!("store i64 {lane_next}, ptr {}", ctx.lane_ctr));
        self.line(format!("br label %{head}"));
        self.label_line(&done);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_tile_b_value(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        j0: &str,
        b_row: &Option<String>,
        lane: &str,
        k: &str,
        panel_base: Option<&str>,
        panel_lane0: Option<&str>,
    ) -> String {
        let (llt, base, index) = if let Some(packed) = &ctx.packed {
            let (row, panel_lane) = if let Some(panel_base) = panel_base {
                let k_offset = self.tmp();
                self.line(format!("{k_offset} = mul i64 {k}, {}", ctx.tile_j));
                let row = self.tmp();
                self.line(format!("{row} = add i64 {panel_base}, {k_offset}"));
                let panel_lane = if let Some(panel_lane0) = panel_lane0 {
                    let panel_lane = self.tmp();
                    self.line(format!("{panel_lane} = add i64 {panel_lane0}, {lane}"));
                    panel_lane
                } else {
                    lane.to_owned()
                };
                (row, panel_lane)
            } else {
                let j = self.tmp();
                self.line(format!("{j} = add i64 {j0}, {lane}"));
                let jt = self.tmp();
                self.line(format!("{jt} = udiv i64 {j}, {}", ctx.tile_j));
                let panel_lane = self.tmp();
                self.line(format!("{panel_lane} = urem i64 {j}, {}", ctx.tile_j));
                let panel_base = self.tmp();
                self.line(format!(
                    "{panel_base} = mul i64 {jt}, {}",
                    site.k * ctx.tile_j
                ));
                let k_offset = self.tmp();
                self.line(format!("{k_offset} = mul i64 {k}, {}", ctx.tile_j));
                let row = self.tmp();
                self.line(format!("{row} = add i64 {panel_base}, {k_offset}"));
                (row, panel_lane)
            };
            let index = self.tmp();
            self.line(format!("{index} = add i64 {row}, {panel_lane}"));
            (packed.llt.as_str(), packed.ptr.as_str(), index)
        } else {
            let start = self
                .emit_tile_index(b_row.clone(), &[(site.b.ck, k), (1, j0)])
                .expect("tile b has lane term");
            let index = self.tmp();
            self.line(format!("{index} = add i64 {start}, {lane}"));
            (ctx.b_llt.as_str(), ctx.b_ptr.as_str(), index)
        };
        let ptr = self.tmp();
        self.line(format!(
            "{ptr} = getelementptr {llt}, ptr {base}, i64 0, i64 {index}"
        ));
        let value = self.tmp();
        self.line(format!("{value} = load {}, ptr {ptr}", ctx.elem_llt));
        value
    }

    /// A `<TJ x elem>` broadcast of one scalar: `insertelement` into `poison`
    /// then a zeroinitializer `shufflevector` — LLVM's canonical splat, which
    /// instcombine folds to a constant vector when the scalar is one.
    fn emit_vec_splat(&mut self, ctx: &TileCtx, scalar: &str) -> String {
        self.emit_splat(&ctx.elem_llt, ctx.tile_j, scalar)
    }

    /// [`Self::emit_vec_splat`] over the two fields it actually needs, so the
    /// conv rung (its own context type, same accumulator shape) shares it.
    fn emit_splat(&mut self, elem_llt: &str, tile_j: u64, scalar: &str) -> String {
        let vllt = vec_llt(elem_llt, tile_j);
        let one = self.tmp();
        self.line(format!(
            "{one} = insertelement {vllt} poison, {elem_llt} {scalar}, i64 0"
        ));
        let all = self.tmp();
        self.line(format!(
            "{all} = shufflevector {vllt} {one}, {vllt} poison, <{tile_j} x i32> zeroinitializer"
        ));
        all
    }

    /// The k step's b operand as ONE contiguous `<TJ x elem>` load. Both
    /// sources are lane-contiguous by construction: the packed panel is
    /// j-tile-major (lanes 0..TJ sit at `panel_base + k*TJ`) and the unpacked
    /// `b` carries lane coefficient 1. Composition rule 3: the load takes the
    /// **element** alignment, never the vector type's ABI alignment — `j0` is
    /// arbitrary and `<16 x float>` would claim 64.
    fn emit_tile_b_vector(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        j0: &str,
        b_row: &Option<String>,
        k: &str,
        panel_base: Option<&str>,
    ) -> String {
        let (llt, base, index) = if let Some(packed) = &ctx.packed {
            // A packed site addresses the panel, never `b` — the gate on this
            // path guarantees the hoisted base is there.
            let panel_base = panel_base.expect("vector tile b needs a hoisted panel base");
            let k_offset = self.tmp();
            self.line(format!("{k_offset} = mul i64 {k}, {}", ctx.tile_j));
            let row = self.tmp();
            self.line(format!("{row} = add i64 {panel_base}, {k_offset}"));
            (packed.llt.as_str(), packed.ptr.as_str(), row)
        } else {
            let start = self
                .emit_tile_index(b_row.clone(), &[(site.b.ck, k), (1, j0)])
                .expect("tile b has lane term");
            (ctx.b_llt.as_str(), ctx.b_ptr.as_str(), start)
        };
        let ptr = self.tmp();
        self.line(format!(
            "{ptr} = getelementptr {llt}, ptr {base}, i64 0, i64 {index}"
        ));
        let value = self.tmp();
        self.line(format!(
            "{value} = load {}, ptr {ptr}, align {}",
            tile_vec_llt(ctx),
            llt_align(&ctx.elem_llt)
        ));
        value
    }

    /// One k step over the vector accumulators: the same per-subrow scalar `a`
    /// loads as the memory path (splatted), the shared b vector, and one
    /// `fmul`/`fadd` pair per subrow in the recorded operand order. SIMD lanes
    /// are independent, so lane j of the result is exactly the scalar chain's
    /// value for lane j — bit-exact, not approximate. `out_names`, when given,
    /// forces the sums onto pre-minted names so the header phi can name them.
    #[allow(clippy::too_many_arguments)]
    fn emit_tile_vec_step(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        j0: &str,
        a_rows: &[Option<String>],
        b_row: &Option<String>,
        panel_base: Option<&str>,
        apack: Option<(&PackedBuffer, &str)>,
        k: &str,
        accs: &[String],
        out_names: Option<&[String]>,
    ) -> Vec<String> {
        let vllt = tile_vec_llt(ctx);
        let a_scalars = match apack {
            Some((apack, k_lo)) => {
                let koff = self.tmp();
                self.line(format!("{koff} = sub i64 {k}, {k_lo}"));
                self.emit_tile_kc_a_values(ctx, apack, accs.len() as u64, &koff)
            }
            None => self.emit_tile_a_values(site, ctx, a_rows, k),
        };
        let b = self.emit_tile_b_vector(site, ctx, j0, b_row, k, panel_base);
        let mut next = Vec::with_capacity(accs.len());
        for (r, a) in a_scalars.iter().enumerate() {
            let a_vec = self.emit_vec_splat(ctx, a);
            let product = self.tmp();
            let (mul_lhs, mul_rhs) = if site.mul_a_first {
                (&a_vec, &b)
            } else {
                (&b, &a_vec)
            };
            self.line(format!(
                "{product} = {}{} {vllt} {mul_lhs}, {mul_rhs}",
                ctx.mul_op, ctx.contract_flag
            ));
            let sum = match out_names {
                Some(names) => names[r].clone(),
                None => self.tmp(),
            };
            let (add_lhs, add_rhs) = if site.add_acc_first {
                (&accs[r], &product)
            } else {
                (&product, &accs[r])
            };
            self.line(format!(
                "{sum} = {}{} {vllt} {add_lhs}, {add_rhs}",
                ctx.add_op, ctx.contract_flag
            ));
            next.push(sum);
        }
        next
    }

    /// The ×2-unrolled k loop with the accumulators carried by `phi` instead of
    /// by memory: `seeds` enter at the preheader edge, each body iteration
    /// threads every accumulator through both k steps, and the exit block
    /// merges the paired and odd-tail values. No `alloca`, no `getelementptr`,
    /// no `load`/`store` of accumulator state anywhere inside the loop — the
    /// register form is what we emit rather than what LICM might grant us.
    #[allow(clippy::too_many_arguments)]
    fn emit_tile_vec_k_loop(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        j0: &str,
        a_rows: &[Option<String>],
        b_row: &Option<String>,
        panel_base: Option<&str>,
        apack: Option<(&PackedBuffer, &str)>,
        k_lo: &str,
        k_hi: &str,
        seeds: &[String],
    ) -> Vec<String> {
        let vllt = tile_vec_llt(ctx);
        let rows = seeds.len();
        // A named preheader: the header phis need a predecessor label, and the
        // emitter does not otherwise track the current block's name.
        let (pre, k_head, k_body, k_tail_check, k_tail, k_done) = (
            self.label(),
            self.label(),
            self.label(),
            self.label(),
            self.label(),
            self.label(),
        );
        self.line(format!("br label %{pre}"));
        self.label_line(&pre);
        self.line(format!("store i64 {k_lo}, ptr {}", ctx.k_ctr));
        self.line(format!("br label %{k_head}"));

        self.label_line(&k_head);
        // The latch values are named before the phis that reference them —
        // LLVM resolves forward references to named locals at parse time.
        let nexts: Vec<String> = (0..rows).map(|_| self.tmp()).collect();
        let accs: Vec<String> = (0..rows)
            .map(|r| {
                let acc = self.tmp();
                self.line(format!(
                    "{acc} = phi {vllt} [ {}, %{pre} ], [ {}, %{k_body} ]",
                    seeds[r], nexts[r]
                ));
                acc
            })
            .collect();
        let kk = self.tmp();
        self.line(format!("{kk} = load i64, ptr {}", ctx.k_ctr));
        let kk1 = self.tmp();
        self.line(format!("{kk1} = add i64 {kk}, 1"));
        let pair = self.tmp();
        self.line(format!("{pair} = icmp ult i64 {kk1}, {k_hi}"));
        self.line(format!(
            "br i1 {pair}, label %{k_body}, label %{k_tail_check}"
        ));

        self.label_line(&k_body);
        if let (Some(packed), Some(panel_base)) = (&ctx.packed, panel_base) {
            let next_k = self.tmp();
            self.line(format!("{next_k} = add i64 {kk}, 2"));
            let next_offset = self.tmp();
            self.line(format!("{next_offset} = mul i64 {next_k}, {}", ctx.tile_j));
            let next_index = self.tmp();
            self.line(format!(
                "{next_index} = add i64 {panel_base}, {next_offset}"
            ));
            let next_ptr = self.tmp();
            self.line(format!(
                "{next_ptr} = getelementptr {}, ptr {}, i64 0, i64 {next_index}",
                packed.llt, packed.ptr
            ));
            self.line(format!(
                "call void @llvm.prefetch.p0(ptr {next_ptr}, i32 0, i32 3, i32 1)"
            ));
        }
        let mid = self.emit_tile_vec_step(
            site, ctx, j0, a_rows, b_row, panel_base, apack, &kk, &accs, None,
        );
        self.emit_tile_vec_step(
            site,
            ctx,
            j0,
            a_rows,
            b_row,
            panel_base,
            apack,
            &kk1,
            &mid,
            Some(&nexts),
        );
        let kk2 = self.tmp();
        self.line(format!("{kk2} = add i64 {kk}, 2"));
        self.line(format!("store i64 {kk2}, ptr {}", ctx.k_ctr));
        self.line(format!("br label %{k_head}"));

        self.label_line(&k_tail_check);
        let tail = self.tmp();
        self.line(format!("{tail} = icmp ult i64 {kk}, {k_hi}"));
        self.line(format!("br i1 {tail}, label %{k_tail}, label %{k_done}"));
        self.label_line(&k_tail);
        let tail_accs = self.emit_tile_vec_step(
            site, ctx, j0, a_rows, b_row, panel_base, apack, &kk, &accs, None,
        );
        self.line(format!("br label %{k_done}"));

        self.label_line(&k_done);
        (0..rows)
            .map(|r| {
                let out = self.tmp();
                self.line(format!(
                    "{out} = phi {vllt} [ {}, %{k_tail_check} ], [ {}, %{k_tail} ]",
                    accs[r], tail_accs[r]
                ));
                out
            })
            .collect()
    }

    /// The `out[(i+r)*C + j0]` element pointers of a vector tile, one per
    /// subrow — the address of a whole `<TJ x elem>` lane run.
    fn emit_tile_vec_out_ptrs(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        out_start: &str,
        rows: u64,
    ) -> Vec<String> {
        (0..rows)
            .map(|r| {
                let index = if r == 0 {
                    out_start.to_owned()
                } else {
                    let shifted = self.tmp();
                    self.line(format!("{shifted} = add i64 {out_start}, {}", r * site.c));
                    shifted
                };
                let ptr = self.tmp();
                self.line(format!(
                    "{ptr} = getelementptr {}, ptr {}, i64 0, i64 {index}",
                    ctx.out_llt, ctx.out_ptr
                ));
                ptr
            })
            .collect()
    }

    /// The j-outer nest's vector main tile: seed splat → phi-carried k loop →
    /// one `<TJ x elem>` store per subrow. The scalar trio's seed/store lane
    /// loops collapse into the splat and the stores; the acc scratch is not
    /// touched at all (it stays allocated for the remainder tiles that still
    /// use it).
    #[allow(clippy::too_many_arguments)]
    fn emit_tile_trio_vec(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        j0: &str,
        row0: &str,
        a_rows: &[Option<String>],
        b_row: &Option<String>,
        panel_base: Option<&str>,
    ) {
        let vllt = tile_vec_llt(ctx);
        let align = llt_align(&ctx.elem_llt);
        let seed = self.emit_vec_splat(ctx, &ctx.seed);
        let seeds = vec![seed; a_rows.len()];
        let accs = self.emit_tile_vec_k_loop(
            site,
            ctx,
            j0,
            a_rows,
            b_row,
            panel_base,
            None,
            "0",
            &site.k.to_string(),
            &seeds,
        );
        let out_start = self.tmp();
        self.line(format!("{out_start} = add i64 {row0}, {j0}"));
        let ptrs = self.emit_tile_vec_out_ptrs(site, ctx, &out_start, accs.len() as u64);
        for (ptr, acc) in ptrs.iter().zip(&accs) {
            self.line(format!("store {vllt} {acc}, ptr {ptr}, align {align}"));
        }
    }

    /// The KC nest's vector main tile. The panel's partial sums still live in
    /// `out` between panels — the peeled kc==0 panel seeds, later panels reload
    /// — but reload and park are now one vector load/store per subrow outside
    /// the k loop instead of lane loops through the acc scratch, which is the
    /// aliasing that stopped LICM promoting it (s29.md §1).
    #[allow(clippy::too_many_arguments)]
    fn emit_tile_kc_trio_vec(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        apack: &PackedBuffer,
        j0: &str,
        row0: &str,
        rows: u64,
        panel_base: &str,
        k_lo: &str,
        k_hi: &str,
        first: bool,
    ) {
        let vllt = tile_vec_llt(ctx);
        let align = llt_align(&ctx.elem_llt);
        let out_start = self.tmp();
        self.line(format!("{out_start} = add i64 {row0}, {j0}"));
        let ptrs = self.emit_tile_vec_out_ptrs(site, ctx, &out_start, rows);
        let seeds: Vec<String> = if first {
            let seed = self.emit_vec_splat(ctx, &ctx.seed);
            vec![seed; rows as usize]
        } else {
            ptrs.iter()
                .map(|ptr| {
                    let value = self.tmp();
                    self.line(format!("{value} = load {vllt}, ptr {ptr}, align {align}"));
                    value
                })
                .collect()
        };
        let accs = self.emit_tile_vec_k_loop(
            site,
            ctx,
            j0,
            &[],
            &None,
            Some(panel_base),
            Some((apack, k_lo)),
            k_lo,
            k_hi,
            &seeds,
        );
        for (ptr, acc) in ptrs.iter().zip(&accs) {
            self.line(format!("store {vllt} {acc}, ptr {ptr}, align {align}"));
        }
    }

    fn emit_map(
        &mut self,
        m: MorphismId,
        source: ObjectId,
        target: ObjectId,
        body: FuncId,
        captures: u32,
    ) {
        if let Some(site) = self
            .tile_plan
            .as_ref()
            .and_then(|plan| plan.sites.get(m))
            // Non-conv k-split sites keep the untiled body-call fallback
            // (rule 3): the affine tile emission ignores `ksplit` and would
            // compute wrong addresses. Conv-shaped k-split sites pass through
            // to the unrolled micro-kernel.
            .filter(|site| (site.a.ksplit.is_none() && site.b.ksplit.is_none()) || conv_site(site))
            .cloned()
        {
            let packed = if self.packing && packing_site(&site) {
                let needs_pack = self
                    .frame
                    .as_ref()
                    .and_then(|frame| frame.packed.get(m))
                    .is_none();
                let packed = self.packed_buffer(m, &site);
                if needs_pack {
                    self.emit_pack_copy(source, &site, &packed);
                }
                Some(packed)
            } else {
                None
            };
            self.emit_tiled_map(source, target, &site, packed);
            return;
        }

        let src_ty = self.obj_ty(source);
        // The mapped array: the bare source (k=0) or the source product's last
        // component (k>0 — ADR-0027: source `(c₁…cₖ, [T; n])`, captures leading).
        // The pointer is taken LAZILY: an elided array (step 3b) has no
        // `%Frame` field, so asking for its slot would panic — and the whole
        // point is that this path never needs it.
        let (arr_ty, arr_slot) = if captures == 0 {
            (src_ty, None)
        } else {
            (
                src_ty.component_ty(captures).cloned().expect("map array"),
                Some(captures),
            )
        };
        let (tllt, n) = array_parts(&arr_ty);
        let tgt_ty = self.obj_ty(target);
        let (ullt, _) = array_parts(&tgt_ty);
        let tgt_arr_llt = lower_ty(&tgt_ty).expect("map tgt lowers");
        let tgt_slot = self.slot(target).expect("map tgt slot");
        let callee = self.fnames[body].clone();
        let ctr = self.scratch("i64");
        let (lo, hi) = self.bulk_bounds(n);

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {lo}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {hi}"));
        self.line(format!("br i1 {done}, label %{ld}, label %{lb}"));
        self.label_line(&lb);
        // plan-s37-stage-structure: if `elem_plan` knows what `arr[i]` IS, build
        // it here instead of reading it back out of memory. The intermediate
        // array is still emitted — this is the query, not a rewrite; whether the
        // buffer survives is a separate (backend-owned) decision. `None` keeps
        // the load, which is what every case did before and is always correct.
        let mapped = if captures == 0 {
            Some(source)
        } else {
            self.pair_source(source, captures)
        };
        let law = mapped.and_then(|o| self.elem.src(o)).cloned();
        let inlined = law
            .filter(|l| !matches!(l, ElemSrc::Load { .. }))
            .zip(arr_ty.component_ty(0).cloned())
            .and_then(|(l, elem_ty)| self.emit_elem(&l, &elem_ty, &iv));
        let e = match inlined {
            Some((_, v)) => v,
            None => {
                let src_arr_llt = lower_ty(&arr_ty).expect("map src lowers");
                let arr_ptr = self
                    .array_operand_ptr(source, arr_slot)
                    .expect("map src slot");
                let ep = self.tmp();
                self.line(format!(
                    "{ep} = getelementptr {src_arr_llt}, ptr {arr_ptr}, i64 0, i64 {iv}"
                ));
                let e = self.tmp();
                self.line(format!("{e} = load {tllt}, ptr {ep}"));
                e
            }
        };
        // The body call's argument: the bare element (k=0) or the assembled
        // `(c₁…cₖ, elem)` product (k>0), per the body fn's input ty — with
        // Array captures by reference, matching the body fn's signature.
        let arg = if captures == 0 {
            format!("{tllt} {e}")
        } else {
            let arg_ty = self.obj_ty(self.ir.func(body).expect("map body").input);
            let arg_llt = lower_body_input_ty(&arg_ty, captures).expect("map body input lowers");
            self.body_call_arg(source, captures, &arg_ty, &arg_llt, &[(&tllt, &e)])
        };
        let r = self.tmp();
        self.line(format!("{r} = call {ullt} @{callee}({arg})"));
        let dp = self.tmp();
        self.line(format!(
            "{dp} = getelementptr {tgt_arr_llt}, ptr {tgt_slot}, i64 0, i64 {iv}"
        ));
        self.line(format!("store {ullt} {r}, ptr {dp}"));
        let iv1 = self.tmp();
        self.line(format!("{iv1} = add i64 {iv}, 1"));
        self.line(format!("store i64 {iv1}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&ld);
    }

    fn emit_fold(&mut self, source: ObjectId, target: ObjectId, body: FuncId, captures: u32) {
        let src_ty = self.obj_ty(source);
        // ADR-0027: source `(c₁…cₖ, Acc, [T; n])` (k=0: `(Acc, [T; n])`) — the
        // accumulator is component k, the folded array component k+1.
        let arr_ty = src_ty
            .component_ty(captures + 1)
            .cloned()
            .expect("fold array");
        let (tllt, n) = array_parts(&arr_ty);
        let arr_llt = lower_ty(&arr_ty).expect("fold array lowers");
        let (acc_llt, acc0) = self.load_component(source, captures).expect("fold acc");
        let arr_ptr = self
            .array_operand_ptr(source, Some(captures + 1))
            .expect("fold array ptr");

        let callee = self.fnames[body].clone();
        let pair_ty = self.obj_ty(self.ir.func(body).expect("fold body").input);
        let pair_llt = lower_body_input_ty(&pair_ty, captures).expect("fold pair lowers");

        let accslot = self.scratch(&acc_llt);
        let ctr = self.scratch("i64");
        let (lo, hi) = self.bulk_bounds(n);
        self.line(format!("store {acc_llt} {acc0}, ptr {accslot}"));
        self.line(format!("store i64 {lo}, ptr {ctr}"));

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {hi}"));
        self.line(format!("br i1 {done}, label %{ld}, label %{lb}"));
        self.label_line(&lb);
        // Same element-law consumption as `emit_map` (plan-s37-stage-structure):
        // a fold over an `iota`/`fill`/`zip` reads the law, not the array. The
        // accumulator chain is untouched — order and arity are exactly as
        // before, so the fold's value semantics cannot move.
        let folded = self.pair_source(source, captures + 1);
        let law = folded.and_then(|o| self.elem.src(o)).cloned();
        let inlined = law
            .filter(|l| !matches!(l, ElemSrc::Load { .. }))
            .zip(arr_ty.component_ty(0).cloned())
            .and_then(|(l, elem_ty)| self.emit_elem(&l, &elem_ty, &iv));
        let e = match inlined {
            Some((_, v)) => v,
            None => {
                let ep = self.tmp();
                self.line(format!(
                    "{ep} = getelementptr {arr_llt}, ptr {arr_ptr}, i64 0, i64 {iv}"
                ));
                let e = self.tmp();
                self.line(format!("{e} = load {tllt}, ptr {ep}"));
                e
            }
        };
        let a = self.tmp();
        self.line(format!("{a} = load {acc_llt}, ptr {accslot}"));
        // The step call's argument: the `(c₁…cₖ, acc, elem)` product (k=0:
        // `(acc, elem)`), assembled in scratch per the body fn's input ty.
        let arg = self.body_call_arg(
            source,
            captures,
            &pair_ty,
            &pair_llt,
            &[(&acc_llt, &a), (&tllt, &e)],
        );
        let na = self.tmp();
        self.line(format!("{na} = call {acc_llt} @{callee}({arg})"));
        self.line(format!("store {acc_llt} {na}, ptr {accslot}"));
        let iv1 = self.tmp();
        self.line(format!("{iv1} = add i64 {iv}, 1"));
        self.line(format!("store i64 {iv1}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&ld);
        let fin = self.tmp();
        self.line(format!("{fin} = load {acc_llt}, ptr {accslot}"));
        self.store_obj(target, &acc_llt, &fin);
    }

    fn emit_zip(&mut self, source: ObjectId, target: ObjectId) {
        let src_ty = self.obj_ty(source);
        let a_ty = src_ty.component_ty(0).cloned().expect("zip a");
        let b_ty = src_ty.component_ty(1).cloned().expect("zip b");
        let (allt, n) = array_parts(&a_ty);
        let (bllt, _) = array_parts(&b_ty);
        let a_arr_llt = lower_ty(&a_ty).expect("zip a lowers");
        let b_arr_llt = lower_ty(&b_ty).expect("zip b lowers");
        let a_ptr = self.array_operand_ptr(source, Some(0)).expect("zip a ptr");
        let b_ptr = self.array_operand_ptr(source, Some(1)).expect("zip b ptr");

        let tgt_ty = self.obj_ty(target);
        let elem_ty = tgt_ty.component_ty(0).cloned().expect("zip elem");
        let tgt_arr_llt = lower_ty(&tgt_ty).expect("zip tgt lowers");
        let tgt_slot = self.slot(target).expect("zip tgt slot");
        let ctr = self.scratch("i64");
        let (lo, hi) = self.bulk_bounds(n);

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {lo}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {hi}"));
        self.line(format!("br i1 {done}, label %{ld}, label %{lb}"));
        self.label_line(&lb);
        let ea = {
            let p = self.tmp();
            self.line(format!(
                "{p} = getelementptr {a_arr_llt}, ptr {a_ptr}, i64 0, i64 {iv}"
            ));
            let v = self.tmp();
            self.line(format!("{v} = load {allt}, ptr {p}"));
            v
        };
        let eb = {
            let p = self.tmp();
            self.line(format!(
                "{p} = getelementptr {b_arr_llt}, ptr {b_ptr}, i64 0, i64 {iv}"
            ));
            let v = self.tmp();
            self.line(format!("{v} = load {bllt}, ptr {p}"));
            v
        };
        let dp = self.tmp();
        self.line(format!(
            "{dp} = getelementptr {tgt_arr_llt}, ptr {tgt_slot}, i64 0, i64 {iv}"
        ));
        let elem_llt = lower_ty(&elem_ty).expect("zip elem lowers");
        self.field_store(&dp, &elem_ty, &elem_llt, 0, &allt, &ea);
        self.field_store(&dp, &elem_ty, &elem_llt, 1, &bllt, &eb);
        let iv1 = self.tmp();
        self.line(format!("{iv1} = add i64 {iv}, 1"));
        self.line(format!("store i64 {iv1}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&ld);
    }

    fn emit_enumerate(&mut self, source: ObjectId, target: ObjectId) {
        let src_ty = self.obj_ty(source);
        let (allt, n) = array_parts(&src_ty);
        let src_arr_llt = lower_ty(&src_ty).expect("enum src lowers");
        let src_slot = self.array_operand_ptr(source, None).expect("enum src ptr");

        let tgt_ty = self.obj_ty(target);
        let elem_ty = tgt_ty.component_ty(0).cloned().expect("enum elem");
        let tgt_arr_llt = lower_ty(&tgt_ty).expect("enum tgt lowers");
        let tgt_slot = self.slot(target).expect("enum tgt slot");
        let ctr = self.scratch("i64");
        let (lo, hi) = self.bulk_bounds(n);

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {lo}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {hi}"));
        self.line(format!("br i1 {done}, label %{ld}, label %{lb}"));
        self.label_line(&lb);
        let idx32 = self.tmp();
        self.line(format!("{idx32} = trunc i64 {iv} to i32"));
        let ep = self.tmp();
        self.line(format!(
            "{ep} = getelementptr {src_arr_llt}, ptr {src_slot}, i64 0, i64 {iv}"
        ));
        let ea = self.tmp();
        self.line(format!("{ea} = load {allt}, ptr {ep}"));
        let dp = self.tmp();
        self.line(format!(
            "{dp} = getelementptr {tgt_arr_llt}, ptr {tgt_slot}, i64 0, i64 {iv}"
        ));
        let elem_llt = lower_ty(&elem_ty).expect("enum elem lowers");
        self.field_store(&dp, &elem_ty, &elem_llt, 0, "i32", &idx32);
        self.field_store(&dp, &elem_ty, &elem_llt, 1, &allt, &ea);
        let iv1 = self.tmp();
        self.line(format!("{iv1} = add i64 {iv}, 1"));
        self.line(format!("store i64 {iv1}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&ld);
    }

    /// `Iota` (ADR-0029): `out[i] = (i32)i`. The count is the (builder-minted)
    /// constant object; `n` rides the target type (validate ties them), so no
    /// source read is needed. Trap-free by construction.
    fn emit_iota(&mut self, _source: ObjectId, target: ObjectId) {
        let tgt_ty = self.obj_ty(target);
        let (_, n) = array_parts(&tgt_ty);
        let tgt_arr_llt = lower_ty(&tgt_ty).expect("iota tgt lowers");
        let tgt_slot = self.slot(target).expect("iota tgt slot");
        let ctr = self.scratch("i64");
        let (lo, hi) = self.bulk_bounds(n);

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {lo}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {hi}"));
        self.line(format!("br i1 {done}, label %{ld}, label %{lb}"));
        self.label_line(&lb);
        let idx32 = self.tmp();
        self.line(format!("{idx32} = trunc i64 {iv} to i32"));
        let dp = self.tmp();
        self.line(format!(
            "{dp} = getelementptr {tgt_arr_llt}, ptr {tgt_slot}, i64 0, i64 {iv}"
        ));
        self.line(format!("store i32 {idx32}, ptr {dp}"));
        let iv1 = self.tmp();
        self.line(format!("{iv1} = add i64 {iv}, 1"));
        self.line(format!("store i64 {iv1}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&ld);
    }

    /// `Fill` (ADR-0029): `out[i] = x` — the internal (x, count) pair feeds
    /// the value; `n` rides the target type (validate ties them). Trap-free.
    fn emit_fill(&mut self, source: ObjectId, target: ObjectId) {
        let tgt_ty = self.obj_ty(target);
        let (_, n) = array_parts(&tgt_ty);
        let tgt_arr_llt = lower_ty(&tgt_ty).expect("fill tgt lowers");
        let tgt_slot = self.slot(target).expect("fill tgt slot");
        let (vllt, v) = self.load_component(source, 0).expect("fill value");
        let ctr = self.scratch("i64");
        let (lo, hi) = self.bulk_bounds(n);

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {lo}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {hi}"));
        self.line(format!("br i1 {done}, label %{ld}, label %{lb}"));
        self.label_line(&lb);
        let dp = self.tmp();
        self.line(format!(
            "{dp} = getelementptr {tgt_arr_llt}, ptr {tgt_slot}, i64 0, i64 {iv}"
        ));
        self.line(format!("store {vllt} {v}, ptr {dp}"));
        let iv1 = self.tmp();
        self.line(format!("{iv1} = add i64 {iv}, 1"));
        self.line(format!("store i64 {iv1}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&ld);
    }

    // --- loop-driver hooks (used by loops.rs) ----------------------------

    /// Copy object `from`'s whole value into object `to`'s slot (init→merge,
    /// next→merge). No-op if either side is erased.
    pub(crate) fn copy_obj(&mut self, from: ObjectId, to: ObjectId) {
        if matches!(self.obj_ty(from), Ty::Array { .. }) {
            let src = self
                .array_operand_ptr(from, None)
                .expect("array copy source ptr");
            let dst = self.slot(to).expect("array copy target slot");
            let llt = lower_ty(&self.obj_ty(from)).expect("array copy lowers");
            self.emit_memcpy(&dst, &src, &llt);
        } else if let Some((llt, val)) = self.load_whole(from) {
            self.store_obj(to, &llt, &val);
        }
    }

    /// Copy component `k` of aggregate `route` into object `to`'s slot
    /// (exit payload → exit object). No-op if erased.
    pub(crate) fn copy_component(&mut self, route: ObjectId, k: u32, to: ObjectId) {
        if matches!(self.obj_ty(route).component_ty(k), Some(Ty::Array { .. })) {
            let src = self
                .array_operand_ptr(route, Some(k))
                .expect("array route source ptr");
            let dst = self.slot(to).expect("array route target slot");
            let llt = lower_ty(&self.obj_ty(to)).expect("array route lowers");
            self.emit_memcpy(&dst, &src, &llt);
        } else if let Some((llt, val)) = self.load_component(route, k) {
            self.store_obj(to, &llt, &val);
        }
    }

    /// Load component `k` of `route` as a bare operand (the loop guard bool).
    pub(crate) fn load_route_component(&mut self, route: ObjectId, k: u32) -> String {
        self.load_component(route, k).expect("route component").1
    }
}

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
