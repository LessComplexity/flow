# Contributing to Mapal

Fork it, work on what interests you, and **bring the evidence**. Modules, recursion, sum
types, a new backend, a better packing kernel, a different scheduling algorithm — all of it
is open, none of it is spoken for.

There is one thing this project asks that most do not, and it is the whole culture:

> **A change is judged by what it demonstrably does, not by what it is expected to do.**
> Implementing it is half the work. Showing the difference — measured, on stated hardware,
> by a method someone else can repeat — is the other half. And if your change moves a number
> this repo already published, **say which one, and re-measure it.**

That rule has cost this project several of its own confident claims. A cache-blocking pass
was 3× slower than what it replaced; the first explanation of why was wrong and a control
refuted it. A "2.2× compiler spread" vanished under CPU pinning. A conv2d kernel deficit
turned out to be a page-fault inside the timer, and the kernel was ahead all along. All of
those are still in `docs/`, wrong explanations included. Yours will be treated the same way:
measured, not argued with.

---

## Contents

- [Ways to contribute](#ways-to-contribute)
- [Before you write code: the model comes first](#before-you-write-code-the-model-comes-first)
- [Where the open work is](#where-the-open-work-is)
- [Proving a correctness change](#proving-a-correctness-change)
- [Proving a performance change](#proving-a-performance-change)
- [Tests](#tests)
- [Docs you update in the same change](#docs-you-update-in-the-same-change)
- [Setup and the gates](#setup-and-the-gates)
- [Commits, PRs, licensing](#commits-prs-licensing)

---

## Ways to contribute

| You want to… | Start with |
| --- | --- |
| Report a wrong result, a crash, or a backend that disagrees with the interpreter | an issue — *Correctness bug* |
| Report or dispute a benchmark number | an issue — *Performance / measurement* |
| Propose a language feature, an op, a backend, an algorithm | an issue — *Proposal (ADR)*, then an ADR |
| Pick up something already designed | the [open ADRs](#where-the-open-work-is) or `docs/suggestions.md` |
| Fix a typo, a stale doc, a broken link | just open the PR |

You do **not** need permission to fork and build something big. You do need to have read the
design that already exists for it, if one exists — most of the interesting work here already
has an ADR arguing both sides.

---

## Before you write code: the model comes first

Mapal is built with [`FRAMEWORK.md`](FRAMEWORK.md), and the compiler's own architecture is
modeled with it (ADR-0014). Read §0–§4 once; it is the shortest path to understanding why
the code is shaped the way it is.

The framework says a running system does exactly two things — it **holds and transforms
data**, and it **transmits data between physical sites** — and decomposes into four atoms:

| Atom | Is | In this repo |
| --- | --- | --- |
| `Dat` | data types and their relations | `mapal_ir::Ty`, `RValue`, the graph's objects |
| `Trn` | transformations | passes, emitters, kernels, interp arms |
| `Loc` | physical execution sites | thread, core, register file, cache, RAM, GPU SM |
| `Trm` | a typed move between two sites | a task handoff, a DMA, a host↔device copy |

Two rules fall out of it, and both are enforced in review:

**1. Model before code (§6.1).** For anything larger than a bug fix, write the model first —
objects, morphisms with signatures (`f : A → B`), what is *stored* vs *deduced*. In this repo
that lives in `docs/components/<component>/plans/plan-<slug>.md`, or in an ADR for a language
change. A morphism table beats three paragraphs of prose, every time.

**2. No free-floating architecture (the grounding rule).** Every claim in a design names the
`Dat`/`Trn`/`Loc`/`Trm` it concerns and maps to a real `file:symbol`, a `planned` item, or an
explicit open question. "We should add a scheduling layer" is not a design. *"`path_plan`
gains a morphism `grain : Region → ℕ`, deduced not stored, realized in
`crates/mapal-ir/src/path_plan.rs`"* is.

Before you open the PR, run the **§4.5 coherence checklist** (FRAMEWORK §8) over what you
built. The one that catches the most real bugs is **placement honesty**: every transformation's
inputs are materialised at its location or delivered there by a transmission. A read of data
nothing put there is a *failed law*, not a style opinion — that is how the conv2d timing bug
was finally located (the OS handing over physical pages was an undeclared `Trm` inside the
timed region).

**The interpreter defines the language.** `crates/mapal-interp` is the oracle. If a backend
disagrees with it, the backend is wrong — unless you can argue the oracle is, in which case
that argument is an ADR.

---

## Where the open work is

**Open ADRs — designed, argued, not built.** Each one records the alternatives and what would
make it wrong. Picking one up means reading it, disagreeing with it in the open if you do, and
implementing what survives.

| ADR | What it would add | State |
| --- | --- | --- |
| [0023](docs/decisions/ADR-0023-dynamic-sized-arrays-candidate.md) | Dynamic-sized arrays — one heap tier of unknown-at-compile-time length | accepted, post-M5; surface syntax, growability and naming still open |
| [0024](docs/decisions/ADR-0024-templates-candidate.md) | Templates — monomorphising generics | candidate, undecided |
| [0025](docs/decisions/ADR-0025-TT-backend-candidate.md) | A Tenstorrent backend — the third functor target, spatial manycore | accepted, post-M5 |
| [0026](docs/decisions/ADR-0026-coproducts-sums-candidate.md) | Coproducts — sum types, variant constructors, pattern guards | candidate, undecided |
| [0030](docs/decisions/ADR-0030-backend-plugin-protocol-candidate.md) | External-backend protocol + SDK — write a backend without recompiling the compiler | candidate, unscheduled |
| [0033](docs/decisions/ADR-0033-second-consumer-proof-obligation-candidate.md) | The genericity proof obligation — CUDA consuming `tile_plan` | candidate; the discharge bar is the open part |
| [0034](docs/decisions/ADR-0034-autotuned-placement-constants-candidate.md) | Tile/thread constants searched instead of hand-set | candidate, undecided |
| [0035](docs/decisions/ADR-0035-co-execution-multi-backend-candidate.md) | Co-execution — one program, several backends at once, typed transmissions between them | candidate, unscheduled |
| [0036](docs/decisions/ADR-0036-scan-core-op-candidate.md) | `scan` — the loop/fold middle class as a Core op | candidate; the surrounding question is explicitly open |

**Not in the language at all yet**, and wanted: recursion, modules, closures, pattern
matching, strings beyond printing. None has an ADR — writing one *is* the contribution that
unblocks the code. See [`HANDOFF.md`](HANDOFF.md) §4 for what Mapal-Core deliberately excludes
and why.

**Smaller, sharper items:** `docs/suggestions.md` (improvements derived from the model, each
citing the rule it applies) and the "What is next" list in the README. Read item 4 there even
though it is done — a rewrite rule that deleted a trap, found by CI on its first run, is the
shortest illustration of what "the interpreter defines the language" costs to honour.

**Writing a new ADR.** An ADR is an *Architecture Decision Record* — one file per significant
decision, recording what was chosen, what was rejected, and why.
[`docs/decisions/README.md`](docs/decisions/README.md) is the index: it explains the numbering,
the status vocabulary, where ADRs sit in the authority order, and the full log. Short version:
next free number, `ADR-NNNN-slug-candidate.md`, `Status: candidate — NOT decided · number
provisional · changes nothing until accepted`; context argued from repo evidence rather than
preference, the decision, the alternatives you rejected and why, and what would falsify it. A
candidate binds nothing and costs nobody anything — it is the cheapest thing to open.

---

## Proving a correctness change

The bar is the same one the project holds itself to: **the same program gives byte-identical
output at `-O0` or `-O2`, on one thread or all of them, and matches the interpreter.**

- `cargo test --workspace --release` — the whole argument, ~950 tests.
- The LLVM **differential** suite emits `.ll`, compiles it, runs it and diffs against the
  oracle: 10 examples plus 320 generated programs, raw and rewritten, at both opt levels.
  New emission behavior belongs in it. It *skips itself* without `clang` — a skipped run
  proves nothing, which is why CI fails on a skip.
- Rewriter changes need the property suite (`crates/mapal-rewrite/tests/property.rs`): R1
  (`Done`/`Trapped`/`Diverged` classes never cross, output byte-exact), R2 (validate clean),
  determinism, idempotence.
- A trap, a divergence and an out-of-bounds are **observable behavior**. Deleting one is a
  bug even when the value is unused.

## Proving a performance change

State the machine, the method and the numbers, or the claim does not count. These rules are
not pedantry; every one of them was learned by publishing something wrong first
(`docs/sessions/`, and the method notes in the S33 log).

1. **Pin the CPU** (`taskset`, or the equivalent). Unpinned readings on a hybrid P/E chip have
   twice produced confident, wrong conclusions here.
2. **Single-threaded cells: the minimum of N runs. Threaded cells: the median.** Inverted from
   the usual rule for a specific reason — a known pool race makes ~3–4% of threaded runs
   self-time far too *low*, and a minimum is maximally vulnerable to a fast outlier.
3. **Compare ratios within one run, never against a number from another session.** A C++
   baseline here drifted 41% between two sessions on the same machine.
4. **Measure the kernel, not the harness.** Allocation, generation and setup belong outside the
   timed window on *both* sides, or inside it on both.
5. **`ref-cycles`, not `cycles`,** when frequency may vary — `cycles` cannot distinguish a
   slower clock from more work.
6. **Same machine, both legs, stamped.** Machine specs go on the results, not in the PR prose.
7. **Rebuild everything the leg links.** `cargo test` does not rebuild
   `target/release/libmapal_rt.a`; a stale static library presents exactly like a fix that does
   nothing.
8. **Say what your change does *not* improve**, and include the cells where it loses.

Then the part that matters most:

> **Which previously published numbers does this change?** Name them — the README rows, the
> `docs/performance/` tables — and either re-measure them in the same PR or say plainly that
> they are now stale. A repo whose numbers silently rot is worse than one with no numbers.

A change that is *neutral* on performance is fine. Say so, with the measurement that shows it.

## Tests

**A test that passes wrongly is worse than none.** Three of those shipped here in one session
and were caught the same way each time — by **negative control**: break the thing on purpose
and check the test notices. Do that for every new assertion, especially in a table-driven or
scope-matching suite where a passing assertion may be matching something else entirely.

Editor grammars have their own suite (`sh editors/test.sh`, 61 assertions) and opposite
precedence rules between Vim and TextMate — a rule added at one end of one goes at the other
end of the other.

## Docs you update in the same change

Not after. In the same PR (FRAMEWORK §6.3):

- A new field or relation is a **new morphism** — add it to the component's morphism table in
  `docs/components/<component>/ARCHITECTURE.md`, with signature, partiality and semantics.
- `docs/components/<component>/IMPLEMENTATION.md` maps model → `file:symbol`. Keep the row true.
- `docs/components/<component>/STATUS.md` and `docs/STATUS.md` record what is built, partial or
  unbuilt.
- If you found that the code violates a documented rule, either fix the code or add an explicit
  `Note:` exception to the rule. Undocumented exceptions rot into bugs.

`docs/sessions/*.md` are immutable maintainer logs — never edit a past one. New findings go in
new docs.

## Setup and the gates

Needs a recent Rust toolchain and `clang`.

```sh
cargo fmt --all --check                    # CI gate 1
cargo test --workspace --release           # CI gate 2 — the correctness argument
sh editors/test.sh                         # if you touched editors/

# the differential must actually run, not skip:
cargo test --release -p mapal-backend-llvm --test differential -- --nocapture
```

`crates/mapal-rewrite/tests/property.proptest-regressions` pins counterexamples CI has drawn.
**Never delete a seed to get a green build** — a pinned seed is the only thing stopping a
randomised suite from passing on a lucky draw. If a pinned seed fails for you, that is the
finding, not the obstacle.

CI runs `fmt` plus the full suite on Linux and macOS, and builds the CUDA backend (its 161
tests skip without `nvcc`; compiling still catches emitter-wiring breaks).

## Commits, PRs, licensing

- **Conventional commits**, scoped: `fix(rewrite): …`, `feat(backend-llvm): …`, `perf(s33): …`,
  `docs: …`, `test(editors): …`. The subject says what changed; the body says *why*, including
  what you tried that did not work.
- **PRs**: fill in the template. It asks for the model, the proof, and the affected numbers —
  the same three things this page is about.
- Keep unrelated changes out of the diff. Reformatting plus a behavior change in one PR means
  neither can be reviewed.
- **Licensing**: contributions are accepted under the repo's
  [Apache-2.0 with the LLVM exception](LICENSE) (Apache-2.0 §5, inbound = outbound). No CLA.
- Be the kind of collaborator described in the [Code of Conduct](CODE_OF_CONDUCT.md).

Disagreement is welcome and expected — this project has been wrong in public repeatedly. Bring
a measurement or an argument from the model, and it will be taken seriously.
