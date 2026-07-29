//! the main tile trio — i-regions, j-split, a-values, lane loop
//!
//! Split out of the former single-file `func.rs` (S41); behaviour is byte-identical.

use super::*;

impl<'a> FnEmit<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_tile_i_regions(
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
    pub(super) fn emit_tile_packed_boundary_row(
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
        let (jw_lo, jw_hi) = self.emit_row_window(site, lo, hi, &row0);

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
    pub(super) fn emit_tile_row_split_j(
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
        let (jw_lo, jw_hi) = self.emit_row_window(site, lo, hi, &row0);
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
    pub(super) fn emit_tile_j_split(
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
    pub(super) fn emit_tile_trio(
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
            let acc_lane = self.emit_acc_lane(&seed_lane, None, r, ctx.tile_j);
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
            let acc_lane = self.emit_acc_lane(&store_lane, None, r, ctx.tile_j);
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

    pub(super) fn emit_tile_a_values(
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
    pub(super) fn emit_tile_lane_loop(
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
                let acc_lane = self.emit_acc_lane(&lane, acc_base, r as u64, acc_row_stride);
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
    pub(super) fn emit_tile_b_value(
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
}
