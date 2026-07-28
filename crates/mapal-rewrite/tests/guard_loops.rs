//! plan-s40 adversarial-review regressions: the five confirmed findings of the
//! S40 review workflow, each pinned as the exact builder graph the reviewer
//! traced (session log 2026-07-28 §review). All are IR-only shapes — L1406
//! keeps loop machinery out of surface Phi arms — reachable by testgen and by
//! hand-built IR.
//!
//! The last test (`dce_must_not_change_arm_ownership`) pins review find [5],
//! a PRE-EXISTING S39-class instability: DCE deleting a dead sibling consumer
//! flipped a site from strict to gated, suppressing a trap that must fire.
//! Fixed in the same session: DCE pins every dead `Temporary` sink
//! forward-reachable from the verdict cone, so dead cones that can touch a
//! guard verdict survive the rewrite (graph_rewrites.rs).

use mapal_interp::{Outcome, RValue, eval_call};
use mapal_ir::{Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value, validate};
use mapal_rewrite::rewrite;

const BUDGET: u64 = 2_000_000;
const L: SourceLoc = SourceLoc { start: 0, end: 0 };

fn arg(v: i32) -> RValue {
    RValue::Scalar(Value::I32(v))
}

/// Review find [0]: nested-site subtraction cascades the loop's payload
/// consumers out of the arm; the re-close must then drop the unit's handle
/// too (re-tested with the join predicate), or the un-gated survivors read an
/// object the gated loop never wrote — an interp PANIC on a valid graph.
/// Post-fix: the handle is dropped, the loop runs unconditionally, and the
/// body's `7 / i` (i from 0) traps — the strict meaning.
#[test]
fn reclose_drops_the_handle_when_the_units_outputs_leak() {
    for a in [-1i32, 1] {
        let mut b = IrBuilder::new();
        let f = b
            .declare(FuncKind::Named, "f", Ty::i32(), Ty::i32(), L)
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let p = fb.input();
            let zero = fb.constant(Value::I32(0), L).unwrap();
            let ten = fb.constant(Value::I32(10), L).unwrap();
            let c1 = fb
                .binop(Operation::Gt, p, zero, Dest::Fresh(None), L)
                .unwrap();
            let c2 = fb
                .binop(Operation::Lt, p, ten, Dest::Fresh(None), L)
                .unwrap();
            let lh = fb.begin_loop(zero, L).unwrap();
            let merge = fb.merge_of(&lh);
            let lc = fb
                .binop(Operation::Lt, merge, ten, Dest::Fresh(None), L)
                .unwrap();
            let seven = fb.constant(Value::I32(7), L).unwrap();
            let next = fb
                .binop(Operation::Div, seven, merge, Dest::Fresh(None), L)
                .unwrap();
            fb.loop_back(&lh, next, lc, L).unwrap();
            let ex = fb.loop_exit(&lh, merge, lc, Dest::Fresh(None), L).unwrap();
            fb.end_loop(lh).unwrap();
            let two = fb.constant(Value::I32(2), L).unwrap();
            let d = fb
                .binop(Operation::Mul, ex, two, Dest::Fresh(None), L)
                .unwrap();
            let five = fb.constant(Value::I32(5), L).unwrap();
            let inner = fb.phi(d, five, c2, Dest::Fresh(None), L).unwrap();
            let sum = fb
                .binop(Operation::Add, d, inner, Dest::Fresh(None), L)
                .unwrap();
            let one = fb.constant(Value::I32(1), L).unwrap();
            fb.phi(sum, one, c1, Dest::Ret { slot: None }, L).unwrap();
            fb.finish().unwrap();
        }
        let ir = b.seal(f).unwrap();
        assert!(validate(&ir).is_empty());
        // Pre-fix this PANICKED (`read before write`) for both inputs. The
        // strict meaning is a trap: the loop is not arm-exclusive after
        // subtraction, so it runs, and `7 / 0` fires on the first iteration.
        let rr = std::panic::catch_unwind(|| eval_call(&ir, ir.entry(), arg(a), BUDGET))
            .expect("interp must never panic on a valid graph");
        assert!(matches!(rr, Outcome::Trapped(_)), "a={a}: {:?}", rr);
    }
}

