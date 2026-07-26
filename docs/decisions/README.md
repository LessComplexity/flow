# Decisions — the ADR log

**ADR = Architecture Decision Record.** One file per significant decision, recording what was
decided, *what was rejected and why*, and what would make it wrong. The point is not
ceremony — it is that nobody re-argues a settled question from memory, and that a decision
which turns out badly can be found and reversed with its reasoning intact.

A decision here is **not** a plan and **not** a status report. Plans live in
`docs/components/<component>/plans/`, state lives in `docs/STATUS.md`, and what happened on a
given day lives in `docs/sessions/`. An ADR is the thing those three all defer to.

## How to read this corpus

Authority order for anything about the language (`HANDOFF.md` §2.2, fixed by ADR-0022 D1):

1. **Accepted ADRs in this directory** — newest wins where two overlap;
2. the v0.2 spec corpus as patched by errata E1–E5 and living corrections LC-1–5;
3. and above all of it, **the interpreter's behaviour is the final arbiter** — if an ADR and
   the oracle disagree, the oracle is what the language *is*, and the gap is a bug in one of
   them.

An ADR is never edited to reverse itself. A later ADR **supersedes** or **amends** it, and the
older file stays as written — the same immutability rule the session logs follow.

## Numbering and naming

```
ADR-NNNN-short-slug.md              e.g. ADR-0021-array-update.md
ADR-NNNN-short-slug-candidate.md    while the decision is still open
```

- `NNNN` is zero-padded, sequential, and never reused. **The next free number is 0037.**
- A new proposal takes the next number with `Status: candidate — NOT decided · number
  provisional`. A candidate **binds nothing and changes nothing**, which is what makes it cheap
  to write and cheap to reject.
- Every file opens with `# ADR-NNNN: <title>` and a `Date: … · Status: …` line. That Status
  line is authoritative.
- **Filename wart, recorded rather than hidden:** three files still carry `-candidate` in the
  name after being accepted (0023, 0025, 0029). Renaming them would break inbound links from
  the session logs, which are immutable. Trust the `Status:` line, not the filename.

## Status vocabulary, as actually used

| Status | Means |
| --- | --- |
| `accepted` | decided; binding on the code |
| `accepted-for-implementation` | decided, and being built in stages — the ADR tracks which shipped |
| `candidate — NOT decided` | written up, argued both ways, binding nothing. **Unclaimed work** |
| `amended by ADR-NNNN` | still stands, with a later refinement |
| `superseded by ADR-NNNN` | replaced; kept for the reasoning |

## The log

