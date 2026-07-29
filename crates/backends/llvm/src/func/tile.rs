//! the tile ladder entry: `emit_tiled_map` and the register-blocked main tile
//!
//! Split out of the former single-file `func.rs` (S41); behaviour is byte-identical.

use super::*;

impl<'a> FnEmit<'a> {
    pub(super) fn emit_tile_index(
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

    pub(super) fn emit_tiled_map(
        &mut self,
        source: ObjectId,
        target: ObjectId,
        site: &TileSite,
        packed: Option<PackedBuffer>,
    ) {
        // S41 SME rung: on a part with a streaming matrix unit, on the contract
        // face, a matmul-shaped f32 site's leaf becomes one ZA outer product
        // per k instead of TI×TJ FMAs (`func/sme.rs` — the whole realization,
        // selected here and nowhere else). Everything it refuses falls through
        // to the rungs below, unchanged.
        if self.sme_tile_site(site) {
            self.emit_tiled_map_sme(source, target, site, packed);
            return;
        }

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
    pub(super) fn emit_tiled_map_blocked(
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
}
