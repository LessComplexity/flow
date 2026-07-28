//! DESIGN §6 — the random Core-program generator (HANDOFF §9: lives here, feeds
//! P5–P7 differential tests). proptest yields a **Clone-able script** (`CategoryIr`
//! is not `Clone`, so it cannot be a strategy value); [`build`] interprets the
//! script into a sealed graph. Seal must always succeed — a generator that emits
//! an ill-typed step is a bug, surfaced as an `unwrap` panic.
//!
//! Two strategies (both modes):
//! - **closed** (`open == false`): an entry `main`, effectful (token-threaded
//!   prints of intermediates) or pure, over a DAG of scalar/tuple/array ops,
//!   helper fns, and canonical bounded loops (`i < K`, `K ≤ 64` — terminating).
//! - **open** (`open == true`): a pure `Named` fn `i32 → i32`, exercised via
//!   `eval_call` with random `i32` args.
//!
//! Modes: `default` (traps permitted — `Div`/`Mod`/`Index` on arbitrary feeders)
//! and `trap_free` (divisors const-nonzero, indices const-in-bounds).
//!
//! ADR-0027 captures: dedicated main-level steps emit `map_captured` /
//! `fold_captured` over body pools whose inputs are the `(c₁…cₖ, …)` products
//! — a scalar capture, an array capture (body indexes it at constant in-bounds
//! indices), a fold capture, a fold nested in a map body capturing across two
//! levels (the matmul miniature), and a loop-carried scalar captured by a map
//! body (the read-at-position case). All are trap-safe by construction in
//! `trap_free` mode and loop-free inside bodies (fusion P3 stays reachable).
#![allow(dead_code)]

use mapal_interp::RValue;
use mapal_ir::{CategoryIr, Dest, FuncId, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value};
use proptest::prelude::*;

thread_local! {
    /// Monotonic source-position counter, reset at the top of [`build`].
    static NEXT_LOC: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// The next source position, one byte wide.
///
/// Generated programs have no source text, but they DO have a statement order —
/// the order this generator emits them — and since plan-s38 that order is
/// semantically load-bearing: `topo_order` breaks ties on source position, so a
/// corpus stamping every statement `0..0` carries no order at all and the
/// tie-break degenerates straight back to insertion order (which is the thing
/// under test). A monotonic counter makes "position order == emission order",
/// i.e. exactly the statement order these programs model. Reset per `build`, so
/// a given `Prog` always yields identical positions.
#[allow(non_snake_case)]
fn L() -> SourceLoc {
    NEXT_LOC.with(|c| {
        let at = c.get();
        c.set(at + 1);
        SourceLoc::new(at, at + 1)
    })
}
/// Every generated array is `[i32; ARR]` — one fixed size keeps `zip`/`index`/
/// `map`/`fold`/`enumerate` typing trivial and always valid.
const ARR: usize = 3;

/// One value-producing step over the typed pools. Indices are reduced modulo
/// the live pool length, so a step with no operand of the right type is skipped.
#[derive(Clone, Debug)]
pub enum Step {
    ConstI32(i32),
    ConstBool(bool),
    Bin {
        op: u8,
        a: u8,
        b: u8,
    },
    Neg {
        a: u8,
    },
    /// ADR-0029: one of the four legal widening-lattice edges.
    Widen {
        edge: u8,
        a: u8,
    },
    /// ADR-0029 array construction (S21): `iota(3)` joins the `[i32; 3]` pool.
    Iota,
    /// ADR-0029 (S21): `fill(x, 3)` from an i32-pool value joins the arr pool.
    Fill {
        a: u8,
    },
    Cmp {
        op: u8,
        a: u8,
        b: u8,
    },
    Not {
        a: u8,
    },
    Logic {
        or: bool,
        a: u8,
        b: u8,
    },
    Phi {
        t: u8,
        e: u8,
        c: u8,
    },
    /// plan-s39: a guard whose arm owns work that can TRAP — a `Div`/`Mod`
    /// consumed ONLY by one Phi arm. `Step::Phi` cannot produce this: it picks
    /// both arms from the pool, so an arm never owns its producer, and a census
    /// over this corpus found 0 of 82 guard sites with a trapping arm. That
    /// hole is why an untaken `7 / 0` reached production.
    PhiTrapArm {
        a: u8,
        b: u8,
        c: u8,
        /// Which arm owns the trapping op.
        on_true: bool,
        /// `Mod` instead of `Div`.
        modulo: bool,
    },
    PackProj {
        a: u8,
        b: u8,
        snd: bool,
    },
    MakeArray {
        a: u8,
        b: u8,
        c: u8,
    },
    Index {
        arr: u8,
        idx: u8,
    },
    Update {
        arr: u8,
        idx: u8,
        val: u8,
    },
    MapArr {
        arr: u8,
        body: u8,
    },
    FoldArr {
        arr: u8,
        seed: u8,
        body: u8,
    },
    Zip {
        a: u8,
        b: u8,
    },
    Enumerate {
        arr: u8,
    },
    Call {
        a: u8,
        helper: u8,
    },
    Loop {
        k: u8,
    },
    /// R-LF: canonical `(counter, acc)` with static `K >= 1`.
    LiftFold {
        k: u8,
        seed: u8,
        cap: u8,
    },
    /// R-LM: canonical `(out, counter)` with one identity-indexed Update and
    /// `len(out) == K == ARR`.
    LiftMap {
        arr: u8,
        cap: u8,
    },
    /// ADR-0027: map with an i32 capture from the scalar pool; body
    /// `(cap, elem) -> i32` (the `x * scale` shape).
    MapCapScalar {
        arr: u8,
        cap: u8,
        body: u8,
    },
    /// ADR-0027: map with an `[i32; ARR]` capture from the array pool; body
    /// `([i32; ARR], elem) -> i32` indexing the capture in-bounds.
    MapCapArray {
        arr: u8,
        cap: u8,
        body: u8,
    },
    /// ADR-0027: fold with an i32 capture; body `(cap, acc, elem) -> i32`.
    FoldCapScalar {
        arr: u8,
        seed: u8,
        cap: u8,
        body: u8,
    },
    /// ADR-0027: map whose body folds the map's own array capture, capturing
    /// the outer element inside the fold — capture across two levels (the
    /// matmul shape in miniature). Body `([i32; ARR], elem) -> i32`.
    MapNestFold {
        arr: u8,
        cap: u8,
        body: u8,
    },
    /// ADR-0027: a canonical bounded loop, then a map capturing the loop's
    /// exit value — the loop-carried read-at-position case.
    LoopCapMap {
        k: u8,
        arr: u8,
        body: u8,
    },
}

/// A generated fold-in-map body (ADR-0027 two-level capture): the inner fold
/// body script `(cap, acc, elem) -> i32`, then the outer map body's own scalar
/// steps (run after the fold, over `[elem, fold_result, …]`).
#[derive(Clone, Debug)]
pub struct NestBody {
    pub inner: Vec<Step>,
    pub outer: Vec<Step>,
}

/// A full generated program script (Clone, for proptest shrinking).
#[derive(Clone, Debug)]
pub struct Prog {
    pub trap_free: bool,
    pub open: bool,
    /// Effectful (token-threaded prints) vs pure, for a closed `main`.
    pub effectful: bool,
    pub helpers: Vec<Vec<Step>>,     // Named   i32 -> i32
    pub map_bodies: Vec<Vec<Step>>,  // MapBody  i32 -> i32
    pub fold_bodies: Vec<Vec<Step>>, // FoldBody (i32,i32) -> i32
    /// ADR-0027 body pools: scalar-capture map bodies `(i32,i32) -> i32`,
    /// array-capture map bodies `([i32;ARR],i32) -> i32`, scalar-capture fold
    /// bodies `(i32,i32,i32) -> i32`, and fold-in-map nest bodies.
    pub map_cap_bodies: Vec<Vec<Step>>,
    pub map_acap_bodies: Vec<Vec<Step>>,
    pub fold_cap_bodies: Vec<Vec<Step>>,
    pub nest_bodies: Vec<NestBody>,
    pub main: Vec<Step>,
    pub prints: Vec<u8>,
    pub ret: u8,
    pub args: Vec<i32>,
}

/// A built program plus how to run it (open ⇒ `eval_call` per arg; closed ⇒ `run`).
pub struct Built {
    pub ir: CategoryIr,
    pub open: bool,
    pub entry: FuncId,
    pub args: Vec<RValue>,
}

// --- strategies -----------------------------------------------------------

/// A small-int-biased `i32` constant strategy (DESIGN §6: ±100 + edge values).
fn const_i32() -> impl Strategy<Value = i32> {
    prop_oneof![
        3 => -100i32..=100,
        1 => prop::sample::select(vec![0, 1, -1, 2, -2, i32::MIN, i32::MAX, 100, -100]),
    ]
}

/// A restricted step (scalar ops only) — for map/fold/helper bodies, which must
/// stay loop-free and collection-free (totality + fusion P3).
fn scalar_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        const_i32().prop_map(Step::ConstI32),
        any::<bool>().prop_map(Step::ConstBool),
        (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(op, a, b)| Step::Bin { op, a, b }),
        any::<u8>().prop_map(|a| Step::Neg { a }),
        (any::<u8>(), any::<u8>()).prop_map(|(edge, a)| Step::Widen { edge, a }),
        Just(Step::Iota),
        any::<u8>().prop_map(|a| Step::Fill { a }),
        (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(op, a, b)| Step::Cmp { op, a, b }),
        any::<u8>().prop_map(|a| Step::Not { a }),
        (any::<bool>(), any::<u8>(), any::<u8>()).prop_map(|(or, a, b)| Step::Logic { or, a, b }),
        (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(t, e, c)| Step::Phi { t, e, c }),
        (
            any::<u8>(),
            any::<u8>(),
            any::<u8>(),
            any::<bool>(),
            any::<bool>()
        )
            .prop_map(|(a, b, c, on_true, modulo)| Step::PhiTrapArm {
                a,
                b,
                c,
                on_true,
                modulo
            }),
        (any::<u8>(), any::<u8>(), any::<bool>()).prop_map(|(a, b, snd)| Step::PackProj {
            a,
            b,
            snd
        }),
    ]
}

