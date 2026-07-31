//! the FIR 1-D window rung (rung 2 dual: blocks over the lane axis)
//!
//! Split out of the former single-file `func.rs` (S41); behaviour is byte-identical.

use super::*;

impl<'a> FnEmit<'a> {
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
    pub(super) fn emit_tiled_map_blocked_1d(
        &mut self,
        source: ObjectId,
        target: ObjectId,
        site: &TileSite,
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
        let subrows = window_subrows(self.profile);

        let a_ptr = self
            .array_operand_ptr(source, Some(site.a.slot))
            .expect("tile a ptr");
        let b_ptr = self
            .array_operand_ptr(source, Some(site.b.slot))
            .expect("tile b ptr");
        let out_ptr = self.slot(target).expect("tile output slot");
        let acc_llt = format!("[{} x {elem_llt}]", subrows * tile_j);
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
            // The window rung blocks LANES, not rows (see `window_subrows`).
            tile_i: subrows,
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
        self.line(format!("{jb_end} = add i64 {jb}, {}", subrows * tile_j));
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
    pub(super) fn emit_tile_window_block(
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
    pub(super) fn emit_tile_window_step(
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
}
