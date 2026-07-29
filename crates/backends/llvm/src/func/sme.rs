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
    /// 8. **Panel reuse** — when the NEON packing rung would have packed `b`,
    ///    its panel is `tile_j` lanes wide and the kernel wants an SVL-wide
    ///    contiguous row. They coincide on this part (both 16); where they do
    ///    not, fall back rather than mint a second layout.
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

        // The A panel: `ti·t` rows × the whole k axis, laid out ap[k·ti·t + i].
        //
        // ponytail: one i-panel deep, so it is `ti · t · k · sizeof` — 256 KB at
        // f32/k=2048/2×2, which `entry_alloc` puts in the arena once it crosses
        // `heap_min_bytes`. A kc-blocked panel (`ti · t · kc`) is the upgrade if
        // a k deep enough to miss L2 ever shows up; that is the same lever
        // `emit_tile_packed_kc` already pulls, and it is measured-off today.
        let ap_llt = format!("[{} x {elem_llt}]", panel_rows * site.k);
        let ap = format!("%s{}", self.fresh());
        self.entry_alloc(&ap, &ap_llt, Some(64));

        let i0_ctr = self.scratch("i64");
        let j0_ctr = self.scratch("i64");
        let pk_ctr = self.scratch("i64");
        let pi_ctr = self.scratch("i64");

        let (i_head, i_body, i_done) = (self.label(), self.label(), self.label());
        let (pk_head, pk_body, pk_done) = (self.label(), self.label(), self.label());
        let (pi_head, pi_body, pi_done) = (self.label(), self.label(), self.label());
        let (j_head, j_body, j_done) = (self.label(), self.label(), self.label());

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

        // --- pack this i-panel of a: ap[k·t + i] = a[base + ci·(i0+i) + ck·k]
        self.label_line(&i_body);
        self.line(format!("store i64 0, ptr {pk_ctr}"));
        self.line(format!("br label %{pk_head}"));

        self.label_line(&pk_head);
        let pk = self.tmp();
        self.line(format!("{pk} = load i64, ptr {pk_ctr}"));
        let pk_end = self.tmp();
        self.line(format!("{pk_end} = icmp uge i64 {pk}, {}", site.k));
        self.line(format!(
            "br i1 {pk_end}, label %{pk_done}, label %{pk_body}"
        ));

        self.label_line(&pk_body);
        let ap_row = self.tmp();
        self.line(format!("{ap_row} = mul i64 {pk}, {panel_rows}"));
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

        self.label_line(&pi_body);
        let row = self.tmp();
        self.line(format!("{row} = add i64 {i0}, {pi}"));
        let a_index = self
            .emit_tile_index(
                (site.a.base != 0).then(|| site.a.base.to_string()),
                &[(site.a.ci, row.as_str()), (site.a.ck, pk.as_str())],
            )
            .unwrap_or_else(|| "0".to_owned());
        let a_elem_ptr = self.tmp();
        self.line(format!(
            "{a_elem_ptr} = getelementptr {a_llt}, ptr {a_ptr}, i64 0, i64 {a_index}"
        ));
        let a_value = self.tmp();
        self.line(format!("{a_value} = load {elem_llt}, ptr {a_elem_ptr}"));
        let ap_index = self.tmp();
        self.line(format!("{ap_index} = add i64 {ap_row}, {pi}"));
        let ap_elem_ptr = self.tmp();
        self.line(format!(
            "{ap_elem_ptr} = getelementptr {ap_llt}, ptr {ap}, i64 0, i64 {ap_index}"
        ));
        self.line(format!("store {elem_llt} {a_value}, ptr {ap_elem_ptr}"));
        let pi_next = self.tmp();
        self.line(format!("{pi_next} = add i64 {pi}, 1"));
        self.line(format!("store i64 {pi_next}, ptr {pi_ctr}"));
        self.line(format!("br label %{pi_head}"));

        self.label_line(&pi_done);
        let pk_next = self.tmp();
        self.line(format!("{pk_next} = add i64 {pk}, 1"));
        self.line(format!("store i64 {pk_next}, ptr {pk_ctr}"));
        self.line(format!("br label %{pk_head}"));

        // --- one panel call per j
        self.label_line(&pk_done);
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
        // distance between its `tj` column blocks. Packed: panel `j0/t` starts
        // at `(j0/t)·k·tile_j`, which at `tile_j == t` is `j0·k`, its k rows are
        // `t` apart, and the NEXT column block is a whole panel (`t·k`) further
        // on. Unpacked: `b` in place, k rows `b.ck` apart and column blocks `t`
        // apart (`b.ci == 0` and `b.clane == 1` by the predicate).
        let (b_panel, b_stride, b_cols) = match &packed {
            Some(packed) => {
                let index = self.tmp();
                self.line(format!("{index} = mul i64 {j0}, {}", site.k));
                let ptr = self.tmp();
                self.line(format!(
                    "{ptr} = getelementptr {}, ptr {}, i64 0, i64 {index}",
                    packed.llt, packed.ptr
                ));
                (ptr, t, t * site.k)
            }
            None => {
                let index = self
                    .emit_tile_index(
                        (site.b.base != 0).then(|| site.b.base.to_string()),
                        &[(1, j0.as_str())],
                    )
                    .unwrap_or_else(|| "0".to_owned());
                let ptr = self.tmp();
                self.line(format!(
                    "{ptr} = getelementptr {b_llt}, ptr {b_ptr}, i64 0, i64 {index}"
                ));
                (ptr, site.b.ck, t)
            }
        };
        let out_index = self.tmp();
        self.line(format!("{out_index} = add i64 {out_row}, {j0}"));
        let out_panel = self.tmp();
        self.line(format!(
            "{out_panel} = getelementptr {out_llt}, ptr {out_ptr}, i64 0, i64 {out_index}"
        ));
        self.line(format!(
            "call void @mapal_sme_panel(ptr {ap}, ptr {b_panel}, ptr {out_panel}, i64 {b_stride}, i64 {b_cols}, i64 {}, i64 {})",
            site.c, site.k
        ));
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
    }
}
