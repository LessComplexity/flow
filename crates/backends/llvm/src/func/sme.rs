//! the ARM SME rung: one tile site realized on the streaming matrix unit
//!
//! **One cohesive unit, selected at one point** (plan-s41 §2.2 rule 2). The
//! whole realization is this file: the legality predicate
//! ([`FnEmit::sme_tile_site`]) and the emission
//! ([`FnEmit::emit_tiled_map_sme`]). `func/tile.rs` asks the predicate once and
//! calls the emitter; nothing else in the emitter knows SME exists — and
//! neither does `mapal-ir`, which supplies only the geometry record it already
//! supplied.
//!
//! The shape is the same one every other rung has: stage the operands, issue a
//! multiply-accumulate over a block, keep a **resident** accumulator hot across
//! the reduction axis, store once at the end. Only the leaf differs — here the
//! accumulator is a ZA tile and one issue is a `t × t` outer product instead of
//! a `1 × TJ` FMA, so the nest above it collapses from (i-block, j-tile, k, lane)
//! to (i-panel, j-panel) with the k reduction living inside the kernel.
//!
//! **Face.** `fmopa` fuses — measured, `benches/sme/README.md`: 92/256 cells
//! differ against separate `mul`+`add`, 0/256 against `fmaf`. So per ADR-0032
//! D1/D3 this rung is a **contract-face realization** and the predicate refuses
//! without `EmitOpts::contract`. Under the default exact face the NEON rung
//! runs, unchanged and bit-for-bit.

use super::*;
use crate::PanelWrite;

impl<'a> FnEmit<'a> {
    /// Is this tile site legal for, and shaped for, the SME rung?
    ///
    /// Everything it refuses falls back **silently** to today's dispatch, which
    /// is the same contract an unrecognized tile site already has. The clauses,
    /// in order:
    ///
    /// 1. **Capability** — the profile has a streaming matrix unit, and derives
    ///    a tile side at this element width. `None` for every profile that
    ///    existed before this rung, which is the byte-identity argument.
    /// 2. **Face** — `fmopa` fuses, so it may only fire where fusion is asked
    ///    for (ADR-0032 D1/D3).
    /// 3. **Width** — f32 only, deliberately (`benches/sme/run16.c` is the
    ///    verification, and it is f32).
    /// 4. **Panel alignment, NOT sequentiality.** An earlier version of this
    ///    predicate refused the parallel task path; it does not any more, and
    ///    the kernel runs inside `@taskN_slice`. What replaced that clause is
    ///    an obligation, so it is written here rather than left implicit: the
    ///    emitted i-loop starts at `lo/c` and steps whole `ti*t`-row panels
    ///    with **no partial-panel path**, so a slice that begins mid-panel
    ///    makes the final panel write past the end of the output. Clause 7
    ///    (`rows % (ti*t) == 0`) plus `slice_sizing` handing the runtime
    ///    `ti*t*c` as the quantum is what makes every slice boundary
    ///    panel-aligned and the `lo/c` division exact. `mapal-rt`'s
    ///    `slice_ranges` rounds the `MAPAL_SLICE` lever up to that quantum for
    ///    the same reason — before it did, a forced misaligned size segfaulted.
    /// 5. **Shape** — the matmul record: `a` invariant in the lane axis, `b`
    ///    invariant in the row axis and unit-stride along it, no k-split. This
    ///    is what makes `ap[k] ⊗ b[k]` the right outer product.
    /// 6. **Seed** — the kernel *stores* the ZA rows rather than accumulating
    ///    into `c`, so the fold's identity has to be a true zero.
    /// 7. **Extent** — whole panels only, and a panel is now the whole tile
    ///    block: `ti·t` rows by `tj·t` columns. A remainder is a real shape (it
    ///    is what predication is for) and it is not built: those sites fall
    ///    back.
    /// 8. **Panel width is a LOAD-WIDTH constraint, not an addressing
    ///    convenience.** When the NEON packing rung would have packed `b`, its
    ///    row is `tile_j` lanes wide and the kernel issues a `t`-wide contiguous
    ///    load. `tile_j < t` would read past a packed row into the next k row;
    ///    `tile_j > t` would silently skip the rest of each row. So the widths
    ///    must be equal — they are on this part (both 16), and where they are
    ///    not, fall back rather than mint a second layout. **The addressing no
    ///    longer depends on that coincidence** (see `emit_tiled_map_sme`'s b
    ///    geometry, which reads `tile_j` and `b.clane`); only the load width
    ///    does. Parameterising the pack width for SME — asking the packing rung
    ///    for a `t`-wide panel — is what retires this clause, and it is a
    ///    **portability** item, not a performance one: S42 measured that the rung
    ///    already takes the packed path on this profile (1.560× at 2048).
    pub(super) fn sme_tile_site(&self, site: &TileSite) -> bool {
        let (Some(t), Some((ti, tj))) = (
            self.profile.sme_tile_side(&site.elem),
            self.profile.sme_block(&site.elem),
        ) else {
            return false;
        };
        self.contract
            && site.elem == Ty::f32()
            && site.a.ksplit.is_none()
            && site.b.ksplit.is_none()
            && site.a.clane == 0
            && site.b.clane == 1
            && site.b.ci == 0
            && matches!(&site.seed, Value::F32(z) if z.to_bits() == 0)
            && site.rows.is_multiple_of(ti * t)
            && site.c.is_multiple_of(tj * t)
            && site.k >= t
            && (!(self.packing && packing_site(site)) || self.profile.tile_j(&site.elem) == t)
    }

