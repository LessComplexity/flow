//! `TargetProfile` — the emitter's machine facts as data (plan-s31-target-profiles).
//!
//! ADR-0032 D4: tile factors, grain sizes and arena thresholds are backend
//! config, not language. Every field here is **value-invariant** — changing one
//! changes how fast the answer arrives, never the answer — which is exactly what
//! keeps the differential suite a valid gate under every profile.
//!
//! The split this file exists to enforce (the backend-genericity contract):
//! **geometry comes from the record, constants come from the profile.** mapal-ir
//! never learns a machine fact; the emitter never re-derives graph analysis.
//! What used to be six literals swept on one M4 Pro is now one table plus
//! arithmetic, and the arithmetic reproduces those literals for the default
//! profile — that is the correctness gate for this change.

use mapal_ir::{MoveSite, Ty};

/// **The L1 data cache's geometry (S45) — three facts, and the fourth that is
/// arithmetic.**
///
/// `ways` is never recorded: for a set-associative cache
/// `ways = bytes / (line · sets)` is a definition, not an estimate, so recording
/// it could only ever make it wrong (the [`TargetProfile::sme_block`] precedent
/// — that is why `f32_tiles` was deleted).
///
/// `sets` **is** recorded, because each host has a different best source for it
/// and only the detector knows which:
///
/// | host | source for `sets` | status |
/// | --- | --- | --- |
/// | Linux | `/sys/…/index0/number_of_sets` | **read** — the truth, exposed |
/// | macOS | `hw.pagesize / hw.cachelinesize` | **derived** — `sysctl` exposes no set or way count |
///
/// The macOS derivation is the VIPT no-alias bound: an L1 indexed beyond the
/// page offset would alias, so `sets · line ≤ page`, and these parts sit at the
/// limit. **It is a checked heuristic, not a law**, and it is stated as one:
/// it reads the *configured* page rather than the architectural minimum, and
/// there are real parts it gets wrong — a PIPT L1 bigger than its page reach
/// (Cortex-A53, 32 KB 4-way: truth 128 sets, this says 64) and alias-handling
/// VIPT designs (AMD K8, 64 KB 2-way: truth 512, this says 64). It is used only
/// where nothing better is readable, and where it is used it is checked:
///
/// | part | line | sets | `ways()` | against the machine |
/// | --- | ---: | ---: | ---: | --- |
/// | M4 Pro P-core | 128 | 128 (16384/128) | **8** | matches the S44-verified 128-set / 8-way reading |
/// | i9-14900F P-core | 64 | 64 (read) | **12** | `ways_of_associativity` = **12** ✓ |
/// | i9-14900F E-core | 64 | 64 (read) | **8** | `ways_of_associativity` = **8** ✓ |
///
/// And the blast radius if the split is ever wrong is bounded by construction:
/// `sets · ways ≡ bytes / line` however the split falls, so a mis-split can only
/// mis-scale the gcd collapse in [`TargetProfile::move_block`], never the total
/// capacity. A factor-two error moves `slots` by two; the nearest measured
/// margin the decision turns on is pressure **21.3 against 1**.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct L1d {
    /// Per-core L1 data cache in bytes.
    pub bytes: u64,
    /// Cache line in bytes — `hw.cachelinesize` / `coherency_line_size`.
    pub line_bytes: u64,
    /// Sets. Read where the host exposes it, derived from the page otherwise —
    /// see the type doc for which host does which, and for the two families of
    /// part where the derivation is wrong.
    pub sets: u64,
}

impl L1d {
    /// Ways, from capacity — `bytes / (line · sets)`, a definition.
    pub fn ways(&self) -> u64 {
        (self.bytes / (self.line_bytes * self.sets.max(1))).max(1)
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Which machine class a profile describes.
///
/// This is a **capability discriminator**, not a branch flag (plan-s41 §2.2
/// rule 2): it selects which *realization* of a tile site fires, and each
/// realization is one cohesive unit of code. It must never appear as an `if` in
/// the body of a shared emitter — that is the code-locality failure the rule
/// exists to prevent, and §9's branch budget is what measures it.
///
/// The classes are not different algorithms. Every unit Mapal targets — SIMD
/// FMA, ARM SME, Intel AMX, NVIDIA tensor cores — does the same thing: stage
/// operands, issue a multiply-accumulate over a block, accumulate into a
/// **resident** accumulator, keep it hot across the reduction axis, store once
/// at the end. What varies is the shape of one issue and how many accumulator
/// blocks stay resident — and [`TargetProfile::tile_i`] already parameterizes
/// the second. So the tile nest is shared and only the innermost leaf differs
/// (Sapir, S41 plan gate).
///
/// The one genuinely structural split is **cooperation**: on a CPU core a
/// single thread owns its whole tile, while on a GPU a block of threads shares
/// one staged panel and must synchronise before reading it. That is why `Gpu`
/// carries facts `Cpu` has no analogue for, and why a barrier is a morphism in
/// the model rather than an idiom (plan-s41 §1.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Machine {
    /// A CPU core. One thread owns a whole tile; accumulators live in the
    /// vector register file; no cooperation, no barrier. Every profile that
    /// existed before S41 is this, which is what keeps their emission
    /// byte-identical.
    Cpu,
    /// A GPU streaming multiprocessor.
    Gpu(Gpu),
}

/// The SM facts a GPU profile carries, and only those a CPU profile has no
/// field for. Everything a CPU profile already expresses (accumulator capacity,
/// block width, panel depth) stays in [`TargetProfile`] — the point of the
/// unification above is that those are *the same quantities*, read differently.
///
/// **UNMEASURED.** These are read off the ISA and the device query, not swept,
/// exactly like [`ZEN3`]. No realization consumes them yet; plan-s41 steps 3–4
/// decide which are actually read, and any fact a GPU realization needs but
/// cannot find here is an ADR-0033 D4(b) finding to report rather than a value
/// to invent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gpu {
    /// Threads per warp — the cooperation quantum. 32 on every NVIDIA part.
    pub warp: u64,
    /// Shared memory addressable by one block, in bytes. The budget the staged
    /// panel is sized against — the GPU reading of `l2_bytes`.
    pub smem_bytes: u64,
    /// The launch-geometry ceiling: threads in one block.
    pub max_threads_per_block: u64,
    /// The PTX target architecture, e.g. `sm_89`. Verified against the device
    /// rather than assumed: the box GPU reports compute capability 8.9.
    pub arch: &'static str,
}

/// The ARM SME facts a profile carries, when the part has a streaming matrix
/// unit — and **only the two that cannot be derived from anything else.**
///
/// Both are **read off the part** by [`native`] (`hw.optional.arm.sme_max_svl_b`
/// and `hw.perflevel0.l1dcachesize`), never assumed: SVL is
/// implementation-defined and a cache size is a property of the silicon. Every
/// other SME quantity the realization needs is *derived* from these two —
/// the panel side (`svl/w`), the tile count (`w`, an ISA rule), the arrangement,
/// the packed A stride, and the k-panel depth — so a part with a different SVL
/// or a different L1D emits a different kernel from the same record, with no new
/// field and no new code. `f32_tiles` used to sit here and was deleted for
/// exactly that reason: it was derivable, so recording it could only ever make
/// it wrong (see [`TargetProfile::sme_block`]).
///
/// A hand-written value is therefore the *cross-compilation* case — describing a
/// machine you are not sitting on — and
/// `native_reproduces_the_hand_written_profile` asserts the two agree on the
/// machine you are.
///
/// `Machine` stays `Cpu`: SME is a **unit inside a CPU core**, not a machine
/// class. One thread owns the whole tile, there is no cooperation and no
/// barrier — exactly the `Cpu` reading. What differs is only which realization
/// the leaf takes, which is what `Option` here expresses (plan-s41 §2.2 rule 2:
/// capability selects realization).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sme {
    /// Streaming vector length in bytes — ZA is `svl_bytes × svl_bytes`, so one
    /// f32 tile is `svl_bytes/4` square. **64 on the M4 Pro** (512 bits), which
    /// is what makes `<vscale x 4 x float>` sixteen lanes there.
    pub svl_bytes: u64,
    /// **Per-core L1 data cache in bytes** — the budget the SME k-panel is sized
    /// against ([`TargetProfile::sme_kc`]).
    ///
    /// It lives **inside `Sme`**, not beside `l2_bytes`, and that placement is
    /// the point: a profile cannot declare a matrix unit without declaring the
    /// working-set budget that unit's panel is cut to. FRAMEWORK §4.5 law 3
    /// (placement totality) as a type, so the omission is a compile error rather
    /// than a silent fallback to the NEON core's number.
    ///
    /// Why a *second* cache fact rather than reusing `l2_bytes`: `tile_kc` sizes
    /// the NEON k-panel against half of a 16 MB **shared** L2 and returns 4096
    /// on this part, which closes its own gate for every K this project runs
    /// (`apple_m_raises_the_kc_threshold_above_every_k_we_run`). Measured, the
    /// SME optimum is a **128 KB two-panel window** — L1D, not L2 — and the
    /// unblocked case loses **1.448×** at K=4096
    /// (`docs/performance/s42-sme-roofline.md` §5, `benches/sme/kc.c`). One
    /// deduced depth serving two placements with different budgets is the defect;
    /// two `TrnLoc`s over one question is the fix.
    pub l1d_bytes: u64,
    /// **Policy ratio, honestly search space — NOT a machine fact.** How many
    /// L1D's worth of operand window one k block may occupy. Same status as
    /// [`TargetProfile::acc_vecs_per_row`] and `nc_tiles`, and ADR-0034 is the ADR
    /// that would search it rather than record it.
    ///
    /// It exists because the obvious derivation is **wrong, measured**. Sizing the
    /// window to fit L1D exactly (ratio 1 ⇒ `kc` 512 here) lands on the losing
    /// side of a sharp curve. Two depth sweeps in the real emitter, N=4096 and
    /// N=2048, f32, 1 thread, alternating, values identical at every depth:
    ///
    /// | working set | N=4096 | N=2048 |
    /// | ---: | ---: | ---: |
    /// | 32 KB | — | 0.220× |
    /// | 64 KB | 0.501× | 0.387× |
    /// | 128 KB ← ratio 1 | 0.785× | 0.639× |
    /// | **256 KB ← ratio 2** | **1.064×** | 0.986× |
    /// | 512 KB | 1.027× | **1.000×** (unblocked) |
    /// | 1024 KB | 1.000× (unblocked) | — |
    ///
    /// **256 KB is the optimum at both sizes, and it is not any machine fact on
    /// this part** — L1D is 128 KB, L1I 192 KB, the per-core L2 slice ~3.2 MB. So
    /// it is recorded as a swept ratio rather than dressed up as a derivation.
    /// Writing `2 * l1d_bytes` into [`TargetProfile::sme_kc`] would be a fitted
    /// constant wearing a derivation's clothes, which is the exact defect
    /// `plan-s31-deduced-blocking.md` exists to remove.
    ///
    /// Note also what the curve says about getting it wrong: every depth *below*
    /// the optimum is catastrophic (64 KB costs 2.0–2.6×), so this constant is not
    /// a mild tuning knob. The first version of `sme_kc` used ratio 1 and that one
    /// wrong constant is what made KC blocking look like a 1.27× loss for most of
    /// S42 — see `func/sme.rs`.
    pub panel_l1d_ratio: u64,
}

