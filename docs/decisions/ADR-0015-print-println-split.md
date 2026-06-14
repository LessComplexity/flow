# ADR-0015: Split the print effect — `print` is raw, `println` appends a newline

Date: 2026-06-14 · Status: **accepted** — decided with Sapir (Session 07)

## Context (what forced the decision; spec refs)

Flow-Core's effect surface was `print` only (HANDOFF §4.1), with the newline
behaviour left unpinned by the spec (`user-guide` shows `"Hello, world!" -> print;`
with no stated console output). Designing the interpreter (`interp/DESIGN.md`,
ADR-0002 oracle) forced the question, and the example set is internally
inconsistent under any single rule for a newline-appending `print`:

- `fanout.flow` expects `36`⏎`12` and the user-guide countdown expects one value
  per line — i.e. `print` **appends** a newline; but
- `pipeline.flow` does `"f(10) = " -> print; result -> print;` and expects the
  single line `f(10) = 25` — i.e. `print` does **not** append a newline.

No one rule satisfies both. The clean resolution is the standard split: a raw
`print` and a line-terminating `println`. This also removes the need to "correct"
`pipeline.flow`'s header: under the split it is exactly right (label via `print`,
value via `println`).

## Decision (one paragraph, imperative)

Add `println` to the Flow-Core effect surface. **`print`** writes its argument's
rendered text with **no** trailing newline; **`println`** writes the rendered text
**followed by `"\n"`**. Both have the Kleisli(IO) signature `(IoToken, P) → IoToken`
(ir §8), obey the same token laws (I4/I4b), and are legal only in sequential
context (E2). In the IR they are **one parameterized morphism**,
`Operation::Print { newline: bool }` (idiomatic, like `Pair { slot, arity }`) —
`newline: false` = `print`, `newline: true` = `println`; the §5.1 typing row is
unchanged, the Mermaid label is `Print` / `Println`. `println` is a **reserved
builtin name** resolved in `flow-lower` exactly as `print` is (it is an
identifier, not a keyword — `flow-syntax` is unchanged). Examples that want
line-separated output use `println`; `pipeline.flow` uses `print` for the label
and `println` for the value (→ `"f(10) = 25\n"`).

## Consequences (tradeoffs, implementation impact)

- **`flow-ir`** (`ir/DESIGN.md`): `Operation::Print { newline: bool }`; builder
  gains `println(token, value, loc)` alongside `print(...)` (or one method with a
  `newline` flag); `validate`, the I4/I4b token rules, and topo/SCC are unchanged
  (the bool does not affect dataflow); Mermaid renders `Println` when `newline`.
  The §5.1 typing table row is unchanged. Existing `Print` goldens that should now
  terminate a line are regenerated.
- **`flow-lower`** (`lower/DESIGN.md`): `println` joins `print` in the reserved
  builtins; `-> println` lowers to `Print { newline: true }`. The `countdown`
  regression fixture (golden h) switches its body `print` to `println`. Golden
  snapshots regenerated; L-code catalogue unchanged.
- **`flow-syntax`**: **no change** — `print`/`println` are identifiers reserved at
  lowering, not keywords.
- **`examples/`**: `abs`, `sum_to_n`, `fanout`, `fir`, `sepia` switch their
  terminal `print` to `println` (they all want a trailing newline);
  `pipeline.flow` keeps `print` for `"f(10) = "` and uses `println` for the value.
  Each example's expected-output header is now exactly the program's output —
  `pipeline`'s `f(10) = 25` is correct as a single line.
- **`flow-interp`** (not yet built): renders `Print{newline:false}` raw and
  `Print{newline:true}` with a trailing `"\n"`. Acceptance outputs: `abs "7\n"`,
  `sum_to_n "55\n"`, `pipeline "f(10) = 25\n"`, `fanout "36\n12\n"`, `fir "5.375\n"`,
  `sepia "4080\n"`. This **supersedes interp IN5** (the print-appends-newline pin)
  and resolves the `pipeline` open item.
- **Scope.** This widens Flow-Core's effect surface from `{print}` to
  `{print, println}` — a deliberate, ADR-gated scope change (HANDOFF §4). It is
  small (one bool on one op) and does not touch the categorical effect model
  (still Kleisli(IO), one world token).
- **Reversible** via a superseding ADR; the cost is the example rewrites + one IR
  field.

## Spec impact (exact files/sections to patch)

- `HANDOFF.md` §4.1 (Flow-Core scope, Effects line): `print` → `print` and
  `println`. patched: yes (this session).
- `docs/components/ir/DESIGN.md` §5 (Operation set, `Print` → `Print{newline}`),
  §5.1 (typing row note), §14 (Mermaid label), §16 (golden list). patched: with
  the ir increment.
- `docs/components/lower/DESIGN.md` (reserved builtins; countdown fixture).
  patched: with the lower increment.
- `docs/components/interp/DESIGN.md` §3/§5/§11/§13/§14 (supersede IN5). patched:
  this session.
- Frozen Level-A spec (`category-ir.md`, `user-guide.md`): **not patched** here —
  `user-guide` examples use `print`; a later doc pass may add `println` to the
  user guide, but Core scope is governed by HANDOFF + this ADR. patched: n/a.