/// A full step (adds collections + loops + ADR-0027 captures) — for `main` only.
fn main_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        6 => scalar_step(),
        1 => (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(a, b, c)| Step::MakeArray { a, b, c }),
        1 => (any::<u8>(), any::<u8>()).prop_map(|(arr, idx)| Step::Index { arr, idx }),
        1 => (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(arr, idx, val)| Step::Update { arr, idx, val }),
        1 => (any::<u8>(), any::<u8>()).prop_map(|(arr, body)| Step::MapArr { arr, body }),
        1 => (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(arr, seed, body)| Step::FoldArr { arr, seed, body }),
        1 => (any::<u8>(), any::<u8>()).prop_map(|(a, b)| Step::Zip { a, b }),
        1 => any::<u8>().prop_map(|arr| Step::Enumerate { arr }),
        1 => (any::<u8>(), any::<u8>()).prop_map(|(a, helper)| Step::Call { a, helper }),
        1 => any::<u8>().prop_map(|k| Step::Loop { k }),
        1 => (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(k, seed, cap)| Step::LiftFold { k, seed, cap }),
        1 => (any::<u8>(), any::<u8>()).prop_map(|(arr, cap)| Step::LiftMap { arr, cap }),
        1 => (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(arr, cap, body)| Step::MapCapScalar { arr, cap, body }),
        1 => (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(arr, cap, body)| Step::MapCapArray { arr, cap, body }),
        1 => (any::<u8>(), any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(arr, seed, cap, body)| Step::FoldCapScalar { arr, seed, cap, body }),
        1 => (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(arr, cap, body)| Step::MapNestFold { arr, cap, body }),
        1 => (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(k, arr, body)| Step::LoopCapMap { k, arr, body }),
    ]
}

fn body_strategy() -> impl Strategy<Value = Vec<Step>> {
    prop::collection::vec(scalar_step(), 0..5)
}

/// A nest-body script: the inner fold body + the outer map body's own steps.
fn nest_body_strategy() -> impl Strategy<Value = NestBody> {
    (body_strategy(), body_strategy()).prop_map(|(inner, outer)| NestBody { inner, outer })
}