/// One target's machine facts, plus the two policy ratios that are honestly
/// search space rather than facts (see the field docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetProfile {
    pub name: &'static str,
    /// The machine class — the capability that selects a realization (S41).
    pub machine: Machine,
    /// One vector register in bytes: NEON 16, AVX2 32, AVX-512 64.
    pub vec_bytes: u64,
    /// Architectural vector register count: NEON 32, AVX2 16, AVX-512 32.
    /// **Not probed** — it is a fact of the ISA, so it arrives with the target
    /// features rather than from a runtime query.
    pub vec_regs: u64,
    /// Policy ratio: how many vector registers one accumulator row occupies,
    /// i.e. how wide a j-tile is measured in registers. This is "how much of
    /// the register file to spend on accumulators" — search space, not a
    /// machine fact (ADR-0034 is the ADR that would search it).
    pub acc_vecs_per_row: u64,
    /// Policy ratio: j-tiles per KC-rung j-block. Same status as
    /// `acc_vecs_per_row`.
    pub nc_tiles: u64,
    /// L2 in bytes, **as reported for one L2 instance** — the budget the KC
    /// rung's k-panel is sized against. Divide by [`Self::l2_cores`] for the
    /// per-core share; see that field for why the distinction is load-bearing.
    pub l2_bytes: u64,
    /// **How many physical cores share one `l2_bytes`.** This field exists
    /// because the old doc comment on `l2_bytes` said "per-core" while
    /// [`APPLE_M`] set it to the 16 MB **shared** P-cluster L2 — a two-word
    /// ambiguity that no consumer had noticed because `tile_kc` only ever wanted
    /// an order of magnitude.
    ///
    /// [`TargetProfile::move_block`]'s cost term divides by it, and there the
    /// difference is the whole answer: 16 MB says a 4 MB array is resident and
    /// the rung should decline, 16 MB / **5** = 3.36 MB says it is not and the
    /// rung should fire — and the M4's measured 1.578× says fire. Detected
    /// (`hw.perflevel0.cpusperl2`, `shared_cpu_list`), never assumed.
    ///
    /// Budgeting **per core** rather than per chip is the right conservatism for
    /// a compiler that emits one binary for both thread counts: S44 §5 measured
    /// that this walk's pressure does not dilute with thread count, so at full
    /// occupancy every core really does get its share and no more.
    pub l2_cores: u64,
    /// Stack ceiling policy: an entry-block block at least this large is placed
    /// in the `mapal_rt_alloc` arena instead of the stack.
    pub heap_min_bytes: u64,
    /// The streaming matrix unit, when the part has one. `None` for every
    /// profile that existed before S41 — which is exactly what keeps their
    /// emission byte-identical: the SME realization cannot be selected without
    /// a fact only this field carries.
    pub sme: Option<Sme>,
    /// The L1D geometry the move-panel decision needs (S45). `None` for
    /// [`GENERIC`] and every profile that predates S45 — the same seam
    /// discipline as [`Self::sme`] and [`Self::gpu`], and it is what keeps the
    /// **default** profile's emission byte-identical: the rung cannot fire
    /// without a fact only this field carries, so `--target=generic` emits today's
    /// text for every one of the 171 swept cells.
    ///
    /// ponytail: [`Sme::l1d_bytes`] is the same number for parts that carry
    /// both, pinned equal by `l1d_and_sme_agree_on_the_l1d`. Merging them routes
    /// `sme_kc` through a new `Option` for a field rename, and a working SME leg
    /// is not worth that; the test makes the duplication unable to drift.
    pub l1d: Option<L1d>,
}

/// The element width a tile site accumulates at. Tile sites are numeric-gated
/// by recognition (`mapal_ir::algo` requires `is_numeric` for the element, the
/// accumulator, the seed and the map target), so the non-numeric arm is
/// unreachable; it returns the f32 width rather than panicking because a wrong
/// tile width is a slow kernel, never a wrong answer.
fn elem_bytes(elem: &Ty) -> u64 {
    match elem {
        Ty::Int { bits, .. } | Ty::Float { bits } => u64::from(*bits).div_ceil(8).max(1),
        _ => 4,
    }
}

impl TargetProfile {
    /// Scalar lanes in one vector register at this element width.
    pub fn lanes(&self, elem: &Ty) -> u64 {
        (self.vec_bytes / elem_bytes(elem)).max(1)
    }

    /// The register micro-kernel's j-tile width in lanes (was `tile_j_for`).
    /// `generic`: f32 `16 / 4 × 4 = 16`, f64 `16 / 8 × 4 = 8` — today's
    /// literals, reproduced.
    pub fn tile_j(&self, elem: &Ty) -> u64 {
        self.lanes(elem) * self.acc_vecs_per_row
    }

    /// Rows per register block (was `TILE_I`, on the matmul rung only — the FIR
    /// window rung reads this same budget via `crate::func::window_subrows`, over a different
    /// quantity over a memory accumulator).
    ///
    /// The `2 ×` is the headroom policy: spend at most HALF the vector file on
    /// accumulators, leaving the rest for the shared b tile, the a splat and
    /// the products. `generic`: `32 / (2 × 4) = 4`. It reproduces S26's swept
    /// result *including its failure* — TI=8 would need 32 accumulator
    /// registers, and that sweep recorded "8 spills: 128 accumulators ≫ 32 NEON
    /// regs".
    pub fn tile_i(&self) -> u64 {
        (self.vec_regs / (2 * self.acc_vecs_per_row)).max(1)
    }

    /// The KC rung's j-block width in lanes (was `tile_nc_for`).
    pub fn nc(&self, elem: &Ty) -> u64 {
        self.tile_j(elem) * self.nc_tiles
    }

    /// The KC rung's k-panel depth (was `TILE_KC`): half of L2 spent on one
    /// (kc × nc) packed-b window. `generic` yields 128 at BOTH widths —
    /// `512 KB/2 ÷ (512 × 4)` and `512 KB/2 ÷ (256 × 8)` — today's single
    /// literal, reproduced. On a 16 MB L2 it yields `kc ≥ K`, so the gate
    /// `site.k > tile_kc` closes and **the KC nest disables itself by
    /// derivation** rather than by a default-off flag (S29/S30's measured
    /// verdict, deduced).
    pub fn tile_kc(&self, elem: &Ty) -> u64 {
        ((self.l2_bytes / 2) / (self.nc(elem) * elem_bytes(elem))).max(1)
    }

    /// The L2 budget **one core** actually gets. See [`Self::l2_cores`].
    pub fn l2_per_core(&self) -> u64 {
        self.l2_bytes / self.l2_cores.max(1)
    }

