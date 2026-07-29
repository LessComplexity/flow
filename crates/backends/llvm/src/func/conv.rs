//! the conv2d unrolled micro-kernel rung and its row/block ranges
//!
//! Split out of the former single-file `func.rs` (S41); behaviour is byte-identical.

use super::*;

impl<'a> FnEmit<'a> {
    /// The conv micro-kernel: cashes the k-split record. The fold's
    /// `(k÷div, k%div)` decomposition makes every tap offset compile-time —
    /// per (row `i`, j-tile) the (kq, kr) tap nest is fully unrolled (kq
    /// outer, kr inner IS k-ascending — the R1 invariant) and the body's
    /// div/mod vanish from the emission. Rows and j-tiles keep the rung-1
    /// idiom: the slice's row range with the signed per-row jw clip,
    /// constant-TJ main tiles, one runtime-`tj` remainder tile — never
    /// masked. TI=1 (row blocking is a recorded ceiling, not this rung).
    pub(super) fn emit_tiled_map_conv(
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
    pub(super) fn emit_conv_row_range(
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
    pub(super) fn slice_sizing(&self, site: &TileSite) -> (u64, u32) {
        if site.rows <= 1 || site.c == 0 {
            return (0, 0);
        }
        // The quantum must match the PANEL HEIGHT of whichever realization will
        // run, because `mapal-rt`'s `slice_ranges` cuts on exactly this value.
        // The SME rung's panel is `ti · t` rows tall — the tile block, not one
        // tile and not `tile_i`; handing it the NEON floor would let a slice
        // boundary land mid-panel and a panel straddle two tasks. With
        // `ti·t · c` and `sme_tile_site`'s `rows % (ti·t) == 0` the quantum
        // divides `n` exactly, so every slice is panel-aligned and there is no
        // ragged tail (see `func/sme.rs::emit_tiled_map_sme`).
        let rows_per_block = match self.sme_panel_rows(site) {
            Some(rows) if self.sme_tile_site(site) => rows,
            _ => self.profile.tile_i(),
        };
        let floor = rows_per_block.saturating_mul(site.c);
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
    pub(super) fn emit_conv_blocked_range(
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
    pub(super) fn emit_conv_block_tile(
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
    pub(super) fn emit_tile_conv_tile_vec(
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
    pub(super) fn emit_tile_conv_tile(
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
}