/// The full program strategy for a given mode/openness (DESIGN §6).
pub fn prog_strategy(trap_free: bool, open: bool) -> impl Strategy<Value = Prog> {
    (
        prop::collection::vec(body_strategy(), 0..2), // helpers
        prop::collection::vec(body_strategy(), 0..3), // map bodies
        prop::collection::vec(body_strategy(), 0..3), // fold bodies
        prop::collection::vec(main_step(), 0..12),    // main
        prop::collection::vec(any::<u8>(), 0..4),     // prints
        any::<bool>(),                                // effectful
        any::<u8>(),                                  // ret pick
        prop::collection::vec(const_i32(), 1..4),     // open args
        // ADR-0027 capture body pools.
        (
            prop::collection::vec(body_strategy(), 0..2), // map_cap_bodies
            prop::collection::vec(body_strategy(), 0..2), // map_acap_bodies
            prop::collection::vec(body_strategy(), 0..2), // fold_cap_bodies
            prop::collection::vec(nest_body_strategy(), 0..2), // nest_bodies
        ),
    )
        .prop_map(
            move |(helpers, map_bodies, fold_bodies, main, prints, effectful, ret, args, caps)| {
                let (map_cap_bodies, map_acap_bodies, fold_cap_bodies, nest_bodies) = caps;
                Prog {
                    trap_free,
                    open,
                    effectful: !open && effectful,
                    helpers,
                    map_bodies,
                    fold_bodies,
                    map_cap_bodies,
                    map_acap_bodies,
                    fold_cap_bodies,
                    nest_bodies,
                    main,
                    prints,
                    ret,
                    args,
                }
            },
        )
}

// --- build ----------------------------------------------------------------

/// Referenced functions available to `main`'s steps.
struct Ctx {
    helpers: Vec<FuncId>,    // i32 -> i32
    map_bodies: Vec<FuncId>, // i32 -> i32
    fold_bodies: Vec<FuncId>,
    map_cap_bodies: Vec<FuncId>,  // (i32, i32) -> i32        (ADR-0027)
    map_acap_bodies: Vec<FuncId>, // ([i32;ARR], i32) -> i32  (ADR-0027)
    fold_cap_bodies: Vec<FuncId>, // (i32, i32, i32) -> i32   (ADR-0027)
    nest_bodies: Vec<FuncId>,     // ([i32;ARR], i32) -> i32, fold inside (ADR-0027)
    pair_sum: FuncId,             // (i32,i32) -> i32 utility, for zip/enumerate results
    trap_free: bool,
}

impl Ctx {
    /// The in-body context: bodies reference no other fns (loop-free,
    /// collection-free — fusion P3 stays reachable). `pair_sum` is a dummy
    /// (scalar bodies never emit zip/enumerate).
    fn bodies(pair_sum: FuncId, trap_free: bool) -> Ctx {
        Ctx {
            helpers: vec![],
            map_bodies: vec![],
            fold_bodies: vec![],
            map_cap_bodies: vec![],
            map_acap_bodies: vec![],
            fold_cap_bodies: vec![],
            nest_bodies: vec![],
            pair_sum,
            trap_free,
        }
    }
}

/// Typed value pools threaded through step emission.
#[derive(Default)]
struct Pool {
    i32s: Vec<mapal_ir::ObjectId>,
    i64s: Vec<mapal_ir::ObjectId>,
    f32s: Vec<mapal_ir::ObjectId>,
    f64s: Vec<mapal_ir::ObjectId>,
    bools: Vec<mapal_ir::ObjectId>,
    arrs: Vec<mapal_ir::ObjectId>, // [i32; ARR]
    /// Count of loops emitted so far. The interp scopes loop layout per-merge
    /// SCC (S12 fix: two sequential loops are supported — `interp
    /// tests/loop_invariants.rs::two_sequential_loops`), so up to `MAX_LOOPS`
    /// sequential loops are generable (the S12 P0 / llvm-review-F1 shape); each
    /// is a self-contained canonical quartet, so they nest not, only sequence.
    loops_used: u8,
}

/// Sequential loops per function (S12 P0 shape needs ≥ 2; kept small so build
/// stays cheap). Each `build_loop` is statically bounded (`i < k`, `k ≤ 64`).
const MAX_LOOPS: u8 = 2;

fn pick<T: Copy>(v: &[T], i: u8) -> Option<T> {
    if v.is_empty() {
        None
    } else {
        Some(v[i as usize % v.len()])
    }
}

/// Interpret a script into a sealed IR. Panics on any seal failure (generator bug).
pub fn build(prog: &Prog) -> Built {
    // Positions are per-program, so a given `Prog` always builds the same graph.
    NEXT_LOC.with(|c| c.set(0));
    let mut b = IrBuilder::new();

    // Utility body: (i32,i32) -> i32 { π0 + π1 } — reduces zip/enumerate pairs.
    let pair_sum = b
        .declare(FuncKind::MapBody, "pair_sum", pair_ty(), Ty::i32(), L())
        .unwrap();
    {
        let mut fb = b.build_fn(pair_sum).unwrap();
        let p = fb.input();
        let a = fb.proj(p, 0, Dest::Fresh(None), L()).unwrap();
        let c = fb.proj(p, 1, Dest::Fresh(None), L()).unwrap();
        fb.binop(Operation::Add, a, c, Dest::Ret { slot: None }, L())
            .unwrap();
        fb.finish().unwrap();
    }

    // Helper Named fns (i32 -> i32) and map bodies (i32 -> i32): loop-free scalar.
    let helpers = declare_scalar_bodies(&mut b, FuncKind::Named, "helper", &prog.helpers);
    let map_bodies = declare_scalar_bodies(&mut b, FuncKind::MapBody, "mapb", &prog.map_bodies);
    // Fold bodies ((i32,i32) -> i32).
    let mut fold_bodies = Vec::new();
    for (i, steps) in prog.fold_bodies.iter().enumerate() {
        let f = b
            .declare(
                FuncKind::FoldBody,
                &format!("foldb{i}"),
                pair_ty(),
                Ty::i32(),
                L(),
            )
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let p = fb.input();
            let acc = fb.proj(p, 0, Dest::Fresh(None), L()).unwrap();
            let elem = fb.proj(p, 1, Dest::Fresh(None), L()).unwrap();
            let ctx = Ctx::bodies(pair_sum, prog.trap_free);
            let mut pool = Pool {
                i32s: vec![acc, elem],
                ..Default::default()
            };
            emit_steps(&mut fb, &ctx, &mut pool, steps, false);
            let ret = pool.i32s.last().copied().unwrap_or(acc);
            fb.output(ret, None, L()).unwrap();
            fb.finish().unwrap();
        }
        fold_bodies.push(f);
    }

    // ADR-0027 capture body pools.
    let map_cap_bodies =
        declare_map_cap_bodies(&mut b, "mapc", &prog.map_cap_bodies, prog.trap_free);
    let map_acap_bodies =
        declare_map_acap_bodies(&mut b, "mapa", &prog.map_acap_bodies, prog.trap_free);
    let fold_cap_bodies =
        declare_fold_cap_bodies(&mut b, "foldc", &prog.fold_cap_bodies, prog.trap_free);
    let nest_bodies = declare_nest_bodies(&mut b, &prog.nest_bodies, prog.trap_free);

    let ctx = Ctx {
        helpers,
        map_bodies,
        fold_bodies,
        map_cap_bodies,
        map_acap_bodies,
        fold_cap_bodies,
        nest_bodies,
        pair_sum,
        trap_free: prog.trap_free,
    };

    // main.
    if prog.open {
        build_open_main(&mut b, &ctx, prog)
    } else {
        build_closed_main(&mut b, &ctx, prog)
    }
}