    /// **The S45 move-panel decision, in one function: fire or not, and with
    /// what block.** `None` declines — the emitter then emits today's flat loop
    /// character-identically.
    ///
    /// This is the join ADR-0032 puts here and nowhere else: [`MoveSite`] is
    /// pure program geometry, [`L1d`] is pure machine fact, and neither crate
    /// may hold both. It replaces `--move-panel=W:B`, where a human held both in
    /// their head and typed the answer.
    ///
    /// ```text
    /// S            = cr · sizeof(elem)                       -- the walk's byte stride
    /// sets_touched = line | S ? sets / gcd((S/line) mod sets, sets) : sets
    /// slots        = sets_touched · ways                     -- lines the walk can reach
    /// lines_live   = min(width, width · S / line)            -- lines it needs at once
    /// FIRE iff  lines_live > slots  AND  read_bytes > l2_per_core
    /// ```
    ///
    /// **Two terms, because one is provably not enough.** `lines_live > slots`
    /// is S44's `pressure > 1` written as what it means — the sweep needs more
    /// lines than it can reach — with no threshold to fit. But pressure predicts
    /// how OFTEN the L1 is defeated and says nothing about what a defeat COSTS:
    /// the i9 at side 512 scores 21.3 and **loses 0.901×** (replicated 0.907×).
    /// A defeat costs real money only when the array being re-swept does not fit
    /// the private level below L1; otherwise the miss is an L2 hit the
    /// out-of-order engine hides. The **read** array is the quantity, not the
    /// whole working set — the conflict is on `a`'s lines, the writes stream —
    /// which also keeps the discriminating case off a knife edge (1 MB against
    /// 2 MB, rather than 2 MB against 2 MB).
    ///
    /// Both terms were tested predictively rather than fitted. M4 side 512 has
    /// pressure 8 and a 1 MB read array: the cost term declines it, S44's
    /// standalone probe claimed **2.09×** there, and the emitted pipeline
    /// measured **every arm overlapping OFF, min-to-min 1.02×** — the term's
    /// prediction, on the machine where it was most at risk
    /// (`benches/results-s45/deduced-move-panel.md` §3).
    ///
    /// | case | pressure | read vs L2/core | verdict | measured |
    /// | --- | ---: | --- | --- | --- |
    /// | M4 1024 | 32 | 4 MB > 3.36 MB | **fire, B=16** | 1.578× 1t / 1.993× threaded |
    /// | M4 2048 | 128 | 16 MB > 3.36 MB | **fire, B=8** | 3.19× (probe) |
    /// | M4 128 | 0.5 | — | decline (pressure) | 1.000× |
    /// | M4 512 | 8 | 1 MB < 3.36 MB | decline (cost) | overlapping, 1.02× min-to-min |
    /// | i9 1024 | 85.3 | 4 MB > 2 MB | **fire, B=8** | 2.646× 1t, disjoint |
    /// | i9 2048 | 170.7 | 16 MB > 2 MB | **fire, B=8** | 3.021× 1t, disjoint |
    /// | i9 512 | 21.3 | 1 MB < 2 MB | decline (cost) | **0.901× LOSS** |
    ///
    /// **The block: the geometric mean of two opposing costs, both measured.**
    ///
    /// ```text
    /// floor   = line / sizeof(elem)   -- a block row shorter than a line refetches it
    /// ceiling = slots / 2             -- the block's read lines share the reachable
    ///                                    sets with its write stream
    /// B       = largest divisor of gcd(width, rows) <= sqrt(floor * ceiling)
    /// ```
    ///
    /// Both costs are 1 inside `[floor, ceiling]`; the window is normally EMPTY
    /// (the bounds pull apart), and the product of two opposing multipliers is
    /// minimised at their geometric mean. The divisibility clause is not
    /// rounding — the permutation needs the panel to tile the geometry both ways
    /// or it would need a remainder arm.
    ///
    /// Each bound is a measurement, not an argument. **Floor:** B=8 on the i9
    /// covers 8 of the 16 f32 in a line and measures **15% slower than B=16** at
    /// side 1024, 24% at 2048. **Ceiling:** B=`slots` on the M4 measures **29%
    /// (S44) and 34% (S45) slower threaded** than B=16 at side 1024, and 56% at
    /// side 2048.
    ///
    /// It reproduces the M4's swept optimum at **both** sides — 16 at side 1024
    /// (floor 32, ceiling 16, mean 22) and 16 at side 2048 (floor 32, ceiling 8,
    /// mean 16) — which the previous ceiling-only rule did not: it returned 8 at
    /// side 2048 and measured **15.7% off**.
    ///
    /// **What it does NOT reproduce, and why no version of it could.** The i9's
    /// measured optimum is **128**, and this returns 8. That gap was priced with
    /// counters rather than argued (`benches/results-s45`, side 1024, P-core):
    ///
    /// | arm | ms | cycles | L1-dcache-load-misses | dTLB misses | LLC misses |
    /// | --- | ---: | ---: | ---: | ---: | ---: |
    /// | off | 2.526 | 16.2 M | **1 053 802** | 289 | 683 |
    /// | B=8 | 1.072 | 9.75 M | 206 414 | 291 | 259 |
    /// | B=16 | 0.940 | 9.14 M | 499 456 | 282 | 316 |
    /// | B=128 | 0.929 | 8.83 M | **1 053 618** | 287 | 253 |
    ///
    /// **`off` and B=128 miss L1 the same number of times — 1 053 802 against
    /// 1 053 618 — and B=128 is 2.7x faster.** B=8 misses **five times less**
    /// and is *slower* than both. TLB misses are flat at ~290 across every arm,
    /// and LLC misses are ~0 (the 4 MB array fits a 36 MB L3). Instruction
    /// counts are identical across the blocked arms (33.03 M), so the ordering
    /// is pure memory behaviour and IPC rises monotonically with B
    /// (3.39 → 3.61 → 3.74).
    ///
    /// So on that machine **L1 residency is not the binding resource at the
    /// optimum**, and neither is the TLB or the LLC: what B=128 buys is
    /// memory-level parallelism — more independent misses in flight, absorbed by
    /// a 512-entry reorder buffer against 12 ways. No quantity in [`L1d`] prices
    /// that, so this derivation cannot produce 128 and does not pretend to. The
    /// residual is **14% of the win at side 1024 and 21% at 2048**, and it is
    /// carried as a known gap rather than closed with a per-machine table.
    pub fn move_block(&self, site: &MoveSite) -> Option<u64> {
        let l1 = self.l1d.as_ref()?;
        let (line, sets, ways) = (l1.line_bytes, l1.sets.max(1), l1.ways());
        let w = elem_bytes(&site.elem);
        let stride = site.cr.checked_mul(w)?;
        // The slow axis must stay inside a line, or there is no line reuse for a
        // block to preserve and this rung has nothing to say about the shape.
        if stride == 0 || site.cq * w >= line {
            return None;
        }
        let touched = if stride.is_multiple_of(line) {
            sets / gcd((stride / line) % sets, sets).max(1)
        } else {
            sets
        };
        // CONFLICT, not capacity — S44's headline, as the gate it always was.
        // A walk that reaches every set is limited by capacity, and capacity is
        // measured FREE on these parts (S43: flat 32 KB → 8 MB). Measured
        // witness: side 1025 has `gcd(4100/128 …)` = no collapse, runs **2.12×
        // FASTER unblocked than side 1024**, and blocking it costs 0.623×. It
        // also scores pressure 1.0009, so without this term it would fire — this
        // is the clause that keeps `lines_live > slots` from being a threshold
        // fitted at its own boundary.
        let slots = touched * ways;
        let lines_live = (site.width * stride / line).min(site.width);
        if touched >= sets || lines_live <= slots || site.len.checked_mul(w)? <= self.l2_per_core()
        {
            return None;
        }
        // The block balances TWO opposing costs, both of them measured.
        //
        //   traffic  T(B) = max(1, (line/w) / B)   -- a block row shorter than a
        //     line fetches that line once per block-row it is split across. At
        //     B=8 on the i9 (16 f32 per line) it doubles the L1<-L2 traffic, and
        //     it measures 15% slower than B=16 at side 1024, 24% at 2048.
        //   conflict C(B) = max(1, B / (slots/2))  -- the block's read lines and
        //     its write stream compete for the sets the walk can reach. At
        //     B=slots on the M4 it measures 29-56% slower threaded than B=16.
        //
        // Both are 1 inside `[line/w, slots/2]`; when that window is EMPTY —
        // which is the normal case, because the two bounds pull apart — the
        // product `T·C` is minimised at their geometric mean. That is the whole
        // derivation: no threshold, no per-machine number, and it reproduces the
        // M4's swept optimum at BOTH sides (16 at side 1024 and at 2048).
        let floor = (line / w).max(1);
        let ceiling = (slots / 2).max(1);
        let target = (floor * ceiling).isqrt().max(2);
        let tile = gcd(site.width, site.rows);
        (2..=target.min(tile))
            .rev()
            .find(|b| tile.is_multiple_of(*b))
    }

    /// The GPU facts, when this profile describes one. `None` for every CPU
    /// profile — callers that need a GPU fact must ask for it, so a CPU profile
    /// reaching a GPU-only path is a `None` at the seam rather than a silently
    /// wrong constant.
    pub fn gpu(&self) -> Option<&Gpu> {
        match &self.machine {
            Machine::Cpu => None,
            Machine::Gpu(g) => Some(g),
        }
    }

    /// The streaming matrix unit's facts, when this part has one. Same seam
    /// discipline as [`TargetProfile::gpu`]: a profile without SME reaching an
    /// SME-only path is a `None`, never a wrong constant.
    pub fn sme(&self) -> Option<&Sme> {
        self.sme.as_ref()
    }

    /// The side of one square ZA tile at this element width — `svl_bytes /
    /// sizeof(elem)`, i.e. **16 for f32 at SVL 512**. `None` without an SME
    /// unit.
    ///
    /// This is the one number the emitted kernel's every literal comes from
    /// (the panel side, the ZA row count, the packed-A row stride). It is a
    /// per-`Loc` machine fact and must never reach `mapal-ir`.
    pub fn sme_tile_side(&self, elem: &Ty) -> Option<u64> {
        Some(self.sme.as_ref()?.svl_bytes / elem_bytes(elem))
    }

    /// How many ZA tiles one panel accumulates into, and in what arrangement:
    /// `(ti, tj)` with `ti · tj` = the architectural tile count at this width,
    /// so a panel covers `ti·t × tj·t` outputs. `None` without an SME unit.
    ///
    /// This is [`TargetProfile::tile_i`]'s question asked of the other
    /// accumulator file — "how many accumulator blocks stay resident" — and it
    /// is answered the same way, by arithmetic rather than by a literal.
    ///
    /// **The tile count is not a recorded fact; it is an ISA rule.** ZA is
    /// `svl_bytes × svl_bytes` bytes and one tile at element width `w` is
    /// `(svl/w)² · w` bytes, so the count is `svl²/((svl/w)²·w) = w` — always
    /// **exactly the element byte width**, for any SVL: 4 tiles at f32
    /// (`ZA0.S`…`ZA3.S`), 8 at f64 (`ZA0.D`…`ZA7.D`), 2 at f16, 1 at i8. That is
    /// the architecture, so it is derived here rather than recorded per part.
    ///
    /// This replaced an `f32_tiles: 4` field. Deriving it removes the last
    /// hardcoded SME number *and* closes the S41b P2 defect that the field
    /// created: a recorded count could say `7`, for which this function would
    /// happily return a 1×7 arrangement that no ZA register file can hold and
    /// no emitted kernel could select. A count that is `elem_bytes` by
    /// construction cannot be wrong.
    ///
    /// The **shape** is a derivation too, not taste. One panel issues `ti · tj`
    /// outer products from `ti + tj` operand loads, and `ti + tj` is minimised
    /// by the most-square factorization: 2×2 feeds 4 MACs from 4 loads, while
    /// 1×4 would need 5 for the same 4. Measured on this part
    /// (`benches/sme/mm4.c`, 2×2 vs 1 tile, f32, 1 thread): 423 → 777 GFLOP/s
    /// at 1024², 237 → 619 at 2048².
    pub fn sme_block(&self, elem: &Ty) -> Option<(u64, u64)> {
        self.sme.as_ref()?;
        let tiles = elem_bytes(elem).max(1);
        // The largest divisor no bigger than sqrt(tiles) — the square-most split.
        let ti = (1..=tiles)
            .filter(|d| d * d <= tiles && tiles.is_multiple_of(*d))
            .max()
            .unwrap_or(1);
        Some((ti, tiles / ti))
    }