/// Review find [1]: a unit member writing the function's Return object has no
/// consumers, and `all()` over the empty list is vacuously true — the unit
/// joined and gated a loop whose body writes the observable output. Post-fix
/// the sink member refuses the join; the loop runs unconditionally.
#[test]
fn unit_with_sink_member_never_joins() {
    for a in [-1i32, 1] {
        let mut b = IrBuilder::new();
        let f = b
            .declare(FuncKind::Named, "f", Ty::i32(), Ty::i32(), L)
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let p = fb.input();
            let zero = fb.constant(Value::I32(0), L).unwrap();
            let ten = fb.constant(Value::I32(10), L).unwrap();
            let lh = fb.begin_loop(zero, L).unwrap();
            let merge = fb.merge_of(&lh);
            let lc = fb
                .binop(Operation::Lt, merge, ten, Dest::Fresh(None), L)
                .unwrap();
            let one = fb.constant(Value::I32(1), L).unwrap();
            let next = fb
                .binop(Operation::Add, merge, one, Dest::Fresh(None), L)
                .unwrap();
            // The observable write INSIDE the loop: Neg(merge) -> Return.
            fb.unop(Operation::Neg, merge, Dest::Ret { slot: None }, L)
                .unwrap();
            fb.loop_back(&lh, next, lc, L).unwrap();
            let ex = fb.loop_exit(&lh, merge, lc, Dest::Fresh(None), L).unwrap();
            fb.end_loop(lh).unwrap();
            let cond = fb
                .binop(Operation::Gt, p, zero, Dest::Fresh(None), L)
                .unwrap();
            let five = fb.constant(Value::I32(5), L).unwrap();
            // Dead Phi consuming the exit — the arm that tried to own the unit.
            fb.phi(ex, five, cond, Dest::Fresh(None), L).unwrap();
            fb.finish().unwrap();
        }
        let ir = b.seal(f).unwrap();
        assert!(validate(&ir).is_empty());
        let rr = std::panic::catch_unwind(|| eval_call(&ir, ir.entry(), arg(a), BUDGET))
            .expect("interp must never panic on a valid graph");
        // The Return must be written for BOTH inputs — pre-fix a=-1 panicked
        // with "non-Unit return is always written".
        assert!(matches!(rr, Outcome::Done(_)), "a={a}: {:?}", rr);
    }
}

/// Review find [2]: an in-body guard's loop-INVARIANT exclusive work (fed by
/// constants) is not a unit member, so the enclosing arm owned it too — the
/// same `Div` in two own-lists, fired twice. Post-fix the subtraction treats
/// a nested Phi inside an owned unit as gated through the handle and strips
/// its work from the enclosing arm.
#[test]
fn in_body_arm_work_is_not_double_owned() {
    for (a, want_done) in [(1i32, true), (-1, true)] {
        let mut b = IrBuilder::new();
        let f = b
            .declare(FuncKind::Named, "f", Ty::i32(), Ty::i32(), L)
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let p = fb.input();
            let zero = fb.constant(Value::I32(0), L).unwrap();
            let ten = fb.constant(Value::I32(10), L).unwrap();
            let lh = fb.begin_loop(zero, L).unwrap();
            let merge = fb.merge_of(&lh);
            let lc = fb
                .binop(Operation::Lt, merge, ten, Dest::Fresh(None), L)
                .unwrap();
            let hundred = fb.constant(Value::I32(100), L).unwrap();
            let bc = fb
                .binop(Operation::Lt, merge, hundred, Dest::Fresh(None), L)
                .unwrap();
            let one = fb.constant(Value::I32(1), L).unwrap();
            let inc = fb
                .binop(Operation::Add, merge, one, Dest::Fresh(None), L)
                .unwrap();
            let seven = fb.constant(Value::I32(7), L).unwrap();
            let zc = fb.constant(Value::I32(0), L).unwrap();
            let bad = fb
                .binop(Operation::Div, seven, zc, Dest::Fresh(None), L)
                .unwrap();
            let next = fb.phi(inc, bad, bc, Dest::Fresh(None), L).unwrap();
            fb.loop_back(&lh, next, lc, L).unwrap();
            let ex = fb.loop_exit(&lh, merge, lc, Dest::Fresh(None), L).unwrap();
            fb.end_loop(lh).unwrap();
            let cond = fb
                .binop(Operation::Gt, p, zero, Dest::Fresh(None), L)
                .unwrap();
            let fortytwo = fb.constant(Value::I32(42), L).unwrap();
            fb.phi(ex, fortytwo, cond, Dest::Ret { slot: None }, L)
                .unwrap();
            fb.finish().unwrap();
        }
        let ir = b.seal(f).unwrap();
        assert!(validate(&ir).is_empty());
        let rr = std::panic::catch_unwind(|| eval_call(&ir, ir.entry(), arg(a), BUDGET))
            .expect("interp must never panic on a valid graph");
        // bc (i < 100) is true on every iteration, so `bad` never fires:
        // a=1 -> the loop's exit (10), a=-1 -> 42. Never a trap.
        assert_eq!(want_done, matches!(rr, Outcome::Done(_)), "a={a}: {:?}", rr);
    }
}