/// Declare + emit a set of scalar `i32 -> i32` bodies of the given kind.
fn declare_scalar_bodies(
    b: &mut IrBuilder,
    kind: FuncKind,
    prefix: &str,
    scripts: &[Vec<Step>],
) -> Vec<FuncId> {
    let mut out = Vec::new();
    for (i, steps) in scripts.iter().enumerate() {
        let f = b
            .declare(kind, &format!("{prefix}{i}"), Ty::i32(), Ty::i32(), L())
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let x = fb.input();
            // Historical: scalar bodies keep default-mode traps even under
            // `trap_free` (pre-ADR-0027 behavior; the harnesses expect 101s).
            let ctx = Ctx::bodies(f, false);
            let mut pool = Pool {
                i32s: vec![x],
                ..Default::default()
            };
            emit_steps(&mut fb, &ctx, &mut pool, steps, false);
            let ret = pool.i32s.last().copied().unwrap_or(x);
            fb.output(ret, None, L()).unwrap();
            fb.finish().unwrap();
        }
        out.push(f);
    }
    out
}

/// Declare + emit scalar-capture map bodies `(cap, elem) -> i32` (ADR-0027):
/// the pool seeds with both input projections, the generated scalar steps
/// combine them — the enclosing-scalar-read shape.
fn declare_map_cap_bodies(
    b: &mut IrBuilder,
    prefix: &str,
    scripts: &[Vec<Step>],
    trap_free: bool,
) -> Vec<FuncId> {
    let mut out = Vec::new();
    for (i, steps) in scripts.iter().enumerate() {
        let f = b
            .declare(
                FuncKind::MapBody,
                &format!("{prefix}{i}"),
                pair_ty(),
                Ty::i32(),
                L(),
            )
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let p = fb.input();
            let cap = fb.proj(p, 0, Dest::Fresh(None), L()).unwrap();
            let elem = fb.proj(p, 1, Dest::Fresh(None), L()).unwrap();
            let ctx = Ctx::bodies(f, trap_free);
            let mut pool = Pool {
                i32s: vec![cap, elem],
                ..Default::default()
            };
            emit_steps(&mut fb, &ctx, &mut pool, steps, false);
            let ret = pool.i32s.last().copied().unwrap_or(elem);
            fb.output(ret, None, L()).unwrap();
            fb.finish().unwrap();
        }
        out.push(f);
    }
    out
}

/// Declare + emit array-capture map bodies `(cap_arr, elem) -> i32` (ADR-0027):
/// the pool seeds with `elem` plus every cell of the captured array, read at
/// **constant in-bounds** indices — the capture-indexing shape, trap-free by
/// construction in both modes.
fn declare_map_acap_bodies(
    b: &mut IrBuilder,
    prefix: &str,
    scripts: &[Vec<Step>],
    trap_free: bool,
) -> Vec<FuncId> {
    let arr_ty = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: ARR as u64,
    };
    let mut out = Vec::new();
    for (i, steps) in scripts.iter().enumerate() {
        let f = b
            .declare(
                FuncKind::MapBody,
                &format!("{prefix}{i}"),
                Ty::Tuple(vec![arr_ty.clone(), Ty::i32()]),
                Ty::i32(),
                L(),
            )
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let p = fb.input();
            let cap_arr = fb.proj(p, 0, Dest::Fresh(None), L()).unwrap();
            let elem = fb.proj(p, 1, Dest::Fresh(None), L()).unwrap();
            let ctx = Ctx::bodies(f, trap_free);
            let mut pool = Pool {
                i32s: vec![elem],
                ..Default::default()
            };
            for j in 0..ARR {
                let idx = fb.constant(Value::I32(j as i32), L()).unwrap();
                let cell = fb.index(cap_arr, idx, Dest::Fresh(None), L()).unwrap();
                pool.i32s.push(cell);
            }
            emit_steps(&mut fb, &ctx, &mut pool, steps, false);
            let ret = pool.i32s.last().copied().unwrap_or(elem);
            fb.output(ret, None, L()).unwrap();
            fb.finish().unwrap();
        }
        out.push(f);
    }
    out
}