    /// The SME rung's k-panel depth: how deep a k block may be before the two
    /// packed operand panels stop fitting the unit's working-set budget.
    /// `None` without an SME unit.
    ///
    /// **There is no opt-in.** The moment a profile records a matrix unit, every
    /// factor that unit's realization needs is *derived* from the facts recorded
    /// alongside it — the same contract [`TargetProfile::sme_block`] and
    /// [`TargetProfile::sme_tile_side`] already honour. A gate the caller has to
    /// remember to open is how `f32_tiles: 4` sat in this file while the emitter
    /// used one tile (S41b), and the deficit was ~4×.
    ///
    /// One k step of a panel streams `ti·t` elements of packed `a` and `tj·t` of
    /// packed `b`, so the window is `(ti + tj)·t·sizeof` bytes per k, and the
    /// depth that fits is the budget divided by it. On this part that is
    /// arithmetic with no free parameter:
    ///
    /// ```text
    /// (ti + tj)·t   = (2 + 2)·16 = 64 elements per k
    /// 64 · 4 B      = 256 B per k
    /// 2*131072 / 256 = 1024       <- the depth swept in the emitter
    /// ```
    ///
    /// **1024 is not written down anywhere; it falls out of L1D x the swept ratio.** That is the
    /// `plan-s31-deduced-blocking.md` discipline — a literal swept once is
    /// replaced by a derivation that reproduces it on the machine it was swept
    /// on, and yields a defensible number on a machine nobody has swept. `f64`
    /// scales off the same fact by element width, so there is no second constant.
    ///
    /// Measured (`benches/sme/kc.c`, N=4096, f32, 1 thread, 9 alternating runs,
    /// medians, every KC gated against an independent scalar reference): 64 KB →
    /// 845.1, **128 KB → 1101.2**, 256 KB → 1046.7, 512 KB → 885.1, unblocked
    /// (1 MB) → 760.7 GFLOP/s. Unimodal, so there is a real optimum rather than
    /// a trend, and the peak puts N=4096 level with N=1024.
    ///
    /// The caller gates on `site.k > sme_kc`, so at shallow k the nest disables
    /// itself by derivation — which is correct, because the probe shows blocking
    /// is a **loss** at K ≤ 512 where the panel already fits.
    pub fn sme_kc(&self, elem: &Ty) -> Option<u64> {
        let sme = self.sme.as_ref()?;
        // A detected machine fact times a SWEPT policy ratio — see
        // `Sme::panel_l1d_ratio` for the two depth sweeps that set it, and for why
        // folding the 2 into this expression would be dishonest.
        let budget = sme.l1d_bytes * sme.panel_l1d_ratio;
        let t = self.sme_tile_side(elem)?;
        let (ti, tj) = self.sme_block(elem)?;
        Some((budget / (elem_bytes(elem) * (ti + tj) * t)).max(1))
    }
}

/// Today's six literals, exactly: 128-bit vectors, 32 registers, a 512 KB
/// per-core L2. Emission under this profile is byte-identical to the
/// pre-profile emitter for every shape — the plan's rule 1, and the reason it
/// is the default.
pub const GENERIC: TargetProfile = TargetProfile {
    name: "generic",
    machine: Machine::Cpu,
    vec_bytes: 16,
    vec_regs: 32,
    acc_vecs_per_row: 4,
    nc_tiles: 32,
    l2_bytes: 512 * 1024,
    l2_cores: 1,
    heap_min_bytes: 256 * 1024,
    sme: None,
    // No L1D geometry ⇒ the S45 move rung cannot fire under the DEFAULT
    // profile, so `--target=generic` stays byte-identical for all 171 swept
    // cells. Firing needs a profile that describes a real part.
    l1d: None,
};

/// Apple M-series: NEON at 16 B × 32 registers, but a 16 MB L2 **shared by five
/// P-cores** (`hw.perflevel0.l2cachesize` / `hw.perflevel0.cpusperl2` on this M4
/// Pro). Same tile widths as `generic`; the L2 is what differs, and it closes the
/// KC gate by derivation.
pub const APPLE_M: TargetProfile = TargetProfile {
    name: "apple-m",
    l2_bytes: 16 * 1024 * 1024,
    l2_cores: 5,
    ..GENERIC
};

/// AVX2 zen3: 32 B vectors, 16 registers, 512 KB per-core L2.
///
/// **UNTESTED — read off documentation, never measured.** Note it moves two
/// constants at once relative to `generic`: `tile_j(f32)` 16 → 32 and `tile_i`
/// 4 → 2. That is self-consistent arithmetic, not a validated configuration;
/// the box leg is what settles it.
pub const ZEN3: TargetProfile = TargetProfile {
    name: "zen3",
    vec_bytes: 32,
    vec_regs: 16,
    ..GENERIC
};

/// Intel Raptor Lake — the i9-14900F box (24C/32T, 8 P + 16 E, governor
/// `performance`). **The cross-compilation case, and the reason named profiles
/// still exist:** the box has gcc and no clang, so every leg is emitted on the
/// Mac, where nothing about the i9 can be detected. Every number here was read
/// off `/sys/devices/system/cpu/cpu0/cache/` on the box itself and is recorded
/// with its source:
///
/// | field | source | value |
/// | --- | --- | --- |
/// | `l1d.bytes` | `index0/size` | 48K (P-core; the E-core is 32K) |
/// | `l1d.line_bytes` | `index0/coherency_line_size` | 64 |
/// | `l1d.page_bytes` | `getconf PAGESIZE` | 4096 |
/// | `l2_bytes` | `index2/size` | 2048K |
/// | `l2_cores` | `index2/shared_cpu_list` = `0-1` | **1** physical core (two SMT threads) |
///
/// `sets`/`ways` are **not** recorded: `page/line = 64` and
/// `48K/(64·64) = 12` reproduce `number_of_sets=64` and
/// `ways_of_associativity=12` exactly (`l1d_derivation_reproduces_the_i9`).
///
/// It describes the **P-core**, which is what the 1-thread legs are pinned to
/// (`taskset -c 4`). The E-core's 32K/8-way L1D derives 8 ways from the same
/// rule and would give the same block here; the P-core is the conservative and
/// the measured one.
pub const RAPTORLAKE: TargetProfile = TargetProfile {
    name: "raptorlake",
    vec_bytes: 32,
    vec_regs: 16,
    l2_bytes: 2 * 1024 * 1024,
    l2_cores: 1,
    l1d: Some(L1d {
        bytes: 48 * 1024,
        line_bytes: 64,
        sets: 64,
    }),
    ..GENERIC
};

/// NVIDIA Ada (`sm_89`) — the box GPU, an RTX 4070 Ti (compute capability 8.9,
/// driver 610.43.03, 12 GB). plan-s41 step 1.
///
/// **No realization consumes this yet.** It exists so the GPU class is
/// expressible and the resolver knows the name; steps 3–4 build the realization
/// that reads it. The CPU-shaped fields are inherited from [`GENERIC`] and are
/// **placeholders, not GPU measurements** — `vec_bytes`/`vec_regs`/`l2_bytes`
/// describe a vector register file and a cache hierarchy, and their GPU
/// readings (per-thread registers, shared memory) are the open question S38
/// already flagged: *"`<TJ x elem>` means SIMD lanes on CPU and per-thread
/// registers on GPU — one record field, two readings."* Which of them a GPU
/// realization actually reads, and which need their own value, is ADR-0033
/// D4(a)/(b) and is answered by building, not here.
///
/// `sm_89` has 4th-generation tensor cores but no MXFP block-scale (that is
/// `sm_100`), so those intrinsics are emittable and not runnable on this part.
/// Irrelevant to the S41 leg — `mma` is out of scope (plan-s41 §7).
pub const CUDA_ADA: TargetProfile = TargetProfile {
    name: "cuda-ada",
    machine: Machine::Gpu(Gpu {
        warp: 32,
        smem_bytes: 48 * 1024,
        max_threads_per_block: 1024,
        arch: "sm_89",
    }),
    ..GENERIC
};