/// Review finds [3]/[6]: DCE's dead-Phi pin keyed on `can_trap` alone, so a
/// dead Phi gating a trap-free NON-TERMINATING loop was deleted, the loop
/// survived via RW11's SCC pin, and the rewritten graph drove it
/// unconditionally: Done raw, Diverged rewritten.
#[test]
fn dce_keeps_the_dead_phi_of_a_heavy_gated_site() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "f", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let p = fb.input();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let big = fb.constant(Value::I32(1_000_000_000), L).unwrap();
        let lh = fb.begin_loop(zero, L).unwrap();
        let merge = fb.merge_of(&lh);
        let lc = fb
            .binop(Operation::Lt, merge, big, Dest::Fresh(None), L)
            .unwrap();
        // Non-terminating within budget: i + 0.
        let next = fb
            .binop(Operation::Add, merge, zero, Dest::Fresh(None), L)
            .unwrap();
        fb.loop_back(&lh, next, lc, L).unwrap();
        let ex = fb.loop_exit(&lh, merge, lc, Dest::Fresh(None), L).unwrap();
        fb.end_loop(lh).unwrap();
        let tru = fb.constant(Value::Bool(true), L).unwrap();
        let fortytwo = fb.constant(Value::I32(42), L).unwrap();
        // Dead Phi: the true arm is taken, the loop arm never fires.
        fb.phi(fortytwo, ex, tru, Dest::Fresh(None), L).unwrap();
        // Function output independent of both.
        fb.binop(Operation::Add, p, zero, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty());
    let before = eval_call(&ir, ir.entry(), arg(0), BUDGET);
    assert!(matches!(before, Outcome::Done(_)), "{:?}", before);
    let after = {
        let res = rewrite(ir);
        eval_call(&res.ir, res.ir.entry(), arg(0), BUDGET)
    };
    assert!(
        matches!(after, Outcome::Done(_)),
        "rewritten run diverged — the gated loop lost its gate: {:?}",
        after
    );
}

/// Review find [4]: ConstFold dropped the losing arm + the triple whenever
/// the condition was constant, but the compensating phi-select alias refuses
/// an SCC winner — replay then rebuilt the losing boundary edge against a
/// dropped feeder and PANICKED ("feeder is not mapped"). Post-fix the drop
/// only happens when the alias provably fires.
#[test]
fn constfold_does_not_drop_what_it_cannot_alias() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "f", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let ten = fb.constant(Value::I32(10), L).unwrap();
        let lh = fb.begin_loop(zero, L).unwrap();
        let merge = fb.merge_of(&lh);
        let lc = fb
            .binop(Operation::Lt, merge, ten, Dest::Fresh(None), L)
            .unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let inc = fb
            .binop(Operation::Add, merge, one, Dest::Fresh(None), L)
            .unwrap();
        fb.loop_back(&lh, inc, lc, L).unwrap();
        // Inside the loop: phi(inc /* SCC winner */, 7/0, const true) feeds
        // the exit value.
        let tru = fb.constant(Value::Bool(true), L).unwrap();
        let seven = fb.constant(Value::I32(7), L).unwrap();
        let zc = fb.constant(Value::I32(0), L).unwrap();
        let bad = fb
            .binop(Operation::Div, seven, zc, Dest::Fresh(None), L)
            .unwrap();
        let sel = fb.phi(inc, bad, tru, Dest::Fresh(None), L).unwrap();
        fb.loop_exit(&lh, sel, lc, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty());
    let before = eval_call(&ir, ir.entry(), arg(0), BUDGET);
    let res = std::panic::catch_unwind(|| rewrite(ir))
        .expect("replay must not panic (feeder is not mapped)");
    let after = eval_call(&res.ir, res.ir.entry(), arg(0), BUDGET);
    assert_eq!(
        std::mem::discriminant(&before),
        std::mem::discriminant(&after),
        "before {:?} after {:?}",
        before,
        after
    );
}

/// Review find [5] — was PRE-EXISTING (S39-class): DCE deleted a dead pure
/// sibling consumer of a trap-capable value; the value's only remaining
/// consumer was a Phi arm, so a site that was strict raw became gated
/// rewritten, and a trap the raw program fires was suppressed:
/// `eval ∘ Dce ≠ eval` with no loop anywhere. Fixed by DCE's verdict-cone
/// dead-sink pin. The class question for OTHER consumer-set-changing passes
/// is S41's (the 1024-case hammer with `PhiTrapArm` exercises them all and
/// only DCE fell).
#[test]
fn dce_must_not_change_arm_ownership() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "g", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let a = fb.input();
        let seven = fb.constant(Value::I32(7), L).unwrap();
        let d = fb
            .binop(Operation::Div, seven, a, Dest::Fresh(None), L)
            .unwrap();
        // Dead pure sibling consumer of d.
        let one = fb.constant(Value::I32(1), L).unwrap();
        fb.binop(Operation::Add, d, one, Dest::Fresh(None), L)
            .unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let cond = fb
            .binop(Operation::Gt, a, zero, Dest::Fresh(None), L)
            .unwrap();
        let fortytwo = fb.constant(Value::I32(42), L).unwrap();
        fb.phi(d, fortytwo, cond, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty());
    // a = 0: raw is strict on d (a second consumer exists, dead or not), so
    // the Div fires and traps. The rewritten graph must agree.
    let before = eval_call(&ir, ir.entry(), arg(0), BUDGET);
    let after = {
        let res = rewrite(ir);
        eval_call(&res.ir, res.ir.entry(), arg(0), BUDGET)
    };
    assert_eq!(
        std::mem::discriminant(&before),
        std::mem::discriminant(&after),
        "DCE changed arm ownership: before {:?} after {:?}",
        before,
        after
    );
}