/// Declare + emit scalar-capture fold bodies `(cap, acc, elem) -> i32`
/// (ADR-0027): the pool seeds with all three input projections.
fn declare_fold_cap_bodies(
    b: &mut IrBuilder,
    prefix: &str,
    scripts: &[Vec<Step>],
    trap_free: bool,
) -> Vec<FuncId> {
    let in_ty = Ty::Tuple(vec![Ty::i32(), Ty::i32(), Ty::i32()]);
    let mut out = Vec::new();
    for (i, steps) in scripts.iter().enumerate() {
        let f = b
            .declare(
                FuncKind::FoldBody,
                &format!("{prefix}{i}"),
                in_ty.clone(),
                Ty::i32(),
                L(),
            )
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let p = fb.input();
            let cap = fb.proj(p, 0, Dest::Fresh(None), L()).unwrap();
            let acc = fb.proj(p, 1, Dest::Fresh(None), L()).unwrap();
            let elem = fb.proj(p, 2, Dest::Fresh(None), L()).unwrap();
            let ctx = Ctx::bodies(f, trap_free);
            let mut pool = Pool {
                i32s: vec![cap, acc, elem],
                ..Default::default()
            };
            emit_steps(&mut fb, &ctx, &mut pool, steps, false);
            let ret = pool.i32s.last().copied().unwrap_or(acc);
            fb.output(ret, None, L()).unwrap();
            fb.finish().unwrap();
        }
        out.push(f);
    }
    out
}

/// Declare + emit the two-level nest bodies (ADR-0027, matmul miniature): an
/// outer map body `(cap_arr, elem) -> i32` whose first action is a
/// `fold_captured` over its own array capture with `elem` as the fold's
/// capture and seed — the inner fold body `nestf{i}` is generated alongside
/// from its own script. The outer pool then holds `[elem, fold_result]` for
/// the generated scalar steps.
fn declare_nest_bodies(b: &mut IrBuilder, scripts: &[NestBody], trap_free: bool) -> Vec<FuncId> {
    let arr_ty = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: ARR as u64,
    };
    let mut out = Vec::new();
    for (i, script) in scripts.iter().enumerate() {
        let inner = declare_fold_cap_bodies(
            b,
            &format!("nestf{i}_"),
            std::slice::from_ref(&script.inner),
            trap_free,
        )[0];
        let f = b
            .declare(
                FuncKind::MapBody,
                &format!("nestm{i}"),
                Ty::Tuple(vec![arr_ty.clone(), Ty::i32()]),
                Ty::i32(),
                L(),
            )
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let p = fb.input();
            let cap_arr = fb.proj(p, 0, Dest::Fresh(None), L()).unwrap();
            let elem = fb.proj(p, 1, Dest::Fresh(None), L()).unwrap();
            // Fold the captured array, capturing the outer element (a capture
            // across two levels once the outer map itself captures `cap_arr`).
            let folded = fb
                .fold_captured(inner, &[elem], elem, cap_arr, Dest::Fresh(None), L())
                .unwrap();
            let ctx = Ctx::bodies(f, trap_free);
            let mut pool = Pool {
                i32s: vec![elem, folded],
                ..Default::default()
            };
            emit_steps(&mut fb, &ctx, &mut pool, &script.outer, false);
            let ret = pool.i32s.last().copied().unwrap_or(folded);
            fb.output(ret, None, L()).unwrap();
            fb.finish().unwrap();
        }
        out.push(f);
    }
    out
}

/// Open pure `main : i32 -> i32`, returned via `eval_call` on the random args.
fn build_open_main(b: &mut IrBuilder, ctx: &Ctx, prog: &Prog) -> Built {
    let f = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L())
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let x = fb.input();
        let mut pool = Pool {
            i32s: vec![x],
            ..Default::default()
        };
        // Seed a couple of constants so collections/arith always have operands.
        seed_consts(&mut fb, &mut pool);
        emit_steps(&mut fb, ctx, &mut pool, &prog.main, true);
        let ret = pick(&pool.i32s, prog.ret).unwrap_or(x);
        fb.output(ret, None, L()).unwrap();
        fb.finish().unwrap();
    }
    let ir = b_seal(b, f);
    let args = prog
        .args
        .iter()
        .map(|&a| RValue::Scalar(Value::I32(a)))
        .collect();
    Built {
        ir,
        open: true,
        entry: f,
        args,
    }
}

/// Closed `main`: effectful (`io -> io`, prints intermediates) or pure (`() -> i32`).
fn build_closed_main(b: &mut IrBuilder, ctx: &Ctx, prog: &Prog) -> Built {
    if prog.effectful {
        let f = b
            .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, L())
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let tok0 = fb.input();
            let mut pool = Pool::default();
            seed_consts(&mut fb, &mut pool);
            emit_steps(&mut fb, ctx, &mut pool, &prog.main, true);
            // Thread the token through prints of selected intermediates.
            let mut tok = tok0;
            let mut printed = false;
            let scalars: Vec<_> = pool
                .i32s
                .iter()
                .chain(&pool.i64s)
                .chain(&pool.f32s)
                .chain(&pool.f64s)
                .copied()
                .collect();
            for &sel in &prog.prints {
                if let Some(v) = pick(&scalars, sel) {
                    tok = fb.println(tok, v, L()).unwrap();
                    printed = true;
                }
            }
            if !printed {
                // A token must reach Return via a token consumer (I4b) — print a
                // constant so the trivial (no-op) main still seals.
                let c = fb.constant(Value::I32(0), L()).unwrap();
                tok = fb.println(tok, c, L()).unwrap();
            }
            fb.output(tok, None, L()).unwrap();
            fb.finish().unwrap();
        }
        let ir = b_seal(b, f);
        Built {
            ir,
            open: false,
            entry: f,
            args: vec![],
        }
    } else {
        let f = b
            .declare(FuncKind::Named, "main", Ty::Unit, Ty::i32(), L())
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let mut pool = Pool::default();
            seed_consts(&mut fb, &mut pool);
            emit_steps(&mut fb, ctx, &mut pool, &prog.main, true);
            let ret = pick(&pool.i32s, prog.ret).unwrap_or(pool.i32s[0]);
            fb.output(ret, None, L()).unwrap();
            fb.finish().unwrap();
        }
        let ir = b_seal(b, f);
        Built {
            ir,
            open: false,
            entry: f,
            args: vec![],
        }
    }
}

/// Seal a builder whose `main` is `entry`, consuming a fresh clone-free builder.
fn b_seal(b: &mut IrBuilder, entry: FuncId) -> CategoryIr {
    // `seal` consumes the builder; swap a fresh one in to move it out.
    let taken = std::mem::replace(b, IrBuilder::new());
    taken.seal(entry).expect("generator: seal must succeed")
}

