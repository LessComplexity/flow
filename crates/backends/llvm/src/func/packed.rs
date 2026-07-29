//! BLAS rung 3: the packed j-outer panel and the KC k-panel nest
//!
//! Split out of the former single-file `func.rs` (S41); behaviour is byte-identical.

use super::*;

impl<'a> FnEmit<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_tile_packed_j_outer(
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

    pub(super) fn emit_tile_panel_base(
        &mut self,
        site: &TileSite,
        ctx: &TileCtx,
        j0: &str,
    ) -> String {
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
    pub(super) fn emit_tile_packed_kc(
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
    pub(super) fn emit_tile_kc_i_regions(
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
    pub(super) fn emit_tile_kc_boundary_row(
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
    pub(super) fn emit_tile_kc_boundary_tile(
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
    pub(super) fn emit_tile_kc_j_split(
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
    pub(super) fn emit_tile_kc_apack(
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
    pub(super) fn emit_tile_kc_trio(
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
    pub(super) fn emit_tile_kc_acc_lane(
        &mut self,
        lane: &str,
        acc_base: &str,
        r: u64,
        nc: u64,
    ) -> String {
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
    pub(super) fn emit_tile_kc_a_values(
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
}