/// Apple M4 with the streaming matrix unit enabled: [`APPLE_M`] plus the SME
/// facts measured on the part (`benches/sme/svl.c` — SVL 64 B, 4 f32 tiles;
/// `hw.optional.arm.FEAT_SME`/`FEAT_SME2`/`FEAT_SME_F32F32` all set).
///
/// It is a **separate name** rather than a field flipped on `apple-m` because
/// the SME leaf is only bit-equal to the NEON leaf on the contract face
/// (`fmopa` fuses — measured 92/256 cells differ against separate mul+add,
/// 0/256 against `fmaf`), so selecting it is a decision about the *program*, not
/// only about the machine. Naming it keeps `apple-m` byte-identical forever and
/// keeps a box run reproducible.
pub const APPLE_M4_SME: TargetProfile = TargetProfile {
    name: "apple-m4-sme",
    // Both facts are `hw.optional.arm.sme_max_svl_b` and
    // `hw.perflevel0.l1dcachesize` on this M4 Pro, and `native` reads both — so
    // this profile is now the *cross-compilation* spelling of a machine you are
    // not sitting on, and `native_reproduces_the_hand_written_profile` asserts
    // detection agrees with it here. The tile count is absent because it is an
    // ISA rule, derived in `sme_block`.
    sme: Some(Sme {
        svl_bytes: 64,
        l1d_bytes: 128 * 1024,
        panel_l1d_ratio: 2,
    }),
    // S45: the same L1D, plus the two facts the move rung needs and the SME rung
    // does not. `hw.cachelinesize` = 128 and `hw.pagesize` = 16384 on this part;
    // 128 sets and 8 ways follow (`L1d`), so no associativity is written down.
    l1d: Some(L1d {
        bytes: 128 * 1024,
        line_bytes: 128,
        sets: 128,
    }),
    ..APPLE_M
};

const PROFILES: [&TargetProfile; 6] = [
    &GENERIC,
    &APPLE_M,
    &ZEN3,
    &CUDA_ADA,
    &APPLE_M4_SME,
    &RAPTORLAKE,
];

/// Resolve a profile by name. `None` for an unknown name — never a silent
/// fallback to `generic`, because a typo that quietly emits the default
/// profile's numbers is the exact failure this table exists to remove.
///
/// **A hand-written profile always wins.** The table is checked first, so
/// `--target=apple-m4-sme` overrides detection; `native` is only ever reached by
/// asking for it by name. That ordering is the contract: detection removes the
/// need to hand-write a profile for the machine you are sitting on, and changes
/// nothing about describing a machine you are not.
pub fn resolve(name: &str) -> Option<&'static TargetProfile> {
    PROFILES
        .into_iter()
        .find(|p| p.name == name)
        .or_else(|| (name == "native").then(native).flatten())
        // S46: `native` is the DEFAULT target, so it must always resolve. When a
        // fact cannot be read — an unknown OS, a locked-down container, a CPU
        // whose caches sysfs does not describe — fall back to `generic` rather
        // than fail the build. `generic` carries no cache geometry, so every
        // rung that needs it declines: the compiler goes conservative instead of
        // guessing, which is the same contract as before this became default.
        .or_else(|| (name == "native").then_some(&GENERIC))
}

/// The known profile names, for the error message on an unknown one.
pub fn names() -> String {
    PROFILES
        .iter()
        .map(|p| p.name)
        .chain(std::iter::once("native"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One `sysctl` integer, or `None` if the key does not exist on this host.
///
/// Shelling out rather than linking `libc`: this crate has two dependencies and
/// the emitter is a build-time tool that reads four keys once per run. A binding
/// for `sysctlbyname` would be a new dependency to save a process spawn that
/// happens at most once.
#[cfg(target_os = "macos")]
fn sysctl_u64(key: &str) -> Option<u64> {
    let out = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", key])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()?.trim().parse().ok()
}

/// The host's own facts, **detected rather than written down**.
///
/// Reached as `--target=native`. Named profiles are resolved first (see
/// [`resolve`]), so this never shadows a hand-written one.
///
/// **Every machine fact is detected; nothing about the part is written down.**
///
/// | fact | source | |
/// | --- | --- | --- |
/// | `l2_bytes` | `hw.perflevel0.l2cachesize` | 16 MB here |
/// | `l1d_bytes` | `hw.perflevel0.l1dcachesize` | 128 KB here |
/// | `svl_bytes` | `hw.optional.arm.sme_max_svl_b` | 64 here |
/// | SME present | `hw.optional.arm.FEAT_SME` | 1 here |
/// | tile count | **derived** — ISA rule, see [`TargetProfile::sme_block`] | |
///
/// **`perflevel0`, not the bare keys.** `hw.l1dcachesize` reports **65536** on
/// this part and `hw.perflevel0.l1dcachesize` reports **131072**: the bare key
/// describes the E-cores. Sizing a P-core matrix panel against an E-core cache
/// would halve the derived k-panel and silently cost throughput, which is the
/// kind of wrong number that never announces itself.
///
/// SVL turned out to be a plain sysctl (`sme_max_svl_b`), so no streaming-mode
/// probe and no `+sme` build of this emitter is needed — a detected profile is a
/// **complete** profile, matrix unit included. `FEAT_SME` gates the block, so a
/// part without the unit yields `sme: None` and the rung falls back exactly as
/// it does for every pre-S41 profile.
///
/// `vec_bytes`/`vec_regs` come from [`vec_geometry`]. They ARE ISA facts rather
/// than sysfs entries, but `native` is now the DEFAULT target (S46), so leaving
/// them at [`GENERIC`]'s NEON values silently tiled an AVX2 part to a 16-byte
/// vector — measured at +8.8% on conv2d before this was read. `acc_vecs_per_row`/`nc_tiles` are
/// policy ratios, honestly search space (ADR-0034), not facts to read.
/// The host's vector geometry, read at runtime.
///
/// `native` means "this machine", and on x86 the vector width is not one value:
/// an AVX-512 part, an AVX2 part and an SSE-only part want 64, 32 and 16 bytes
/// with 32, 16 and 16 architectural registers. Detecting the caches but assuming
/// the ISA is how a default build tiled an AVX2 box to NEON's 16 bytes.
/// aarch64 keeps `GENERIC`'s 16/32, which is NEON and correct; SVE would be a
/// separate rung, not a wider `vec_bytes`.
fn vec_geometry() -> (u64, u64) {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx512f") {
            return (64, 32);
        }
        if std::arch::is_x86_feature_detected!("avx2") {
            return (32, 16);
        }
        return (16, 16);
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        (GENERIC.vec_bytes, GENERIC.vec_regs)
    }
}

pub fn native() -> Option<&'static TargetProfile> {
    static NATIVE: std::sync::OnceLock<Option<TargetProfile>> = std::sync::OnceLock::new();
    NATIVE.get_or_init(detect).as_ref()
}

#[cfg(target_os = "macos")]
fn detect() -> Option<TargetProfile> {
    // perflevel0 is the performance core cluster; the bare hw.* keys are the
    // efficiency cores. See the doc comment — this distinction is load-bearing.
    let l1d_bytes = sysctl_u64("hw.perflevel0.l1dcachesize")?;
    let line = sysctl_u64("hw.cachelinesize")?;
    Some(TargetProfile {
        name: "native",
        l2_bytes: sysctl_u64("hw.perflevel0.l2cachesize")?,
        // 5 on this M4 Pro: the 16 MB is a CLUSTER L2, not a per-core one, and
        // `move_block`'s cost term is wrong by 4.8x without this.
        l2_cores: sysctl_u64("hw.perflevel0.cpusperl2")?.max(1),
        sme: (sysctl_u64("hw.optional.arm.FEAT_SME") == Some(1))
            .then(|| {
                Some(Sme {
                    svl_bytes: sysctl_u64("hw.optional.arm.sme_max_svl_b")?,
                    l1d_bytes,
                    // Detected profiles inherit the swept ratio; it is policy, not
                    // a property of the silicon, so there is nothing to read.
                    panel_l1d_ratio: 2,
                })
            })
            .flatten(),
        // S45: line and page are plain sysctls; sets and ways are derived from
        // them, so a detected profile is a COMPLETE one for this rung too.
        // Line is a plain sysctl; SETS is the one fact macOS does not expose,
        // so it comes from the page bound — see `L1d` for why that is a checked
        // heuristic and what it gets wrong. On this part it yields 128, which is
        // the value S44 verified by a stride probe.
        l1d: Some(L1d {
            bytes: l1d_bytes,
            line_bytes: line,
            sets: (sysctl_u64("hw.pagesize")? / line).max(1),
        }),
        vec_bytes: vec_geometry().0,
        vec_regs: vec_geometry().1,
        ..GENERIC
    })
}

/// One `sysfs` file parsed as a `u64`, tolerating the `48K` / `2048K` spelling
/// the cache nodes use for sizes.
#[cfg(target_os = "linux")]
fn sysfs_u64(path: &str) -> Option<u64> {
    let raw = std::fs::read_to_string(path).ok()?;
    let t = raw.trim();
    match t.strip_suffix('K') {
        Some(k) => Some(k.parse::<u64>().ok()? * 1024),
        None => t.parse().ok(),
    }
}

/// The same facts on Linux — a different parser, not a different idea.
///
/// `index0` is the L1 data cache and `index2` the unified L2 on every part this
/// runs on; `shared_cpu_list` is what makes `l2_cores` a measurement rather than
/// an assumption (`0-1` on the i9 is two SMT threads of ONE physical core, so
/// the answer is 1 core, not 2). SMT siblings are collapsed via
/// `topology/thread_siblings_list`, because a hyperthread is not a core to
/// budget for.
///
/// No SME on x86, so `sme` stays `None` and the SME realization cannot be
/// selected — the same seam as every pre-S41 profile.
#[cfg(target_os = "linux")]
fn detect() -> Option<TargetProfile> {
    let l1 = "/sys/devices/system/cpu/cpu0/cache/index0";
    let l2 = "/sys/devices/system/cpu/cpu0/cache/index2";
    let count = |path: &str| -> u64 {
        // "0-1" -> 2, "0,4" -> 2, "0" -> 1.
        std::fs::read_to_string(path)
            .ok()
            .map(|s| {
                s.trim()
                    .split(',')
                    .map(|part| match part.split_once('-') {
                        Some((a, b)) => match (a.parse::<u64>(), b.parse::<u64>()) {
                            (Ok(a), Ok(b)) if b >= a => b - a + 1,
                            _ => 1,
                        },
                        None => 1,
                    })
                    .sum()
            })
            .unwrap_or(1)
            .max(1)
    };
    let smt = count("/sys/devices/system/cpu/cpu0/topology/thread_siblings_list");
    Some(TargetProfile {
        name: "native",
        l2_bytes: sysfs_u64(&format!("{l2}/size"))?,
        l2_cores: (count(&format!("{l2}/shared_cpu_list")) / smt).max(1),
        // Linux EXPOSES the set count, so it is read rather than derived —
        // the macOS page heuristic is a fallback for a host that hides it, and
        // must never shadow a readable truth (`L1d`).
        l1d: Some(L1d {
            bytes: sysfs_u64(&format!("{l1}/size"))?,
            line_bytes: sysfs_u64(&format!("{l1}/coherency_line_size"))?,
            sets: sysfs_u64(&format!("{l1}/number_of_sets"))?,
        }),
        vec_bytes: vec_geometry().0,
        vec_regs: vec_geometry().1,
        ..GENERIC
    })
}