/// Push a few seed constants so early collection/arith steps have operands.
fn seed_consts(fb: &mut mapal_ir::FnBuilder<'_>, pool: &mut Pool) {
    for k in [1i32, 2, 3] {
        pool.i32s.push(fb.constant(Value::I32(k), L()).unwrap());
    }
    pool.bools
        .push(fb.constant(Value::Bool(true), L()).unwrap());
}

// --- step emission --------------------------------------------------------

fn emit_steps(
    fb: &mut mapal_ir::FnBuilder<'_>,
    ctx: &Ctx,
    pool: &mut Pool,
    steps: &[Step],
    collections: bool,
) {
    for step in steps {
        emit_step(fb, ctx, pool, step, collections);
    }
}

fn emit_step(
    fb: &mut mapal_ir::FnBuilder<'_>,
    ctx: &Ctx,
    pool: &mut Pool,
    step: &Step,
    collections: bool,
) {
    use Operation::*;
    match *step {
        Step::ConstI32(k) => pool.i32s.push(fb.constant(Value::I32(k), L()).unwrap()),
        Step::ConstBool(v) => pool.bools.push(fb.constant(Value::Bool(v), L()).unwrap()),
        Step::Bin { op, a, b } => {
            let (Some(x), Some(mut y)) = (pick(&pool.i32s, a), pick(&pool.i32s, b)) else {
                return;
            };
            let op = [Add, Sub, Mul, Div, Mod][op as usize % 5];
            if ctx.trap_free && matches!(op, Div | Mod) {
                // Force a non-zero constant divisor (no runtime trap).
                y = fb.constant(Value::I32((b as i32 % 7) + 1), L()).unwrap();
            }
            let r = fb.binop(op, x, y, Dest::Fresh(None), L()).unwrap();
            pool.i32s.push(r);
        }
        Step::Neg { a } => {
            if let Some(x) = pick(&pool.i32s, a) {
                pool.i32s
                    .push(fb.unop(Neg, x, Dest::Fresh(None), L()).unwrap());
            }
        }
        Step::Widen { edge, a } => match edge % 4 {
            0 => {
                if let Some(x) = pick(&pool.i32s, a) {
                    pool.i64s
                        .push(fb.widen(x, Ty::i64(), Dest::Fresh(None), L()).unwrap());
                }
            }
            1 => {
                if let Some(x) = pick(&pool.i32s, a) {
                    pool.f32s
                        .push(fb.widen(x, Ty::f32(), Dest::Fresh(None), L()).unwrap());
                }
            }
            2 => {
                if let Some(x) = pick(&pool.i32s, a) {
                    pool.f64s
                        .push(fb.widen(x, Ty::f64(), Dest::Fresh(None), L()).unwrap());
                }
            }
            _ => {
                let x = match pick(&pool.f32s, a) {
                    Some(x) => x,
                    None => {
                        let Some(i) = pick(&pool.i32s, a) else {
                            return;
                        };
                        let x = fb.widen(i, Ty::f32(), Dest::Fresh(None), L()).unwrap();
                        pool.f32s.push(x);
                        x
                    }
                };
                pool.f64s
                    .push(fb.widen(x, Ty::f64(), Dest::Fresh(None), L()).unwrap());
            }
        },
        Step::Iota => {
            let c = fb.constant(Value::I32(ARR as i32), L()).unwrap();
            pool.arrs.push(fb.iota(c, Dest::Fresh(None), L()).unwrap());
        }
        Step::Fill { a } => {
            let Some(x) = pick(&pool.i32s, a) else {
                return;
            };
            let c = fb.constant(Value::I32(ARR as i32), L()).unwrap();
            pool.arrs
                .push(fb.fill(x, c, Dest::Fresh(None), L()).unwrap());
        }
        Step::Cmp { op, a, b } => {
            let (Some(x), Some(y)) = (pick(&pool.i32s, a), pick(&pool.i32s, b)) else {
                return;
            };
            let op = [Eq, Neq, Lt, Gt, Le, Ge][op as usize % 6];
            pool.bools
                .push(fb.binop(op, x, y, Dest::Fresh(None), L()).unwrap());
        }
        Step::Not { a } => {
            if let Some(x) = pick(&pool.bools, a) {
                pool.bools
                    .push(fb.unop(Not, x, Dest::Fresh(None), L()).unwrap());
            }
        }
        Step::Logic { or, a, b } => {
            let (Some(x), Some(y)) = (pick(&pool.bools, a), pick(&pool.bools, b)) else {
                return;
            };
            let op = if or { Or } else { And };
            pool.bools
                .push(fb.binop(op, x, y, Dest::Fresh(None), L()).unwrap());
        }
        Step::Phi { t, e, c } => {
            let (Some(x), Some(y), Some(cond)) = (
                pick(&pool.i32s, t),
                pick(&pool.i32s, e),
                pick(&pool.bools, c),
            ) else {
                return;
            };
            pool.i32s
                .push(fb.phi(x, y, cond, Dest::Fresh(None), L()).unwrap());
        }
        Step::PhiTrapArm {
            a,
            b,
            c,
            on_true,
            modulo,
        } => {
            let (Some(x), Some(mut y), Some(cond)) = (
                pick(&pool.i32s, a),
                pick(&pool.i32s, b),
                pick(&pool.bools, c),
            ) else {
                return;
            };
            let op = if modulo { Mod } else { Div };
            if ctx.trap_free {
                // Same rule as `Step::Bin`: a const non-zero divisor.
                y = fb.constant(Value::I32((b as i32 % 7) + 1), L()).unwrap();
            }
            let risky = fb.binop(op, x, y, Dest::Fresh(None), L()).unwrap();
            // NOT pushed to the pool: the whole point is that this object's
            // only consumer is the Phi arm, so the arm exclusively owns it.
            let other = pick(&pool.i32s, a.wrapping_add(1)).unwrap_or(x);
            let (t, e) = if on_true {
                (risky, other)
            } else {
                (other, risky)
            };
            pool.i32s
                .push(fb.phi(t, e, cond, Dest::Fresh(None), L()).unwrap());
        }
        Step::PackProj { a, b, snd } => {
            let (Some(x), Some(y)) = (pick(&pool.i32s, a), pick(&pool.i32s, b)) else {
                return;
            };
            let p = fb.pack(&[x, y], Dest::Fresh(None), L()).unwrap();
            let idx = if snd { 1 } else { 0 };
            pool.i32s
                .push(fb.proj(p, idx, Dest::Fresh(None), L()).unwrap());
        }
        Step::MakeArray { a, b, c } if collections => {
            let (Some(x), Some(y), Some(z)) = (
                pick(&pool.i32s, a),
                pick(&pool.i32s, b),
                pick(&pool.i32s, c),
            ) else {
                return;
            };
            pool.arrs
                .push(fb.pack_array(&[x, y, z], Dest::Fresh(None), L()).unwrap());
        }
        Step::Index { arr, idx } if collections => {
            let Some(a) = pick(&pool.arrs, arr) else {
                return;
            };
            let i = if ctx.trap_free {
                fb.constant(Value::I32(idx as i32 % ARR as i32), L())
                    .unwrap()
            } else {
                match pick(&pool.i32s, idx) {
                    Some(x) => x,
                    None => return,
                }
            };
            pool.i32s
                .push(fb.index(a, i, Dest::Fresh(None), L()).unwrap());
        }
        Step::Update { arr, idx, val } if collections => {
            let (Some(a), Some(v)) = (pick(&pool.arrs, arr), pick(&pool.i32s, val)) else {
                return;
            };
            // trap_free: index literal-and-in-bounds by construction; default:
            // an arbitrary pool feeder (sometimes OOB — the exit-101 trap path).
            let i = if ctx.trap_free {
                fb.constant(Value::I32(idx as i32 % ARR as i32), L())
                    .unwrap()
            } else {
                match pick(&pool.i32s, idx) {
                    Some(x) => x,
                    None => return,
                }
            };
            pool.arrs
                .push(fb.update(a, i, v, Dest::Fresh(None), L()).unwrap());
        }
        Step::MapArr { arr, body } if collections => {
            let (Some(a), Some(bd)) = (pick(&pool.arrs, arr), pick(&ctx.map_bodies, body)) else {
                return;
            };
            pool.arrs
                .push(fb.map(bd, a, Dest::Fresh(None), L()).unwrap());
        }
        Step::FoldArr { arr, seed, body } if collections => {
            let (Some(a), Some(s), Some(bd)) = (
                pick(&pool.arrs, arr),
                pick(&pool.i32s, seed),
                pick(&ctx.fold_bodies, body),
            ) else {
                return;
            };
            let pair = fb.pack(&[s, a], Dest::Fresh(None), L()).unwrap();
            pool.i32s
                .push(fb.fold(bd, pair, Dest::Fresh(None), L()).unwrap());
        }
        Step::Zip { a, b } if collections => {
            let (Some(x), Some(y)) = (pick(&pool.arrs, a), pick(&pool.arrs, b)) else {
                return;
            };
            let pairs = fb.zip(x, y, Dest::Fresh(None), L()).unwrap();
            // Reduce the pair-array back to [i32;3] via the utility body.
            pool.arrs
                .push(fb.map(ctx.pair_sum, pairs, Dest::Fresh(None), L()).unwrap());
        }
        Step::Enumerate { arr } if collections => {
            let Some(a) = pick(&pool.arrs, arr) else {
                return;
            };
            let pairs = fb.enumerate(a, Dest::Fresh(None), L()).unwrap();
            pool.arrs
                .push(fb.map(ctx.pair_sum, pairs, Dest::Fresh(None), L()).unwrap());
        }
        Step::Call { a, helper } if collections => {
            let (Some(x), Some(g)) = (pick(&pool.i32s, a), pick(&ctx.helpers, helper)) else {
                return;
            };
            pool.i32s
                .push(fb.call(g, x, Dest::Fresh(None), L()).unwrap());
        }
        Step::Loop { k } if collections && pool.loops_used < MAX_LOOPS => {
            pool.loops_used += 1;
            pool.i32s.push(build_loop(fb, (k % 65) as i32));
        }
        Step::LiftFold { k, seed, cap } if collections && pool.loops_used < MAX_LOOPS => {
            let Some(seed) = pick(&pool.i32s, seed) else {
                return;
            };
            let cap = fb.constant(Value::I32(i32::from(cap)), L()).unwrap();
            pool.loops_used += 1;
            pool.i32s
                .push(build_lift_fold(fb, (k % ARR as u8 + 1) as i32, seed, cap));
        }
        Step::LiftMap { arr, cap } if collections && pool.loops_used < MAX_LOOPS => {
            let cap = fb.constant(Value::I32(i32::from(cap)), L()).unwrap();
            let init = pick(&pool.arrs, arr)
                .unwrap_or_else(|| fb.pack_array(&[cap; ARR], Dest::Fresh(None), L()).unwrap());
            pool.loops_used += 1;
            pool.arrs.push(build_lift_map(fb, init, cap));
        }
        Step::MapCapScalar { arr, cap, body } if collections => {
            let (Some(a), Some(c), Some(bd)) = (
                pick(&pool.arrs, arr),
                pick(&pool.i32s, cap),
                pick(&ctx.map_cap_bodies, body),
            ) else {
                return;
            };
            pool.arrs.push(
                fb.map_captured(bd, &[c], a, Dest::Fresh(None), L())
                    .unwrap(),
            );
        }
        Step::MapCapArray { arr, cap, body } if collections => {
            let (Some(a), Some(c), Some(bd)) = (
                pick(&pool.arrs, arr),
                pick(&pool.arrs, cap),
                pick(&ctx.map_acap_bodies, body),
            ) else {
                return;
            };
            pool.arrs.push(
                fb.map_captured(bd, &[c], a, Dest::Fresh(None), L())
                    .unwrap(),
            );
        }
        Step::FoldCapScalar {
            arr,
            seed,
            cap,
            body,
        } if collections => {
            let (Some(a), Some(s), Some(c), Some(bd)) = (
                pick(&pool.arrs, arr),
                pick(&pool.i32s, seed),
                pick(&pool.i32s, cap),
                pick(&ctx.fold_cap_bodies, body),
            ) else {
                return;
            };
            pool.i32s.push(
                fb.fold_captured(bd, &[c], s, a, Dest::Fresh(None), L())
                    .unwrap(),
            );
        }
        Step::MapNestFold { arr, cap, body } if collections => {
            let (Some(a), Some(c), Some(bd)) = (
                pick(&pool.arrs, arr),
                pick(&pool.arrs, cap),
                pick(&ctx.nest_bodies, body),
            ) else {
                return;
            };
            pool.arrs.push(
                fb.map_captured(bd, &[c], a, Dest::Fresh(None), L())
                    .unwrap(),
            );
        }
        Step::LoopCapMap { k, arr, body } if collections && pool.loops_used < MAX_LOOPS => {
            pool.loops_used += 1;
            let exit = build_loop(fb, (k % 65) as i32);
            pool.i32s.push(exit);
            // The loop's exit value is the map's capture — read-at-position.
            let (Some(a), Some(bd)) = (pick(&pool.arrs, arr), pick(&ctx.map_cap_bodies, body))
            else {
                return;
            };
            pool.arrs.push(
                fb.map_captured(bd, &[exit], a, Dest::Fresh(None), L())
                    .unwrap(),
            );
        }
        // Collection/loop steps in a scalar body: skipped.
        _ => {}
    }
}

