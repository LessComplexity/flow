# ADR-0002: Loops are traced in the partiality Kleisli category, not in the total core (E1)

Date: 2026-06-11 · Status: accepted

## Context (what forced the decision; spec refs)

`category-ir.md` §2.1 defines Flow-Cat's morphisms as pure **total** functions —
terminating with a value of the target type on every input. `category-ir.md` §2.7/§2.8
simultaneously claim Flow-Cat is **traced** with monoidal product equal to the categorical
product (×), i.e. a *traced cartesian* category. These two claims are jointly inconsistent:
by Hasegawa (1997), a traced cartesian category is equivalent to one carrying a Conway
fixed-point operator, but total functions lack fixpoints in general (`not : Bool → Bool`
has none). And unbounded loops are exactly where partiality must enter: `loop { -> loop; }`
is legal Flow and diverges. The §4.5 loop lowering already realizes loops as a `Trace`
over a `LoopMerge` carried-state object, so the trace structure is load-bearing — it cannot
simply be dropped. The §8.3 claim that "F_Verilog commutes with Tr" was likewise stated as
if free, when it has real content (Mealy/clocked feedback is a *different* traced structure
from software iteration).

## Decision (one paragraph, imperative)

Place loops and iteration in the **Kleisli category of the partiality (divergence) monad** —
the same §2.6 Kleisli machinery already used for I/O and errors — not in the total core.
The total core of Flow-Cat carries **no trace**; the traced structure exists only on the
partial extension, with least-fixpoint / Elgot-iteration semantics. Rewrite `category-ir.md`
§8.3 so that `F_Verilog` maps an *iteration* trace to a **guarded** trace (a register is a
unit delay, hence always productive, hence total — Mealy-machine semantics); the two traced
structures are different, so "F commutes with Tr" is a **theorem with content**, not a free
identity. State that theorem precisely via a **done-signal protocol** — *the iteration
terminates in n steps with value v ⟺ the circuit asserts `done` at cycle n with output v* —
discharge it informally now and mechanize it later (it is the project's single most
publishable result). Mark §2.1/§2.7/§2.8 with the partiality caveat accordingly.

## Consequences (tradeoffs, implementation impact)

- Tradeoff: the clean "everything is total" story is gone; divergence is now a first-class,
  *defined* outcome rather than an impossibility. This is the price of admitting real loops.
- Interpreter: all loop evaluation is **fueled** (a step/fuel limit in every test); divergence
  is a returned outcome, never a hung process — a hanging loop test is a protocol violation.
- Verilog backend: every lowered loop implements the done-protocol handshake
  (`valid_in / busy / done / result`); the FSM's termination is checked bit-for-bit against
  the fueled interpreter via differential tests.
- The E1 trace-preservation theorem is the one item reserved for Lean/Coq mechanization
  (HANDOFF §5 item 8), and only at write-up time.

## Spec impact (exact files/sections to patch; patched? yes — Session 01)

`docs/spec/category-ir.md` §2.1 (morphisms now total only on the core; partial extension
carries the trace), §2.6 (divergence-monad row added to the monad table so the §2.1/§2.7/§2.8
cross-references resolve to a named monad), §2.7 (trace lives on the Kleisli partiality
extension, not the cartesian core), §2.8 (structure summary corrected), and §8.3
(iteration-trace → guarded-trace functor; done-signal protocol theorem stated). Each patched
section is marked
`> **Erratum E1 applied — see docs/spec/ERRATA.md and ADR-0002.**`. patched? yes — Session 01.
