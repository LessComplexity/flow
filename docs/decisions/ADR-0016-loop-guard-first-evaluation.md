# ADR-0016: Loop branch evaluation is guard-first — the continue-branch is not speculatively evaluated on the exit step (E1 refinement)

Date: 2026-06-15 · Status: **accepted** — autonomous (Session 08), decided from Sapir's "pick the best place per the language's design" delegation; **flagged for Sapir's explicit review in next-session.md**, revisable by superseding ADR

## Context (what forced the decision; spec refs)

Designing the interpreter oracle (`interp/DESIGN.md` §4, the SCC loop driver)
against the real lowered IR exposed a semantic gap in how a loop's body is
evaluated on the **exit iteration**.

Flow-Core loops are Elgot/least-fixpoint iteration of a step `f : U → B ⊕ U`
(ADR-0002 / E1): apply `f` to the carried state `U`; `inr(u')` continues with the
new state, `inl(b)` exits with value `b`. `lower` realizes this as a pure
dataflow cycle (`category-ir.md` §4.5): a `LoopMerge` carrying `U`, a guard
`cond : Bool` computed from the merge, a **continue-branch** sub-DAG feeding the
`LoopBack` route `(next_state, cond)`, and an **exit-branch** feeding the
`LoopExit` route `(value, cond)`. The two routes share the one guard bool and
fire mutually exclusively (`LoopBack` on `true`, `LoopExit` on `false` — ir D7).

The interp DESIGN §4 driver, as first written, evaluated the **entire** body each
iteration and *then* tested the guard ("eager-both", like a pure Phi). The state
walks one past the guard (`sum_to_n`: `(1,0)…(11,55)`, exit reads `acc=55`), and
the §4 prose reasoned that the discarded next-state on that last step is
harmless. It is **not** harmless in general: the continue-branch can **trap**.

Concretely, `fir4` (`examples/fir.flow`) carries `U = (k, acc)` with guard
`k < 4`; the next-`acc` computation contains `coeffs[k] * signal[4+k]` —
`Index` ops reading the merge's `k` (lowered objects `f0o13/f0o14/f0o18/f0o19`).
States walk `k = 0→1→2→3→4`. On the exit step `k = 4`, eager-both evaluates the
continue-branch **before** testing `4 < 4 = false`, indexing `coeffs[4]` on a
`[f32; 4]` ⇒ `Trapped(IndexOob)`. That contradicts the pinned acceptance golden
`fir → "5.375\n"` (ir/lower goldens, the example header, interp DESIGN §11).

This is a meaning question (does the exit step evaluate the not-taken arm?), so
the authority order — ADRs/E1 (#1) and `category-ir.md` §4.5 (#2) — decides it,
not a per-component implementation choice. Three further facts pin the answer:

- **Speculation is sound only for pure branches.** HANDOFF §4.1 restricts the
  compute-both-select (Phi) lowering to **pure branches only**, exactly because
  pure branches cannot trap or emit. A loop's continue-branch is not pure: it can
  `Trapped(IndexOob)`/`DivZero` (ADR-0013) and threads effects (countdown's
  in-loop `println`). Evaluating it speculatively is the loop-shaped twin of
  putting an effect in a parallel fanout (E2) — a category error.
- **Functor correctness wants one definition.** The thesis (HANDOFF §1; VISION
  O4) is "same source = provably the same function across targets". The rule must
  live where every backend functor inherits it (the oracle + this ADR, enforced
  by differential tests), not be rediscovered per backend.
- **Guard-first is the natural compilation for every target.** A CPU
  `while (cond) { body }` and a Verilog done-protocol FSM (next-state computed
  combinationally but only **latched** while `busy`; `done` asserted with the
  exit value — ADR-0002) both naturally avoid *committing* the exit-step
  continue-branch. The fix aligns targets rather than taxing each.

## Decision (one paragraph, imperative)

Loop branch evaluation is **guard-first**, and this is the meaning of the §4.5
trace under E1: on each iteration, evaluate only the **decision + exit cone** —
the guard `cond` and the `LoopExit` route's payload (including any in-loop effect
that precedes the guard and feeds the exit value, e.g. countdown's `println`) —
then read the guard; if it selects *exit*, take the exit value and stop **without
evaluating the continue-branch** (the `inr(U)` arm); only if it selects *continue*
evaluate the remaining continue-branch sub-DAG (the `LoopBack` next-state, which
is where speculative traps such as `fir`'s `Index` live) and iterate. The
continue-branch is the not-taken arm of the coproduct on the exit step and MUST
NOT be evaluated there. Every consumer of the IR — the interpreter oracle now,
and every backend functor later — honors this; differential testing against the
oracle enforces it. The interp DESIGN §4 driver is corrected to this shape;
`category-ir.md` is **not** edited (the frozen Level-A spec already says E1/Elgot
— this ADR records the operational refinement that the eager-both reading was
unsound, and is the authority for it).

## Consequences (tradeoffs, implementation impact)

- The §4 driver is no longer one straight body pass: per iteration it splits the
  per-iteration morphisms into a **decide/exit cone** (backward-reachable within
  the iteration from the `LoopExit` route object — this carries both the shared
  `cond` and the exit payload) and an **advance set** (the rest — the next-state
  computation feeding `LoopBack`). Decide first, branch on the guard, advance only
  on continue. This is more code than eager-both, but it is the only reading
  consistent with the pinned goldens.
- `fir` is trap-free on the exit step (`Index(coeffs, 4)` is in the advance set,
  never evaluated when `k = 4`) ⇒ `5.375`. `sum_to_n` is unchanged (`55`; its
  discarded next-state was harmless either way). `countdown` still prints `0` on
  its exit step, because its `println` feeds the **exit** route's token and is
  therefore in the decide/exit cone — fired exactly once per iteration, before
  the guard is read.
- **Dissolves an open cross-target trap worry.** The IN6/ADR-0013 concern (a
  backend reading "Index OOB traps" literally could diverge from the oracle on a
  speculative index) does not arise for `fir`: the OOB index is never
  *semantically reached*, so there is no trap to reconcile on any target.
- Divergence is unaffected: an always-`true` guard still re-enters the advance
  set every iteration and exhausts fuel ⇒ `Diverged` (E1), never a hang.
- Backend obligation (P5+): loop codegen must be guard-first (while-loop / latched
  FSM). A backend emitting eager-both loop bodies will diverge from the oracle on
  `fir` and fail differential testing — the obligation is structural, not
  advisory. Noted in the backend capability rows when those components land.
- Scope unchanged: still single-merge / single-back / single-exit canonical SCCs
  (interp DESIGN §4); out-of-M1 shapes still error rather than miscompute.

## Spec impact (exact files/sections to patch; patched? yes — Session 08)

- `docs/spec/category-ir.md`: **no edit** (frozen Level-A; §4.5 + E1 already carry
  the Elgot reading — this ADR is the operational authority that eager-both was
  unsound). ERRATA: **no new entry** (not a spec-text defect; a design refinement
  of E1 made concrete by `fir`).
- `docs/components/interp/DESIGN.md` §4 (driver rewritten guard-first; §0/§11
  cross-refs and the spec-authority line gain ADR-0016): patched? yes — Session 08.
- `docs/decisions/` ledger + global `STATUS.md` Errata/ADR table: add ADR-0016.