/// A canonical bounded loop `i = 0; while i < k { i += 1 }; i` → `k` (`k ≤ 64`).
/// One merge / one back / one exit — the interp-M1 canonical quartet.
fn build_loop(fb: &mut mapal_ir::FnBuilder<'_>, k: i32) -> mapal_ir::ObjectId {
    let zero = fb.constant(Value::I32(0), L()).unwrap();
    let kc = fb.constant(Value::I32(k), L()).unwrap();
    let lh = fb.begin_loop(zero, L()).unwrap();
    let i = fb.merge_of(&lh);
    let cond = fb
        .binop(Operation::Lt, i, kc, Dest::Fresh(None), L())
        .unwrap();
    let one = fb.constant(Value::I32(1), L()).unwrap();
    let next = fb
        .binop(Operation::Add, i, one, Dest::Fresh(None), L())
        .unwrap();
    fb.loop_back(&lh, next, cond, L()).unwrap();
    let exit = fb.loop_exit(&lh, i, cond, Dest::Fresh(None), L()).unwrap();
    fb.end_loop(lh).unwrap();
    exit
}

fn build_lift_fold(
    fb: &mut mapal_ir::FnBuilder<'_>,
    k: i32,
    seed: mapal_ir::ObjectId,
    capture: mapal_ir::ObjectId,
) -> mapal_ir::ObjectId {
    let zero = fb.constant(Value::I32(0), L()).unwrap();
    // Make the invariant an explicit predecessor of LoopEnter. Core starts a
    // loop when its init is ready, so an otherwise body-only invariant could
    // still be pending when the first advance phase runs.
    let capture_zero = fb
        .binop(Operation::Mul, capture, zero, Dest::Fresh(None), L())
        .unwrap();
    let ready_seed = fb
        .binop(Operation::Add, seed, capture_zero, Dest::Fresh(None), L())
        .unwrap();
    let init = fb
        .pack(&[zero, ready_seed], Dest::Fresh(None), L())
        .unwrap();
    let lh = fb.begin_loop(init, L()).unwrap();
    let state = fb.merge_of(&lh);
    let counter = fb.proj(state, 0, Dest::Fresh(None), L()).unwrap();
    let acc = fb.proj(state, 1, Dest::Fresh(None), L()).unwrap();
    let bound = fb.constant(Value::I32(k), L()).unwrap();
    let cond = fb
        .binop(Operation::Lt, counter, bound, Dest::Fresh(None), L())
        .unwrap();
    let partial = fb
        .binop(Operation::Add, acc, counter, Dest::Fresh(None), L())
        .unwrap();
    let next_acc = fb
        .binop(Operation::Add, partial, capture, Dest::Fresh(None), L())
        .unwrap();
    let one = fb.constant(Value::I32(1), L()).unwrap();
    let next_counter = fb
        .binop(Operation::Add, counter, one, Dest::Fresh(None), L())
        .unwrap();
    let next = fb
        .pack(&[next_counter, next_acc], Dest::Fresh(None), L())
        .unwrap();
    fb.loop_back(&lh, next, cond, L()).unwrap();
    let exit = fb
        .loop_exit(&lh, acc, cond, Dest::Fresh(None), L())
        .unwrap();
    fb.end_loop(lh).unwrap();
    exit
}