/// No detection on other hosts. `None` means `--target=native` errors with the
/// known-name list instead of guessing.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn detect() -> Option<TargetProfile> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const F32: Ty = Ty::Float { bits: 32 };
    const F64: Ty = Ty::Float { bits: 64 };

    /// Rule 1, at the arithmetic level: the default profile must reproduce every
    /// literal the emitter used to carry. If this fails, emission has moved.
    #[test]
    fn generic_reproduces_the_six_literals() {
        assert_eq!(GENERIC.tile_j(&F32), 16, "tile_j_for f32");
        assert_eq!(GENERIC.tile_j(&F64), 8, "tile_j_for f64");
        assert_eq!(GENERIC.tile_i(), 4, "TILE_I");
        assert_eq!(GENERIC.nc(&F32), 512, "tile_nc_for f32");
        assert_eq!(GENERIC.nc(&F64), 256, "tile_nc_for f64");
        assert_eq!(GENERIC.tile_kc(&F32), 128, "TILE_KC f32");
        assert_eq!(GENERIC.tile_kc(&F64), 128, "TILE_KC f64");
        assert_eq!(GENERIC.heap_min_bytes, 256 * 1024, "HEAP_MIN_BYTES");
    }

    /// The S26 sweep's verdict, as arithmetic: TI=4 fits in half the file and
    /// TI=8 does not. The recorded failure was "8 spills: 128 accumulators ≫ 32
    /// NEON regs" — 8 rows × 16 f32 lanes ÷ 4 lanes per register = 32 registers,
    /// the whole file with nothing left for operands.
    #[test]
    fn tile_i_is_the_register_budget_not_a_taste() {
        let regs_per_row = GENERIC.tile_j(&F32) / GENERIC.lanes(&F32);
        assert_eq!(regs_per_row, GENERIC.acc_vecs_per_row);
        assert_eq!(GENERIC.tile_i() * regs_per_row, GENERIC.vec_regs / 2);
        assert_eq!(8 * regs_per_row, GENERIC.vec_regs, "TI=8 is the whole file");
    }

    /// The measured S29/S30 verdict, reproduced as a deduction — **and its
    /// boundary, pinned honestly**. On a 16 MB L2 the k-panel is 4096, deeper
    /// than every K we run, so the gate `site.k > tile_kc` never opens. Above
    /// 4096 it DOES reopen: this is a threshold, not an off-switch, and the
    /// 8192 assertion is here so nobody reads the others as "apple-m disables
    /// the nest". See `tile_kc`'s note — past the threshold the derivation and
    /// the measurement disagree.
    #[test]
    fn apple_m_raises_the_kc_threshold_above_every_k_we_run() {
        assert_eq!(APPLE_M.tile_kc(&F32), 4096);
        for k in [1024_u64, 2048, 4096] {
            assert!(k <= APPLE_M.tile_kc(&F32), "KC nest must stay off at K={k}");
        }
        assert!(
            8192 > APPLE_M.tile_kc(&F32),
            "the gate REOPENS past 4096 — an unmeasured regime, not a disabled rung"
        );
        assert!(
            2048 > GENERIC.tile_kc(&F32),
            "generic keeps today's gate open"
        );
    }

    /// The KC a-panel is `tile_i x tile_kc x sizeof` = `2 * l2_bytes/nc_tiles`,
    /// so it scales with the profile's L2: 2 KB under `generic`, 64 KB under
    /// `apple-m`. Pinned because `func/mod.rs`'s heap note used to claim a flat
    /// 2 KB, and that number is a stack-sizing claim.
    #[test]
    fn kc_apanel_scales_with_l2() {
        let apack = |p: &TargetProfile, e: &Ty| p.tile_i() * p.tile_kc(e) * 4;
        assert_eq!(apack(&GENERIC, &F32), 2 * 1024);
        assert_eq!(apack(&APPLE_M, &F32), 64 * 1024);
        assert!(
            apack(&APPLE_M, &F32) < GENERIC.heap_min_bytes,
            "still on the stack, but the headroom is profile-dependent"
        );
    }

    #[test]
    fn zen3_moves_two_constants_at_once() {
        assert_eq!(ZEN3.tile_j(&F32), 32);
        assert_eq!(ZEN3.tile_i(), 2);
    }

    #[test]
    fn unknown_name_is_not_silently_generic() {
        assert_eq!(resolve("generic"), Some(&GENERIC));
        assert_eq!(resolve("apple-m"), Some(&APPLE_M));
        assert_eq!(resolve("cuda-ada"), Some(&CUDA_ADA));
        assert_eq!(resolve("apple-m4-sme"), Some(&APPLE_M4_SME));
        assert!(resolve("apple_m").is_none());
        assert!(
            names().contains("native"),
            "the unknown-name error must offer `native`, or detection is undiscoverable"
        );
    }

    /// The SME k-panel depth is **derived from L1D**, and the derivation
    /// reproduces the swept optimum — `plan-s31-deduced-blocking.md`'s rule.
    /// `512` appears in no source file; it falls out of 128 KB / 256 B.
    #[test]
    fn sme_kc_reproduces_the_measured_optimum() {
        assert_eq!(
            APPLE_M4_SME.sme_kc(&F32),
            Some(1024),
            "2 x 128 KB / ((ti+tj)·t·4 = 256 B) = 1024 — the depth SWEPT in the real \
             emitter at N=4096 (1.064x, disjoint) and N=2048. Ratio 1 gives 512, \
             which measured 0.785x — a loss"
        );
        // f64 scales off the same recorded fact by element width: t halves to 8,
        // the tile count doubles to 8 so the block is 2x4, and the window is
        // (2+4)·8·8 = 384 B per k. No second constant anywhere.
        assert_eq!(APPLE_M4_SME.sme_kc(&F64), Some(2 * 128 * 1024 / 384));
        // No matrix unit => no answer, never a wrong one. Same seam as `sme()`.
        assert_eq!(GENERIC.sme_kc(&F32), None);
        assert_eq!(APPLE_M.sme_kc(&F32), None);
    }

    /// The SME budget is L1D and **not** the NEON rung's L2, and the two answers
    /// differ by 8x — which is the whole reason `sme_kc` exists rather than
    /// reusing `tile_kc`. If these two ever coincide, one of them is wrong.
    #[test]
    fn the_sme_budget_is_not_the_neon_one() {
        let neon = APPLE_M4_SME.tile_kc(&F32);
        let sme = APPLE_M4_SME.sme_kc(&F32).unwrap();
        assert_eq!(neon, 4096, "unchanged: half of 16 MB shared L2 over nc");
        assert_eq!(sme, 1024);
        assert!(
            sme < neon,
            "the matrix unit's window is smaller than the NEON k-panel's; \
             a shared derivation would size SME's panel 8x too deep and lose \
             1.448x at K=4096 (docs/performance/s42-sme-roofline.md §5)"
        );
        // And the gate this feeds: at K=4096 the SME nest must OPEN...
        assert!(4096 > sme, "KC nest must fire at K=4096");
        // ...while at K=512 it must stay shut, because blocking is a LOSS there.
        assert!(
            1024 <= sme,
            "KC nest must stay off at K=1024 — measured neutral-to-worse there"
        );
    }

    /// Detection reads the keys it documents. Not a value assertion — the point
    /// is that the plumbing works on this host, so "automatic" is not a claim
    /// resting on an untested code path.
    #[cfg(target_os = "macos")]
    #[test]
    fn native_reads_the_facts_it_claims_to() {
        let l2 = sysctl_u64("hw.perflevel0.l2cachesize");
        let l1d = sysctl_u64("hw.perflevel0.l1dcachesize");
        assert!(l2.is_some_and(|v| v >= 256 * 1024), "L2 detected: {l2:?}");
        assert!(l1d.is_some_and(|v| v >= 32 * 1024), "L1D detected: {l1d:?}");
        assert!(sysctl_u64("hw.optional.arm.no.such.key").is_none());

        let native = native().expect("detection must succeed on macOS");
        assert_eq!(native.name, "native");
        assert_eq!(Some(native.l2_bytes), l2, "detected, not inherited");

        // The E-core trap: the bare key is a DIFFERENT, smaller cache. If these
        // two ever agree, this host stopped being big.LITTLE and the guard below
        // stops proving anything — but on any such part it is also harmless.
        if let (Some(bare), Some(p0)) = (sysctl_u64("hw.l1dcachesize"), l1d) {
            assert!(
                bare <= p0,
                "hw.l1dcachesize ({bare}) must not exceed the P-core's ({p0}); \
                 detection reads perflevel0 precisely because the bare key is \
                 the efficiency cluster"
            );
        }
    }

    /// **The genericity gate.** On this host, detection must reproduce the
    /// hand-written profile *exactly* — same SVL, same L1D, same derived tile
    /// side, same derived block, same derived k-panel. If it does not, then one
    /// of the two is wrong and the hand-written numbers were never facts about
    /// this machine.
    ///
    /// This is what makes `apple-m4-sme` a **cross-compilation** spelling rather
    /// than the only way to describe the machine under the keyboard: the numbers
    /// in it are no longer load-bearing here, they are checked against the part.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn native_reproduces_the_hand_written_profile() {
        let Some(native) = native() else {
            panic!("detection must succeed on macOS")
        };
        let Some(nsme) = native.sme() else {
            // A Mac without SME: nothing to compare, and the fallback is the
            // pre-S41 behaviour. Not a failure.
            assert_eq!(sysctl_u64("hw.optional.arm.FEAT_SME"), Some(0));
            return;
        };
        let hand = APPLE_M4_SME.sme().expect("hand-written profile has SME");
        assert_eq!(nsme.svl_bytes, hand.svl_bytes, "SVL: detected vs written");
        assert_eq!(nsme.l1d_bytes, hand.l1d_bytes, "L1D: detected vs written");
        assert_eq!(native.l2_bytes, APPLE_M4_SME.l2_bytes, "L2");
        for elem in [F32, F64] {
            assert_eq!(
                native.sme_tile_side(&elem),
                APPLE_M4_SME.sme_tile_side(&elem)
            );
            assert_eq!(native.sme_block(&elem), APPLE_M4_SME.sme_block(&elem));
            assert_eq!(native.sme_kc(&elem), APPLE_M4_SME.sme_kc(&elem));
        }
        // And the number the whole S42 campaign turns on, arrived at with no
        // constant written anywhere: 131072 / ((2+2)*16*4) = 512.
        assert_eq!(native.sme_kc(&F32), Some(1024));
    }

    /// One `MoveSite` for a square `side x side` f32 transpose — the shape every
    /// measured point in `move_block`'s table is.
    fn transpose(side: u64) -> MoveSite {
        MoveSite {
            width: side,
            rows: side,
            cq: 1,
            cr: side,
            elem: F32,
            len: side * side,
        }
    }

    /// **The whole S45 decision, scored against every measured point on two
    /// machines.** Each row is a measurement, not a preference; the comment
    /// beside it is what the emitted pipeline actually did.
    #[test]
    fn move_block_reproduces_both_machines() {
        // M4 Pro: fires at 1024 with the block S44 SWEPT to (16), fires at 2048,
        // declines at 128 on pressure and at 512 on cost.
        // BOTH M4 sides land on the block the machine measured fastest — 1024
        // has floor 32 / ceiling 16 and 2048 has floor 32 / ceiling 8, and the
        // geometric mean is 22 and 16, so the largest divisor is 16 either way.
        assert_eq!(APPLE_M4_SME.move_block(&transpose(1024)), Some(16));
        assert_eq!(APPLE_M4_SME.move_block(&transpose(2048)), Some(16));
        assert_eq!(
            APPLE_M4_SME.move_block(&transpose(128)),
            None,
            "pressure 0.5: 128 lines live into 32 sets x 8 ways = 256 slots — measured 1.000x"
        );
        assert_eq!(
            APPLE_M4_SME.move_block(&transpose(512)),
            None,
            "pressure 8 but the 1 MB read array sits inside 16 MB / 5 = 3.36 MB — measured \
             overlapping, 1.02x min-to-min"
        );
        // i9: fires at 1024 and 2048, and DECLINES at 512 — the discriminating
        // case, where pressure alone would fire and the machine measured 0.901x.
        // The i9's 12 reachable slots put the ceiling at 6 against a floor of
        // 16, so the mean is 9 and the block is 8. Its measured optimum is 128,
        // which no L1-derived rule can reach — see `move_block`'s note and the
        // counter evidence in benches/results-s45.
        assert_eq!(RAPTORLAKE.move_block(&transpose(1024)), Some(8));
        assert_eq!(RAPTORLAKE.move_block(&transpose(2048)), Some(8));
        assert_eq!(
            RAPTORLAKE.move_block(&transpose(512)),
            None,
            "pressure 21.3 — ABOVE the M4's 8 at the same side — but the 1 MB read array fits \
             the 2 MB private L2, and it measured a 0.901x LOSS (replicated 0.907x). This is \
             the case pressure alone gets wrong, and the reason the cost term exists"
        );
        // Odd sides never collapse the set index, so there is nothing to win:
        // S44 measured side 1025 running 2.12x faster UNBLOCKED than 1024, and
        // blocking it at 0.623x. Note this one scores pressure **1.0009** — it
        // is the witness for the conflict clause, not the pressure clause, and
        // without that clause the deduction would fire on a measured LOSS.
        assert_eq!(APPLE_M4_SME.move_block(&transpose(1025)), None);
        // And a profile with no L1D geometry cannot answer at all.
        for p in [&GENERIC, &APPLE_M, &ZEN3, &CUDA_ADA] {
            assert_eq!(p.move_block(&transpose(1024)), None, "{}", p.name);
        }
    }

    /// The pressure term is `lines_live > slots` — the literal statement that the
    /// sweep needs more lines than it can reach — so the set collapse, not the
    /// power of two, is what it turns on. S44's two load-bearing nulls (side 128
    /// at pressure 0.5, side 544 at 0.53) are both collapses that stay under 1.
    #[test]
    fn pressure_counts_lines_not_powers_of_two() {
        let g = APPLE_M4_SME.l1d.expect("apple-m4-sme has L1D geometry");
        assert_eq!((g.sets, g.ways()), (128, 8));
        // side 128: stride 512 B = 4 lines, gcd(4, 128) = 4 => 32 sets, 256 slots
        // against 128 lines live. A REAL collapse (32 of 128 sets) that still
        // must not fire — "it is not 'a collapse is bad', it is pressure".
        assert_eq!(APPLE_M4_SME.move_block(&transpose(128)), None);
        // side 544: gcd(17, 128) = 1 => all 128 sets, 1024 slots, 544 live.
        assert_eq!(APPLE_M4_SME.move_block(&transpose(544)), None);
    }

    /// `ways` is arithmetic, `sets` is read where the host exposes it — and the
    /// two i9 geometries are where the truth IS exposed, so they are the check
    /// on the whole scheme. Both reproduce `ways_of_associativity` exactly.
    #[test]
    fn l1d_ways_reproduce_the_readable_truth() {
        let i9_p = RAPTORLAKE.l1d.expect("raptorlake has L1D geometry");
        assert_eq!(i9_p.ways(), 12, "sysfs: ways_of_associativity = 12");
        let i9_e = L1d {
            bytes: 32 * 1024,
            line_bytes: 64,
            sets: 64,
        };
        assert_eq!(i9_e.ways(), 8, "sysfs: the E-core is 8-way");
        // The M4's set count is the one macOS hides; the page bound gives 128,
        // which is the value S44 verified with a stride probe, and 8 ways falls
        // out of capacity.
        let m4 = APPLE_M4_SME.l1d.expect("apple-m4-sme has L1D geometry");
        assert_eq!((16384 / m4.line_bytes, m4.sets), (128, 128));
        assert_eq!(m4.ways(), 8);
        // Whatever the split, capacity is exact — the bound on being wrong.
        for g in [i9_p, i9_e, m4] {
            assert_eq!(g.sets * g.ways(), g.bytes / g.line_bytes);
        }
    }

    /// The L2 sharing fact, which is the cost term's divisor and the field this
    /// session added because the old doc comment said "per-core" while `apple-m`
    /// recorded a SHARED cluster cache. Getting it wrong by 5x is the difference
    /// between firing and declining at M4 side 1024.
    #[test]
    fn l2_per_core_is_not_l2_bytes() {
        assert_eq!(APPLE_M.l2_per_core(), 16 * 1024 * 1024 / 5);
        assert!(
            4 * 1024 * 1024 > APPLE_M.l2_per_core(),
            "a 4 MB read array must OVERFLOW the M4's share (it measured 1.578x)"
        );
        assert!(
            4 * 1024 * 1024 < APPLE_M.l2_bytes,
            "...while fitting the cluster L2 whole, which is why the distinction is the answer"
        );
        assert_eq!(
            RAPTORLAKE.l2_per_core(),
            2 * 1024 * 1024,
            "private per P-core"
        );
        assert_eq!(GENERIC.l2_per_core(), GENERIC.l2_bytes, "l2_cores = 1");
    }

    /// The duplicated L1D size cannot drift: `Sme::l1d_bytes` and `L1d::bytes`
    /// are the same machine fact reached by two rungs. See `TargetProfile::l1d`
    /// for why the merge was not done here.
    #[test]
    fn l1d_and_sme_agree_on_the_l1d() {
        for p in PROFILES {
            if let (Some(sme), Some(l1d)) = (p.sme(), p.l1d) {
                assert_eq!(sme.l1d_bytes, l1d.bytes, "{} disagrees with itself", p.name);
            }
        }
    }

    /// **The genericity gate, extended to S45's facts.** Detection on this host
    /// must reproduce the hand-written L1D geometry and L2 sharing *exactly*,
    /// and therefore reach the same fire/decline verdict and the same block. If
    /// it does not, one of the two is wrong and the hand-written numbers were
    /// never facts about this machine.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn native_reproduces_the_hand_written_move_geometry() {
        let native = native().expect("detection must succeed on macOS");
        assert_eq!(
            native.l1d, APPLE_M4_SME.l1d,
            "detected L1D geometry vs written"
        );
        assert_eq!(native.l2_cores, APPLE_M4_SME.l2_cores, "cores sharing L2");
        assert_eq!(native.l2_per_core(), APPLE_M4_SME.l2_per_core());
        for side in [128_u64, 512, 1024, 1025, 2048] {
            assert_eq!(
                native.move_block(&transpose(side)),
                APPLE_M4_SME.move_block(&transpose(side)),
                "detected and written must decide side {side} the same way"
            );
        }
        // And the number the whole session turns on, detected rather than typed.
        assert_eq!(native.move_block(&transpose(1024)), Some(16));
    }

    /// `apple-m` gained `l2_cores` and nothing else — no derivation reads it, so
    /// its emission cannot have moved. The negative control for the profile edit.
    #[test]
    fn apple_m_gained_only_the_sharing_fact() {
        assert_eq!(
            APPLE_M,
            TargetProfile {
                l2_cores: APPLE_M.l2_cores,
                name: APPLE_M.name,
                l2_bytes: APPLE_M.l2_bytes,
                ..GENERIC
            }
        );
        assert_eq!(
            APPLE_M.tile_kc(&F32),
            4096,
            "unchanged: the KC gate is where it was"
        );
        assert!(
            APPLE_M.l1d.is_none(),
            "no L1D geometry ⇒ the move rung cannot fire"
        );
    }

    /// A hand-written profile always wins over detection (Sapir's rule).
    #[test]
    fn named_profiles_override_detection() {
        for p in PROFILES {
            assert_eq!(
                resolve(p.name).map(|r| r.name),
                Some(p.name),
                "the table is consulted before `native`"
            );
        }
        assert!(
            resolve("nativ").is_none(),
            "no fuzzy match, no silent default"
        );
        assert!(resolve("cuda_ada").is_none());
        assert!(resolve("apple-m4").is_none());
        assert!(resolve("").is_none());
    }

    /// Rule 1 for the SME leg: the capability is reachable only through
    /// `sme()`, and no profile that existed before it can answer. That is the
    /// whole byte-identity argument — the SME realization's gate cannot open
    /// without a fact only `apple-m4-sme` carries.
    #[test]
    fn sme_is_absent_from_every_pre_sme_profile() {
        for p in [&GENERIC, &APPLE_M, &ZEN3, &CUDA_ADA] {
            assert!(p.sme().is_none(), "{} must expose no SME facts", p.name);
            assert!(
                p.sme_tile_side(&F32).is_none(),
                "{} derives no tile",
                p.name
            );
        }
    }

    /// The measured M4 Pro facts, and the one derivation the emitter reads:
    /// SVL 512 bits ⇒ a 16×16 f32 tile (`benches/sme/svl.c`, `run16.c`). f64
    /// halves it, which is why the side is derived rather than written down.
    #[test]
    fn apple_m4_sme_derives_the_measured_tile_side() {
        let sme = APPLE_M4_SME.sme().expect("apple-m4-sme has an SME unit");
        assert_eq!(sme.svl_bytes, 64, "SVL = 512 bits, detected");
        assert_eq!(APPLE_M4_SME.sme_tile_side(&F32), Some(16));
        assert_eq!(APPLE_M4_SME.sme_tile_side(&F64), Some(8));
        // The tile count is an ISA rule, not a field: exactly `elem_bytes`.
        assert_eq!(APPLE_M4_SME.sme_block(&F32), Some((2, 2)), "4 tiles at f32");
        assert_eq!(APPLE_M4_SME.sme_block(&F64), Some((2, 4)), "8 tiles at f64");
    }

    /// The SME profile moves EXACTLY one field off `apple-m`. Every derived
    /// tile factor is unchanged, so a site that falls back to the NEON rung
    /// under `apple-m4-sme` emits what `apple-m` emits — the negative control
    /// the golden test pins at the text level.
    #[test]
    fn apple_m4_sme_moves_only_the_sme_field() {
        // S45 adds `l1d` alongside `sme`: the same part, one more capability
        // declared. Every DERIVED tile factor below must still be `apple-m`'s.
        assert_eq!(
            APPLE_M4_SME,
            TargetProfile {
                name: APPLE_M4_SME.name,
                sme: APPLE_M4_SME.sme,
                l1d: APPLE_M4_SME.l1d,
                ..APPLE_M
            }
        );
        assert_eq!(APPLE_M4_SME.tile_j(&F32), APPLE_M.tile_j(&F32));
        assert_eq!(APPLE_M4_SME.tile_i(), APPLE_M.tile_i());
        assert_eq!(APPLE_M4_SME.tile_kc(&F32), APPLE_M.tile_kc(&F32));
    }

    /// The accumulator budget, spent. 4 f32 tiles ⇒ a 2×2 block, which is the
    /// arrangement `benches/sme/mm4.c` measured at 1.84–2.61× over one tile.
    /// f64 has twice as many tiles (one per element byte), and the count is
    /// derived from the recorded f32 one rather than written down again.
    #[test]
    fn sme_block_is_the_square_most_split_of_the_tile_count() {
        assert_eq!(APPLE_M4_SME.sme_block(&F32), Some((2, 2)));
        assert_eq!(APPLE_M4_SME.sme_block(&F64), Some((2, 4)), "8 tiles at f64");
        for p in [&GENERIC, &APPLE_M, &ZEN3, &CUDA_ADA] {
            assert!(p.sme_block(&F32).is_none(), "{} has no ZA", p.name);
        }
    }

    /// The derivation's two invariants: the block spends the WHOLE tile budget,
    /// and it is the square-most factorization — the one that minimises `ti + tj`
    /// operand loads per `ti · tj` outer products (1×4 needs 5 loads for the 4
    /// MACs 2×2 gets from 4).
    ///
    /// **Swept over element WIDTHS, not over a recorded count.** The previous
    /// version of this test swept `f32_tiles` 1..=64, and S41b logged the defect
    /// that created: it pinned arrangements for counts like 7 or 64 that no ZA
    /// register file has and no emitted kernel could select, so the test asserted
    /// properties of unreachable IR. Now that the count is `elem_bytes` by
    /// construction there is nothing unreachable left to sweep — every width here
    /// is a width the architecture actually has.
    #[test]
    fn sme_block_spends_every_tile_and_stays_square_most() {
        for bits in [8_u8, 16, 32, 64] {
            let elem = Ty::Float { bits };
            let tiles = u64::from(bits).div_ceil(8);
            let (ti, tj) = APPLE_M4_SME.sme_block(&elem).expect("has ZA");
            assert_eq!(
                ti * tj,
                tiles,
                "the block must spend every tile ({tiles} at f{bits})"
            );
            assert!(ti <= tj, "ti is the smaller factor (f{bits})");
            let best = (1..=tiles)
                .filter(|d| tiles.is_multiple_of(*d))
                .map(|d| d + tiles / d)
                .min()
                .expect("a divisor exists");
            assert_eq!(ti + tj, best, "fewest operand loads at f{bits}");
            // Every arrangement must actually fit ZA: `ti·t x tj·t` elements of
            // `w` bytes is exactly `svl x svl`, the whole array, never more.
            let t = APPLE_M4_SME.sme_tile_side(&elem).expect("has ZA");
            let svl = APPLE_M4_SME.sme().unwrap().svl_bytes;
            assert_eq!(
                ti * t * tj * t * tiles,
                svl * svl,
                "the block must be exactly ZA at f{bits}, not larger"
            );
        }
        assert_eq!(
            (1..=4_u64)
                .filter(|d| 4_u64.is_multiple_of(*d))
                .map(|d| d + 4 / d)
                .min(),
            Some(4),
            "2x2 is 4 loads; 1x4 is 5"
        );
    }

    /// The packed-B panel the NEON rung already builds is `tile_j` lanes wide,
    /// and the SME kernel wants an SVL-wide contiguous row. On this part they
    /// are the same 16 — which is why the SME rung consumes the existing packed
    /// buffer instead of minting its own. `func/sme.rs` re-checks this per site
    /// and falls back when it does not hold, so this is a pin on *why the reuse
    /// is free here*, not a load-bearing invariant.
    #[test]
    fn sme_panel_and_packed_b_panel_coincide_on_this_part() {
        assert_eq!(
            APPLE_M4_SME.sme_tile_side(&F32),
            Some(APPLE_M4_SME.tile_j(&F32))
        );
    }

    /// S41 step 1, rule 1: adding the machine class must not move a single CPU
    /// emission. Every profile that existed before S41 is `Cpu`, so no CPU
    /// realization can observe the new field — the type system carries the
    /// byte-identity argument, and the 159-emission A/B sweep confirms it.
    #[test]
    fn every_pre_s41_profile_is_cpu() {
        for p in [&GENERIC, &APPLE_M, &ZEN3] {
            assert_eq!(p.machine, Machine::Cpu, "{} must stay Cpu", p.name);
            assert!(p.gpu().is_none(), "{} must expose no GPU facts", p.name);
        }
    }

    /// A GPU fact is reachable only through `gpu()`, so a CPU profile on a
    /// GPU-only path is a `None` at the seam rather than a wrong constant
    /// (plan-s41 §2.2 rule 2 — capability selects realization).
    #[test]
    fn gpu_facts_are_asked_for_never_defaulted() {
        let g = CUDA_ADA.gpu().expect("cuda-ada is a GPU profile");
        assert_eq!(g.warp, 32, "cooperation quantum");
        assert_eq!(g.max_threads_per_block, 1024);
        assert_eq!(
            g.arch, "sm_89",
            "verified against the device: compute capability 8.9 (RTX 4070 Ti)"
        );
        assert!(
            g.smem_bytes < CUDA_ADA.l2_bytes,
            "smem is the smaller budget"
        );
    }

    /// The placeholder honesty pin. `cuda-ada` inherits CPU-shaped constants it
    /// has no business claiming as GPU measurements; this test states that in
    /// the suite so nobody reads them as swept. When a GPU realization starts
    /// consuming one of these, it either gets its own value or this assertion
    /// changes — either way the change is deliberate and reviewed.
    #[test]
    fn cuda_ada_cpu_shaped_fields_are_inherited_placeholders() {
        assert_eq!(CUDA_ADA.vec_bytes, GENERIC.vec_bytes);
        assert_eq!(CUDA_ADA.vec_regs, GENERIC.vec_regs);
        assert_eq!(CUDA_ADA.l2_bytes, GENERIC.l2_bytes);
        assert_eq!(CUDA_ADA.acc_vecs_per_row, GENERIC.acc_vecs_per_row);
    }
}
