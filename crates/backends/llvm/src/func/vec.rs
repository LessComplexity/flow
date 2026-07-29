//! the vector path: splats, b-vectors, the vectorized k loop
//!
//! Split out of the former single-file `func.rs` (S41); behaviour is byte-identical.

use super::*;

impl<'a> FnEmit<'a> {
    /// A `<TJ x elem>` broadcast of one scalar: `insertelement` into `poison`
    /// then a zeroinitializer `shufflevector` — LLVM's canonical splat, which
    /// instcombine folds to a constant vector when the scalar is one.
    pub(super) fn emit_vec_splat(&mut self, ctx: &TileCtx, scalar: &str) -> String {
        self.emit_splat(&ctx.elem_llt, ctx.tile_j, scalar)
    }

    /// [`Self::emit_vec_splat`] over the two fields it actually needs, so the
    /// conv rung (its own context type, same accumulator shape) shares it.
    pub(super) fn emit_splat(&mut self, elem_llt: &str, tile_j: u64, scalar: &str) -> String {
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
    pub(super) fn emit_tile_b_vector(
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
    pub(super) fn emit_tile_vec_step(
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
    pub(super) fn emit_tile_vec_k_loop(
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
    pub(super) fn emit_tile_vec_out_ptrs(
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
    pub(super) fn emit_tile_trio_vec(
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
    pub(super) fn emit_tile_kc_trio_vec(
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
}