    /// The panel height in output rows — `ti · t`, the quantum every i-grid
    /// step, the A pack and `slice_sizing` all have to agree on.
    pub(super) fn sme_panel_rows(&self, site: &TileSite) -> Option<u64> {
        Some(self.profile.sme_tile_side(&site.elem)? * self.profile.sme_block(&site.elem)?.0)
    }

    /// Emit the site as a grid of `ti·t × tj·t` streaming panels.
    ///
    /// ```text
    /// for i0 in (0..rows).step(ti·t):
    ///     pack ap[k][i] = a[base + ci·(i0+i) + ck·k]   -- ti·t × k, contiguous per k
    ///     for j0 in (0..c).step(tj·t):
    ///         mapal_sme_panel(ap, b-panel(j0), &out[i0·c + j0], bn, bj, c, k)
    /// ```
    ///
    /// The A pack is the one piece of staging the kernel cannot do for itself:
    /// `a` is strided by `ci` across the rows of a panel, and the outer product
    /// needs those `t` values contiguous. It is hoisted out of the j loop, so
    /// each element of `a` is read once per i-panel instead of once per panel —
    /// the same reason `emit_tile_packed_kc` packs it.
    ///
    /// **One pack of `ti·t` rows, not `ti` packs of `t`.** The kernel's `ti`
    /// A operands are then `ap + r·t` inside one buffer whose k stride is
    /// `ti·t` — one `alloca`, one pack loop, one pointer argument, and the same
    /// bytes touched either way. `ti` separate buffers would need `ti` extra
    /// parameters to say nothing new.
    ///
    /// `b` needs no staging when the packing rung already ran: its panel is
    /// already `[jt][k][lane]` with a contiguous `tile_j`-wide row, which at
    /// `tile_j == t` is exactly the vector the kernel loads. Unpacked, the
    /// kernel strides `b` by its row stride and reads it in place. The `tj`
    /// column blocks are `bj` apart — `t` in place, a whole packed panel
    /// (`t·k`) when packed, which is the one thing the two layouts disagree on.
    pub(super) fn emit_tiled_map_sme(
        &mut self,
        source: ObjectId,
        target: ObjectId,
        site: &TileSite,
        packed: Option<PackedBuffer>,
    ) {
        let t = self
            .profile
            .sme_tile_side(&site.elem)
            .expect("sme_tile_site gated this");
        let (ti, tj) = self
            .profile
            .sme_block(&site.elem)
            .expect("sme_tile_site gated this");
        let (panel_rows, panel_cols) = (ti * t, tj * t);
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
        debug_assert_eq!(array_parts(&out_ty).1, site.rows * site.c);
        let elem_llt = lower_ty(&site.elem).expect("tile element lowers");

        let a_ptr = self
            .array_operand_ptr(source, Some(site.a.slot))
            .expect("tile a ptr");
        let b_ptr = self
            .array_operand_ptr(source, Some(site.b.slot))
            .expect("tile b ptr");
        let out_ptr = self.slot(target).expect("tile output slot");

        // **KC blocking: how deep one k block is, and whether to block at all.**
        //
        // The depth is the profile's, derived from the matrix unit's own cache
        // budget rather than the NEON core's (`TargetProfile::sme_kc` — L1D, not
        // half of a shared L2, which would size this panel 8× too deep on this
        // part). Blocking fires only when the whole-k panel would actually
        // overflow that budget, and only when the split is exact — a ragged final
        // block is a real shape and it is not built, exactly like the row and
        // column remainders clause 7 refuses.
        //
        // **DEFAULT OFF, and the reason is measured, not cautious.** Riding
        // `kc_nest` for the same reason the NEON KC rung does (`EmitOpts::kc_nest`
        // — "measured a 3× LOSS"), because the integrated result contradicted the
        // standalone probe:
        //
        // | N | unblocked | blocked | |
        // | ---: | ---: | ---: | --- |
        // | 1024 | 1022 | **682** | GF/s — 0.67×, a 33% REGRESSION |
        // | 2048 | 978 | **647** | GF/s — 0.66×, a 34% REGRESSION |
        // | 4096 | 716 | **626** | GF/s — 0.87×, still a REGRESSION |
        //
        // (`benches/sme/sme_ab.sh`, 21 alternating runs, medians, values identical
        // at every size before any timing was read; commit `f01fb73`. The NEON leg
        // reproduced S41b throughout — 1237 ms vs 1286 at 4096 — which is what
        // makes the comparison trustworthy rather than a machine-state artifact.)
        //
        // **THE CAUSE IS NOT ESTABLISHED. Read the caveat before acting on this.**
        // Six candidate causes were measured and refuted (see the end of this
        // comment). Two earlier attributions written here were WRONG and are
        // retracted: "the accumulate read-out costs 85.8 ms" and "the blocked
        // kernel runs at 1598 GFLOP/s" both came from a probe that forced the
        // kernel's `K` argument to 1 — which shrinks the k loop but leaves the
        // full 16-row × 4-tile ZA read-out intact, so it measured the read-out as
        // if it were pack cost. Do not reuse that probe design.
        //
        // What IS solid: the numbers below, and that a hand-written kernel with
        // the same structure, same depth, same machine runs the blocked case in
        // **124.8 ms against this emitter's 225.9** — so the deficit is ours, not
        // the technique's. That is the open question.
        //
        // **RE-MEASURED AT THE CORRECT DEPTH (kc=1024).** Everything below the next
        // table was taken at kc=512, which a depth sweep later showed to be two
        // steps down a sharp curve — see `Sme::panel_l1d_ratio`. The corrected
        // numbers, 15 alternating runs, values identical at every cell:
        //
        // | N | config | KC off | KC on | | distributions |
        // | ---: | --- | ---: | ---: | ---: | --- |
        // | 2048 | 1 thread | 18.014 ms | 17.751 | +1.5% | overlap |
        // | 2048 | threaded | **6.783** | 7.779 | **−12.8%** | disjoint |
        // | 4096 | 1 thread | 171.179 | **161.360** | **+6.1%** | disjoint |
        // | 4096 | threaded | **53.485** | 71.796 | **−25.5%** | disjoint |
        //
        // So the depth fix turned the 1-thread case from a loss into a small win,
        // and left the threaded case a large loss. **That is why this stays OFF:**
        // enabling it would take threaded 4096 — the headline matmul cell — from
        // 53.5 ms to 71.8 ms.
        //
        // The explanation is consistent across all four cells. The A panel is
        // `ti·t × k` = 512 KB **regardless of thread count**, because slices are cut
        // on the `ti·t·c` quantum and a core still works one panel at a time. So
        // threaded, 14 cores hold ~7 MB of A panels plus ~4 MB of packed B — ~11 MB
        // inside a 16 MB L2, which fits. Blocking shrinks each panel to 128 KB but
        // adds 4× the `c` sweeps; at one thread there is spare bandwidth so the
        // cache win shows, and threaded 14 cores contend so the `c` traffic wins.
        // ⇒ **a one-thread-only optimization on this part.**
        //
        // The older kc=512 numbers are kept below only because the refutation list
        // that follows was gathered against them.
        //
        // N=4096, full pool, medians, AT kc=512 (superseded):
        //
        // | variant | GFLOP/s |
        // | --- | ---: |
        // | KC off | **2526** |
        // | KC on, accumulate | 1527 |
        // | KC on, accumulate REMOVED (wrong values, timing only) | 2158 |
        //
        // The "read-modify-write removed" row is a **timing probe with wrong
        // values** (every block stores, so only the last survives) and it forced
        // `K=1`, which leaves the full read-out intact — so it does NOT isolate
        // what its label suggests. Treat it as an upper bound on nothing in
        // particular; it is recorded only because it is what prompted the search.
        //
        // **Six candidate causes measured and REFUTED**, so nobody re-tests them:
        //
        // | candidate | verdict |
        // | --- | --- |
        // | the loop nest is wrong | no — verified index by index, work counts exact, values identical |
        // | the b layout (whole-k slice vs kc-deep repack) | **1.065×** (`benches/sme/bslice.c`) |
        // | the read-out CODE is bad | no — emitted asm is 4 instructions per tile, no spills |
        // | the streaming-mode ABI (`_body` transitions + d8–d15 spills) | **1.0 ms** over 131072 calls (`benches/sme/smcost.c`) |
        // | the pack's memory ORDER breaks under blocking | no — the same loops in C: 8.46 ms unblocked, 8.05 blocked (`benches/sme/packcost.c`) |
        // | the pack spills its row pointers (it did) | fixed above — scalar float loads 51 → 5 — worth only **3%** |
        //
        // **~100 ms of the blocked path is therefore UNEXPLAINED**, against a
        // hand-written kernel of the same structure that does it in 124.8 ms.
        //
        // THE RIGHT NEXT EXPERIMENT, before any further guessing: sweep the k-block
        // COUNT in the emitter (kc = 4096, 2048, 1024, 512, 256, where 4096 is the
        // unblocked case) and read the slope. Cost per k block falls straight out,
        // with no forced arguments and no wrong values. `benches/sme/kc.c` does
        // exactly that sweep standalone and gets a clean unimodal curve; the
        // emitter has only ever been tested at a single depth.
        //
        // The one lesson that IS established, and it is Sapir's rule 16:
        // `benches/sme/kc.c` measured **1101.2 GFLOP/s at kc=512, N=4096** against
        // 760.7 unblocked — a **1.448× gain** — standalone. Integrated it delivers
        // 0.79× at one thread and 0.60× threaded. *A standalone probe cannot settle
        // what an optimization is worth inside the real pipeline* — and here it
        // could not even locate the cause when the two were compared directly.
        //
        // The `>` gate is still right on its own terms — at N=512 the whole-k
        // panel already fits the budget and the probe measured blocking as a loss
        // there — but it is not sufficient: 1024 and 2048 also fit comfortably in
        // L2 and lose. Whatever replaces it has to be a measured threshold, not
        // this arithmetic one.
        let kc_budget = self
            .profile
            .sme_kc(&site.elem)
            .expect("sme_tile_site gated this");
        let blocked = self.kc_nest && site.k > kc_budget && site.k.is_multiple_of(kc_budget);
        let kc = if blocked { kc_budget } else { site.k };

        // The A panel: `ti·t` rows × **one k block**, laid out ap[k·ti·t + i].
        // Unblocked that is the whole k axis and this is what shipped before;
        // blocked it is `ti·t·kc·sizeof` = 128 KB at f32/2×2 regardless of k,
        // which is the point — it stops scaling with k and starts fitting the
        // unit's cache. (Discharges the `ponytail:` marker that predicted exactly
        // this upgrade.)
        let ap_llt = format!("[{} x {elem_llt}]", panel_rows * kc);
        let ap = format!("%s{}", self.fresh());
        self.entry_alloc(&ap, &ap_llt, Some(64));

        let i0_ctr = self.scratch("i64");
        let j0_ctr = self.scratch("i64");
        let pk_ctr = self.scratch("i64");
        let pi_ctr = self.scratch("i64");
        let k0_ctr = blocked.then(|| self.scratch("i64"));

        let (i_head, i_body, i_done) = (self.label(), self.label(), self.label());
        let (pk_head, pk_body, pk_done) = (self.label(), self.label(), self.label());
        let (pi_head, pi_body, pi_done) = (self.label(), self.label(), self.label());
        let (j_head, j_body, j_done) = (self.label(), self.label(), self.label());
        // The k-block loop, and the call-site diamond that picks the read-out.
        // `None` unblocked, so not one label is minted and the emission is
        // byte-identical to the pre-KC rung — the property that makes this
        // reviewable.
        let k_labels = blocked.then(|| {
            (
                self.label(), // k0_head
                self.label(), // k0_body
                self.label(), // k0_done
                self.label(), // call store arm
                self.label(), // call accumulate arm
                self.label(), // j tail (the two arms rejoin)
            )
        });

        // The task's row range. `bulk_bounds` is the SAME question the NEON
        // rungs ask (`func/tile.rs`, `conv.rs`, `window.rs`, `bulk.rs`); this
        // rung asks it too rather than assuming it owns the whole output.
        //
        // The division is EXACT, and that is a proof obligation this rung pays
        // for in `slice_sizing`: the runtime cuts slices on `slice_elems` as a
        // quantum (`mapal-rt`'s `slice_ranges`), `slice_sizing` hands it
        // `ti·t · c` whenever this rung fires, and `sme_tile_site` requires
        // `rows % (ti·t) == 0` — so `n = rows · c` is an exact multiple of the
        // quantum, every boundary is panel-aligned, there is no ragged final
        // slice, and no panel can straddle two tasks. Unsliced, `bulk_bounds`
        // yields the literals `0`/`n` and this folds away.
        let (lo, hi) = self.bulk_bounds(site.rows * site.c);
        let i_lo = self.tmp();
        self.line(format!("{i_lo} = udiv i64 {lo}, {}", site.c));
        let i_hi = self.tmp();
        self.line(format!("{i_hi} = udiv i64 {hi}, {}", site.c));

        // --- the k-block loop, outside the i loop.
        //
        // This order — k blocks outermost — is the one `benches/sme/kc.c`
        // measured, and it is chosen for that reason rather than derived. It
        // costs a full sweep of `out` per k block (read-modify-write), which is
        // why `PanelWrite::Accumulate` exists; the alternative orders either
        // re-pack A once per j panel (128× the pack work at N=4096) or keep the
        // whole k axis in one call, which is what we are trying to stop doing.
        // A depth of `kc` makes that trade pay: `kc` flops per output byte moved.
        let k0 = if let Some((k0_head, k0_body, _, _, _, _)) = &k_labels {
            let k0_ctr = k0_ctr.as_ref().expect("blocked ⇒ counter");
            self.line(format!("store i64 0, ptr {k0_ctr}"));
            self.line(format!("br label %{k0_head}"));
            self.label_line(k0_head);
            let k0 = self.tmp();
            self.line(format!("{k0} = load i64, ptr {k0_ctr}"));
            let k_end = self.tmp();
            self.line(format!("{k_end} = icmp uge i64 {k0}, {}", site.k));
            let k0_done = &k_labels.as_ref().expect("blocked").2;
            self.line(format!("br i1 {k_end}, label %{k0_done}, label %{k0_body}"));
            self.label_line(k0_body);
            Some(k0)
        } else {
            None
        };

        self.line(format!("store i64 {i_lo}, ptr {i0_ctr}"));
        self.line(format!("br label %{i_head}"));

        self.label_line(&i_head);
        let i0 = self.tmp();
        self.line(format!("{i0} = load i64, ptr {i0_ctr}"));
        let rows_done = self.tmp();
        self.line(format!("{rows_done} = icmp uge i64 {i0}, {i_hi}"));
        self.line(format!(
            "br i1 {rows_done}, label %{i_done}, label %{i_body}"
        ));

        // --- pack this i-panel of a: ap[k·ti·t + i] = a[base + ci·(i0+i) + ck·k]
        //
        // **ROW OUTER, k INNER — and the order is the whole point.** The layout is
        // unchanged; only which index moves fastest is.
        //
        // The previous order was k outer, row inner: for one `k` it read one
        // element from each of the `ti·t` rows, which are `ci` apart. LLVM
        // strength-reduces that into **`ti·t` simultaneous row pointers** — 32 on
        // this part. Unblocked it just fits; add the live `k0` and the enclosing
        // k-block loop and it does not, and LLVM spills the row pointers into the
        // innermost loop. Measured in the emitted asm: `ldr x28, [sp, #568]` /
        // `ldr s0, [x28, x8]` — a pointer reload per element.
        //
        // The cost of that was the single largest term in the whole S42 campaign:
        // the pack is **8 ms of work** (`benches/sme/packcost.c` runs these exact
        // loops in C: 8.46 ms unblocked, 8.05 ms blocked, so the memory pattern is
        // not the problem) and it was costing **29.97 ms unblocked and 139.76 ms
        // blocked**. It is what made KC blocking look like a 1.27× loss when its
        // kernel is a 1.7× win (931 → 1598 GFLOP/s).
        //
        // Row outer needs **one** live row pointer, and with `ck == 1` — the
        // matmul shape this rung is gated on — the inner loop walks that row
        // contiguously, which is also the form a vectorizer can do something with.
        // The store side becomes `panel_rows`-strided, which is why `ap` being
        // small matters: blocked it is `ti·t·kc·4` = 64 KB and L1-resident.
        self.label_line(&i_body);
        self.line(format!("store i64 0, ptr {pi_ctr}"));
        self.line(format!("br label %{pi_head}"));

        self.label_line(&pi_head);
        let pi = self.tmp();
        self.line(format!("{pi} = load i64, ptr {pi_ctr}"));
        let pi_end = self.tmp();
        self.line(format!("{pi_end} = icmp uge i64 {pi}, {panel_rows}"));
        self.line(format!(
            "br i1 {pi_end}, label %{pi_done}, label %{pi_body}"
        ));

        // The row is loop-invariant for the whole inner loop now — one pointer,
        // hoisted, instead of `panel_rows` of them live at once.
        self.label_line(&pi_body);
        let row = self.tmp();
        self.line(format!("{row} = add i64 {i0}, {pi}"));
        self.line(format!("store i64 0, ptr {pk_ctr}"));
        self.line(format!("br label %{pk_head}"));

        self.label_line(&pk_head);
        let pk = self.tmp();
        self.line(format!("{pk} = load i64, ptr {pk_ctr}"));
        let pk_end = self.tmp();
        // `kc`, not `site.k`: one k block deep. Equal when unblocked.
        self.line(format!("{pk_end} = icmp uge i64 {pk}, {kc}"));
        self.line(format!(
            "br i1 {pk_end}, label %{pk_done}, label %{pk_body}"
        ));

        self.label_line(&pk_body);
        // The k coordinate in `a` is the block base plus the offset inside it.
        // Unblocked there is no base and this is `pk`, unchanged.
        let a_k = match &k0 {
            Some(k0) => {
                let abs = self.tmp();
                self.line(format!("{abs} = add i64 {k0}, {pk}"));
                abs
            }
            None => pk.clone(),
        };
        let a_index = self
            .emit_tile_index(
                (site.a.base != 0).then(|| site.a.base.to_string()),
                &[(site.a.ci, row.as_str()), (site.a.ck, a_k.as_str())],
            )
            .unwrap_or_else(|| "0".to_owned());
        let a_elem_ptr = self.tmp();
        self.line(format!(
            "{a_elem_ptr} = getelementptr {a_llt}, ptr {a_ptr}, i64 0, i64 {a_index}"
        ));
        let a_value = self.tmp();
        self.line(format!("{a_value} = load {elem_llt}, ptr {a_elem_ptr}"));
        let ap_row = self.tmp();
        self.line(format!("{ap_row} = mul i64 {pk}, {panel_rows}"));
        let ap_index = self.tmp();
        self.line(format!("{ap_index} = add i64 {ap_row}, {pi}"));
        let ap_elem_ptr = self.tmp();
        self.line(format!(
            "{ap_elem_ptr} = getelementptr {ap_llt}, ptr {ap}, i64 0, i64 {ap_index}"
        ));
        self.line(format!("store {elem_llt} {a_value}, ptr {ap_elem_ptr}"));
        let pk_next = self.tmp();
        self.line(format!("{pk_next} = add i64 {pk}, 1"));
        self.line(format!("store i64 {pk_next}, ptr {pk_ctr}"));
        self.line(format!("br label %{pk_head}"));

        self.label_line(&pk_done);
        let pi_next = self.tmp();
        self.line(format!("{pi_next} = add i64 {pi}, 1"));
        self.line(format!("store i64 {pi_next}, ptr {pi_ctr}"));
        self.line(format!("br label %{pi_head}"));

        // --- one panel call per j
        self.label_line(&pi_done);
        let out_row = self.tmp();
        self.line(format!("{out_row} = mul i64 {i0}, {}", site.c));
        self.line(format!("store i64 0, ptr {j0_ctr}"));
        self.line(format!("br label %{j_head}"));

        self.label_line(&j_head);
        let j0 = self.tmp();
        self.line(format!("{j0} = load i64, ptr {j0_ctr}"));
        let cols_done = self.tmp();
        self.line(format!("{cols_done} = icmp uge i64 {j0}, {}", site.c));
        self.line(format!(
            "br i1 {cols_done}, label %{j_done}, label %{j_body}"
        ));

        self.label_line(&j_body);
        // The b panel, the stride the kernel walks its k axis with, and the
        // distance between its `tj` column blocks — each **derived from a
        // recorded fact**, not from a literal that happens to coincide on this
        // part. Three of these were written as `t` and one as `1`, and every one
        // of them was correct only because `tile_j == t` and `b.clane == 1`
        // here. That is the `f32_tiles` defect in miniature (S41b): a fact the
        // profile records, restated as a constant at the use site. Emission is
        // byte-identical on `apple-m4-sme` — the values coincide today, which is
        // exactly why the substitution is provable.
        //
        // Packed layout is `[jt][k][lane]`: a panel is `pack_w` lanes wide and
        // `k` rows deep, so panel `j0/pack_w` starts a whole panel stride in,
        // its k rows are `pack_w` apart, and the next column block is one whole
        // panel further on. `j0` is a multiple of `pack_w` whenever the layout
        // can serve this kernel at all (clause 8), so `(j0/pack_w)·pack_w` is
        // `j0` and the panel offset stays one multiply.
        //
        // Unpacked: `b` in place, k rows `b.ck` apart, and a column block `t`
        // lanes on is `t · b.clane` elements on (`b.ci == 0` by the predicate,
        // so the row axis contributes nothing).
        // **`b_cols` stays a WHOLE-panel stride even when blocked** (`pack_w · k`,
        // not `pack_w · kc`): the packed panels themselves are not re-blocked, so
        // the distance between one panel's start and the next is unchanged. Only
        // where the kernel *begins reading inside* a panel moves with `k0`. This
        // is the easiest thing here to get wrong, and it is why the value-identity
        // gate rather than the emission sweep is what catches it.
        let (b_panel, b_stride, b_cols) = match &packed {
            Some(packed) => {
                let pack_w = self.profile.tile_j(&site.elem);
                let index = self.tmp();
                self.line(format!("{index} = mul i64 {j0}, {}", site.k));
                // Blocked: step into the panel by `k0` k-rows, each `pack_w` wide.
                let index = match &k0 {
                    Some(k0) => {
                        let koff = self.tmp();
                        self.line(format!("{koff} = mul i64 {k0}, {pack_w}"));
                        let sum = self.tmp();
                        self.line(format!("{sum} = add i64 {index}, {koff}"));
                        sum
                    }
                    None => index,
                };
                let ptr = self.tmp();
                self.line(format!(
                    "{ptr} = getelementptr {}, ptr {}, i64 0, i64 {index}",
                    packed.llt, packed.ptr
                ));
                (ptr, pack_w, pack_w * site.k)
            }
            None => {
                // In place, `b`'s k axis is strided by `b.ck`, so the block base
                // is `k0 · b.ck` elements on.
                let mut terms: Vec<(u64, &str)> = vec![(site.b.clane, j0.as_str())];
                if let Some(k0) = &k0 {
                    terms.push((site.b.ck, k0.as_str()));
                }
                let index = self
                    .emit_tile_index((site.b.base != 0).then(|| site.b.base.to_string()), &terms)
                    .unwrap_or_else(|| "0".to_owned());
                let ptr = self.tmp();
                self.line(format!(
                    "{ptr} = getelementptr {b_llt}, ptr {b_ptr}, i64 0, i64 {index}"
                ));
                (ptr, site.b.ck, t * site.b.clane)
            }
        };
        let out_index = self.tmp();
        self.line(format!("{out_index} = add i64 {out_row}, {j0}"));
        let out_panel = self.tmp();
        self.line(format!(
            "{out_panel} = getelementptr {out_llt}, ptr {out_ptr}, i64 0, i64 {out_index}"
        ));
        // The read-out choice: the FIRST k block owns the output block and stores
        // over it; every later block must join the partials already there. `K` is
        // the block depth, not the whole axis.
        let call = |kernel: &str| {
            format!(
                "call void @{kernel}(ptr {ap}, ptr {b_panel}, ptr {out_panel}, i64 {b_stride}, i64 {b_cols}, i64 {}, i64 {kc})",
                site.c
            )
        };
        match (&k0, &k_labels) {
            (Some(k0), Some((_, _, _, store_arm, acc_arm, j_tail))) => {
                let first = self.tmp();
                self.line(format!("{first} = icmp eq i64 {k0}, 0"));
                self.line(format!(
                    "br i1 {first}, label %{store_arm}, label %{acc_arm}"
                ));
                self.label_line(store_arm);
                self.line(call(PanelWrite::Store.symbol()));
                self.line(format!("br label %{j_tail}"));
                self.label_line(acc_arm);
                self.line(call(PanelWrite::Accumulate.symbol()));
                self.line(format!("br label %{j_tail}"));
                self.label_line(j_tail);
            }
            _ => self.line(call(PanelWrite::Store.symbol())),
        }
        let j0_next = self.tmp();
        self.line(format!("{j0_next} = add i64 {j0}, {panel_cols}"));
        self.line(format!("store i64 {j0_next}, ptr {j0_ctr}"));
        self.line(format!("br label %{j_head}"));

        self.label_line(&j_done);
        let i0_next = self.tmp();
        self.line(format!("{i0_next} = add i64 {i0}, {panel_rows}"));
        self.line(format!("store i64 {i0_next}, ptr {i0_ctr}"));
        self.line(format!("br label %{i_head}"));
        self.label_line(&i_done);

        // Close the k-block loop. Unblocked, `i_done` is the terminal label and
        // nothing below is emitted — byte-for-byte the pre-KC rung.
        if let (Some((k0_head, _, k0_done, _, _, _)), Some(k0_ctr), Some(k0)) =
            (&k_labels, &k0_ctr, &k0)
        {
            let k0_next = self.tmp();
            self.line(format!("{k0_next} = add i64 {k0}, {kc}"));
            self.line(format!("store i64 {k0_next}, ptr {k0_ctr}"));
            self.line(format!("br label %{k0_head}"));
            self.label_line(k0_done);
        }
    }
}