| # | Title | Date | Status |
| --- | --- | --- | --- |
| [0001](ADR-0001-flow-core-scope.md) | Flow-Core (v0.3 subset) is the frozen implementation scope through M5 | 2026-06-11 | accepted |
| [0002](ADR-0002-loops-partiality-trace.md) | Loops are traced in the partiality Kleisli category, not the total core | 2026-06-11 | accepted — encodes erratum **E1** |
| [0003](ADR-0003-parallel-effects-kpn.md) | Effects forbidden in parallel fanout; channels use Kahn process-network semantics | 2026-06-11 | accepted — **E2** |
| [0004](ADR-0004-memory-guarantee-scope.md) | The zero-annotation memory guarantee is scoped to the first-order non-cyclic core | 2026-06-11 | accepted — **E3** |
| [0005](ADR-0005-flow-is-a-statement.md) | `a -> b + c -> d` parses as `a -> (b + c) -> d`; a flow is a statement, not a value | 2026-06-11 | accepted — **E4** |
| [0006](ADR-0006-rename-category-to-type.md) | Rename the surface keyword `category` to `type` | 2026-06-11 | accepted — **E5** |
| [0007](ADR-0007-tech-stack.md) | Compiler stack — Rust, handwritten front end, arena IR, interpreter oracle | 2026-06-11 | accepted |
| [0008](ADR-0008-editor-tooling-lsp.md) | Editor tooling — syntax files now, `flow-lsp` later | 2026-06-11 | accepted |
| [0009](ADR-0009-collection-operator-syntax.md) | Collection operators take a postfix inline block; input tuple ↔ block params positionally | 2026-06-11 | accepted |
| [0010](ADR-0010-guard-arrow-lexing.md) | Guard arrows are single lexemes — adjacency + statement-initial context gate | 2026-06-11 | accepted |
| [0011](ADR-0011-loop-labels-ident-brace.md) | Loop labels are the keyword `loop` only; statement-initial `Ident {` resolved by scan | 2026-06-11 | accepted — **amended by 0012** |
| [0012](ADR-0012-labeled-blocks-sigil.md) | Labeled blocks `:label { … }`, jumps `-> :label;`, enclosing targets only | 2026-06-12 | accepted |
| [0013](ADR-0013-ir-realization.md) | IR realization — all dataflow is edges; Core op set; loops as inline cycles; IO as a linear token | 2026-06-12 | accepted |
| [0014](ADR-0014-categorical-architecture-model.md) | `FRAMEWORK.md` is the Level-B model layer for compiler-internal design, distinct from Flow-Cat | 2026-06-13 | accepted |
| [0015](ADR-0015-print-println-split.md) | Split the print effect — `print` raw, `println` appends a newline | 2026-06-14 | accepted |
| [0016](ADR-0016-loop-guard-first-evaluation.md) | Loop branch evaluation is guard-first — no speculative continue-branch on the exit step | 2026-06-15 | accepted — refines E1 |
| [0017](ADR-0017-category-architect-docs-tree.md) | The category-architect docs tree, and immutable session logs | 2026-07-16 | accepted |
| [0018](ADR-0018-zip-enumerate-core.md) | `zip` and `enumerate` join Flow-Core as collection primitives | 2026-07-16 | accepted |
| [0019](ADR-0019-seq-statement-block.md) | `seq` is a statement block, not a fanout kind | 2026-07-16 | accepted |
| [0020](ADR-0020-backend-emission-contract.md) | Backend emission contract — one convention, one runtime, oracle-parity semantics | 2026-07-17 | accepted |
| [0021](ADR-0021-array-update.md) | Array element update — pure `Update` op + `c[i] <- x` rebind sugar | 2026-07-18 | accepted |
| [0022](ADR-0022-truth-in-docs-and-level-b-freeze.md) | Truth in docs — the as-implemented index, "frozen" retired, Level-B maintenance freeze | 2026-07-18 | accepted |
| [0023](ADR-0023-dynamic-sized-arrays-candidate.md) | Dynamic-sized arrays — one heap tier of unknown-at-compile-time length | 2026-07-18 | accepted, post-M5 — **surface syntax, growability and naming still open** |
| [0024](ADR-0024-templates-candidate.md) | Templates — C++-style monomorphizing generics | 2026-07-18 | **candidate** |
| [0025](ADR-0025-TT-backend-candidate.md) | A Tenstorrent backend — the third functor target, spatial manycore | 2026-07-18 | accepted, post-M5 |
| [0026](ADR-0026-coproducts-sums-candidate.md) | Coproducts — named sum types, variant constructors, pattern guards | 2026-07-19 | **candidate** |
| [0027](ADR-0027-capture-semantics.md) | Capture semantics — `map`/`fold` bodies may read enclosing bindings | 2026-07-21 | accepted |
| [0028](ADR-0028-tree-reduction-exact-op-folds.md) | Tree reduction for exact-op folds — associativity as a graph property | 2026-07-22 | accepted |
| [0029](ADR-0029-array-construction-builtins-candidate.md) | Array-construction builtins — `iota`, `fill`, numeric widening | 2026-07-22 | accepted-for-implementation — stages 1 + 2 shipped |
| [0030](ADR-0030-backend-plugin-protocol-candidate.md) | External-backend protocol + SDK — write a backend without recompiling the compiler | 2026-07-22 | **candidate**, unscheduled |
| [0031](ADR-0031-iota-fill-pipeline-surface.md) | `iota`/`fill` ride the pipeline — the call-expression carve is removed | 2026-07-22 | accepted |
| [0032](ADR-0032-precision-contracts-vs-backend-config.md) | Precision contracts in the type system, machine tailoring in backend config | 2026-07-24 | accepted |
| [0033](ADR-0033-second-consumer-proof-obligation-candidate.md) | The backend-genericity proof obligation — CUDA consumes `tile_plan` | 2026-07-25 | **candidate** |
| [0034](ADR-0034-autotuned-placement-constants-candidate.md) | Placement constants are searched, not set — an autotuner over `tile_plan` | 2026-07-25 | **candidate** |
| [0035](ADR-0035-co-execution-multi-backend-candidate.md) | Co-execution — one source, several backends at once; `Trm` as cross-backend transmission | 2026-07-25 | **candidate**, unscheduled |
| [0036](ADR-0036-scan-core-op-candidate.md) | `scan` — the loop/fold middle class as a first-class Core op | 2026-07-25 | **candidate** |

Errata **E1–E5** predate the ADR numbering; ADR-0002 … ADR-0006 encode them one-for-one. The
historical ledger — including which errata were patched back into the spec text — is the
`Errata/ADR ledger` table at the bottom of [`docs/STATUS.md`](../STATUS.md).

## Open — the candidates are unclaimed work

Seven ADRs are written, argued and unbuilt: **0024** (generics), **0026** (sum types),
**0030** (external-backend SDK), **0033** (the genericity proof obligation), **0034**
(autotuned constants), **0035** (co-execution), **0036** (`scan`). Plus **0023** (dynamic
arrays), accepted but with its surface syntax still open.

Picking one up means reading it, disagreeing in the open if you disagree, and building what
survives. See [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Writing one

1. Take the next free number (**0037**), name the file `ADR-0037-<slug>-candidate.md`.
2. Open with `# ADR-0037: <title>` and `Date: YYYY-MM-DD · Status: candidate — NOT decided ·
   number provisional · changes nothing until accepted`.
3. **Context** — what forces the decision, argued from repo evidence (`file:symbol`, a measured
   wall, a rejection you hit), not from preference.
4. **Decision** — the categorical model first (objects, morphisms with signatures, what is
   deduced rather than stored), then the mechanics.
5. **Alternatives rejected** — each with the reason. This is the part future readers actually
   need.
6. **What would falsify it** — the measurement or failure that should make someone reverse it.
7. If it changes the language surface, say what the **interpreter** does with it; the oracle
   defines the language, so no backend can be specified before it.