fn build_lift_map(
    fb: &mut mapal_ir::FnBuilder<'_>,
    init_array: mapal_ir::ObjectId,
    capture: mapal_ir::ObjectId,
) -> mapal_ir::ObjectId {
    let zero = fb.constant(Value::I32(0), L()).unwrap();
    // As above, sequence the invariant before LoopEnter through the collection
    // init. Coverage overwrites every cell, so this value is observationally
    // dead and is dropped by R-LM.
    let ready_array = fb
        .update(init_array, zero, capture, Dest::Fresh(None), L())
        .unwrap();
    let init = fb
        .pack(&[ready_array, zero], Dest::Fresh(None), L())
        .unwrap();
    let lh = fb.begin_loop(init, L()).unwrap();
    let state = fb.merge_of(&lh);
    let out = fb.proj(state, 0, Dest::Fresh(None), L()).unwrap();
    let counter = fb.proj(state, 1, Dest::Fresh(None), L()).unwrap();
    let bound = fb.constant(Value::I32(ARR as i32), L()).unwrap();
    let cond = fb
        .binop(Operation::Lt, counter, bound, Dest::Fresh(None), L())
        .unwrap();
    let value = fb
        .binop(Operation::Add, counter, capture, Dest::Fresh(None), L())
        .unwrap();
    let updated = fb
        .update(out, counter, value, Dest::Fresh(None), L())
        .unwrap();
    let one = fb.constant(Value::I32(1), L()).unwrap();
    let next_counter = fb
        .binop(Operation::Add, counter, one, Dest::Fresh(None), L())
        .unwrap();
    let next = fb
        .pack(&[updated, next_counter], Dest::Fresh(None), L())
        .unwrap();
    fb.loop_back(&lh, next, cond, L()).unwrap();
    let exit = fb
        .loop_exit(&lh, out, cond, Dest::Fresh(None), L())
        .unwrap();
    fb.end_loop(lh).unwrap();
    exit
}

fn pair_ty() -> Ty {
    Ty::Tuple(vec![Ty::i32(), Ty::i32()])
}
