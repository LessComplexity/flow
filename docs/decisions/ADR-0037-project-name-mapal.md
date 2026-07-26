# ADR-0037: The language is named **Mapal**; source files are `.mapal`

Date: 2026-07-26 (S34) · Status: **accepted — decided by Sapir 2026-07-26 ("Let's go with Mapal.
Sounds the best" · confirmed at close: "Mapal is definitive. The extension name might change")**.
The **name** is settled; the **extension** is explicitly revisable (D4). Number final: ADR-0037. Supersedes nothing — "Flow" was never fixed by an
ADR, which is part of what made this cheap.

## Context — what forced the decision

The project went public on 2026-07-26 (S33) as `LessComplexity/flow`. Within a day the name was
found to be unusable, and not for the reason expected. It is not that "flow" is a common word —
it is that **the nearest neighbour in the same niche already owns the name and the toolchain
around it**, verified against the registries rather than assumed:

| Evidence | State |
| --- | --- |
| `flowc` on crates.io | **taken** — *"A compiler for 'flow' programs"*, 92,059 downloads, `andrewdavidmackenzie/flow` — a Rust-implemented **dataflow language** whose compiler binary is `flowc` |
| `flow` on crates.io | taken — a realtime log analyzer, 14,289 downloads |
| flow.org / flow-lang.com | Meta's Flow, a JS type checker — compiler-adjacent, dominates search |
| `flow-lang`, `flow-ir`, `flow-rt` | free, so nothing of ours was squatted yet |

`cargo install flowc` was therefore never available to this project, and a search for "flow
language dataflow rust compiler" lands on someone else's work.

Cheapest possible moment to fix it: the repository was one day old, with **0 issues, 0 pull
requests, no crates.io publications, and no releases**. GitHub redirects renamed repository
URLs, so inbound links survive.

## Decision

**D1 — The language is `Mapal`. Source files are `.mapal` (provisional — see D4).**

מפל (*mapal*) is Hebrew for **waterfall** — flow falling through stages, which is what a pass
pipeline and a dataflow graph both are. Three properties decided it over ~150 checked
alternatives:

1. **It keeps the lineage.** The project was called Flow because a program *is* a flow; Mapal
   says the same thing in the language it was written in, without the collision.
2. **It contains `map`** — the language's central operator (`map`/`fold`). Nothing else in the
   entire search had both a lineage meaning *and* a core operator inside its Latin letters.
3. **One pronunciation: MA-pal.** No accents, no ambiguity, spells itself from hearing, works
   in every language. This eliminated most of the field (Kleis, Etale, Cadere and Kleisli each
   had three or four plausible readings; a name you must correct forever is a tax).

Availability, verified 2026-07-26: `mapal` is free on **crates.io and npm**; **no GitHub
repository** bears the name (nearest is `mapalign`, 69★, unrelated); `mapal.dev`, `mapal.io`,
`mapal.sh` and `mapal-lang.com` are unregistered. PyPI is taken and irrelevant — nothing here
publishes Python. `mapal.com` belongs to MAPAL Dr. Kress, a German precision-tooling firm in an
unrelated sector which has left **every software namespace open**.

**D2 — Naming scheme, fixed here so it stops being a per-file judgement call.**

| Surface | Was | Is |
| --- | --- | --- |
| language / project | Flow | **Mapal** |
| implemented subset | Flow-Core | **Mapal-Core** |
| Level-A categorical model | Flow-Cat | **Mapal-Cat** |
| source extension | `.flow` | **`.mapal`** (provisional, D4) |
| CLI binary | `flow` | **`mapal`** |
| crates | `flow-ir`, `flow-rt`, … | `mapal-ir`, `mapal-rt`, … (modules `mapal_ir`, …) |
| runtime ABI symbols | `flow_rt_*`, `flow_par_*`, `flow_trap`, `flow_main`, `flow_print_*` | `mapal_rt_*`, `mapal_par_*`, `mapal_trap`, `mapal_main`, `mapal_print_*` |
| environment variables | `FLOW_PAR`, `FLOW_PERF`, `FLOW_FILE`, `FLOW_SLICE`, `FLOW_LD`, `FLOW_REQUIRE_CLANG`, `FLOW_EMIT_SWEEP_MERMAID`, `FLOW_BENCH_MAX_N` | `MAPAL_*` for each |
| editor filetype / TextMate scope | `flow` / `source.flow` | `mapal` / `source.mapal` |
| bench leg labels | `flow-llvm-*`, `flow-cuda-*` | `mapal-llvm-*`, `mapal-cuda-*` |

**D4 — The extension is `.mapal`, and it is provisional.** Sapir, 2026-07-26: *"Mapal is
definitive. The extension name might change."* The name and the extension are therefore decided
at different confidence levels, and this section is what a future change reads first.

It took two attempts, and the failure is worth recording because a test caught it rather than a
human:

- **`.flow` → `.mp`** was the first choice — short, obvious. It is **MetaPost**. Every Vim and
  Neovim install ships `syntax/mp.vim`, which sources METAFONT's groups and wins the filetype
  race, so `editors/test.sh` failed 29 highlighting assertions with `mfNumeric` leaking in.
  GitHub's Linguist would have mislabelled all 49 source files the same way. Nobody would have
  thought to test "does the extension we picked already belong to someone" — the editor suite
  did it for free.
- **`.mp` → `.mapal`**, after checking the alternatives against Linguist's `languages.yml`
  rather than guessing.

| Extension | Owner | Verdict |
| --- | --- | --- |
| `.mapal` | unclaimed in Linguist, unclaimed in Vim | **chosen** — self-documenting, zero collision |
| `.mal` | unclaimed | the only viable shorter option; note *mal* = "bad" in Spanish, German and French |
| `.mp` | MetaPost (Vim, in practice) | rejected — proven broken by our own suite |
| `.mpl` | **JetBrains MPS** (Linguist) | rejected — GitHub would label the files MPS |
| `.ml` | **OCaml *and* Standard ML** (Linguist); Vim ships `ocaml.vim` | rejected — and the collision sits inside this project's own audience |
| `.map` | unclaimed in Linguist, but universally source maps (`app.js.map`) and linker maps | rejected on ambiguity |

**Cost of changing it later, measured rather than feared:** one scripted pass — rename 49 files,
rewrite ~300 references, update `editors/nvim/ftdetect/mapal.vim`, `editors/vscode/package.json`,
the test fixtures, and re-run the gate. Roughly ten minutes of machine time. Any replacement must
be checked against Linguist's `languages.yml` *and* `$VIMRUNTIME/syntax/` first, because being
unclaimed on crates.io says nothing about being unclaimed as a file extension.

**What deliberately keeps the word "flow":** lowercase *flow* remains the name of the language
construct — `a -> b -> c;` is a **flow statement**, and the model is a **dataflow graph**. That
is the dataflow vocabulary, not the brand, and ADR-0005's "a flow is a statement, not a value"
still holds verbatim.

**D3 — Immutable history is not rewritten.** ADR-0017 makes session logs immutable, and this
directory's rule is that an ADR is never edited to reverse itself. Therefore untouched:

- `docs/sessions/**` — they say "Flow" because that is what it was called then;
- `docs/decisions/ADR-0001 … ADR-0036` — same reason;
- `docs/performance/**` and every recorded `results*.csv` — a number's provenance includes the
  name the binary had when it was measured, so old CSVs keep their `flow-llvm-*` leg labels
  while the harness now writes `mapal-llvm-*`.

A reader who greps for "Flow" will find it in exactly the places where rewriting it would be a
lie. This ADR is the pointer that explains why.

## What the rename actually touched

| Surface | Count |
| --- | --- |
| crate module references | 691 |
| runtime symbol occurrences | 1,409 |
| environment-variable occurrences | 202 |
| `.flow` → `.mapal` source files | 49 |
| living files rewritten in the docs/editor pass | 323 |
| TextMate scope names | 83 |

The emitter snapshots were the one non-mechanical part: renaming `flow_main` changes every
golden `.ll` and `.cu`. Those files are *expected output*, so they were re-pinned, and the
**differential suite is what proves behaviour did not move** — the goldens only prove the text
did not move for any other reason.

## Alternatives rejected

~150 names across eight families were checked against crates.io, npm, PyPI, GitHub and RDAP.
The pattern worth recording: **crates.io is exhausted for English dictionary words**;
availability survives only in other languages, precise technical terms, and invented compounds.

| Rejected | Why |
| --- | --- |
| **Homset** | Briefly chosen, then reversed. `Hom(A,B)` is the set of arrows `A → B`, so the name states the guarantee the differential proves — but it abandons the lineage and signals "category theory required". Everything free except `.com`; would still be the pick if the criterion were rigour over continuity |
| **Mapsto** (`↦`) | Everything free; the only name a working programmer decodes in three seconds. Names one glyph rather than the idea, and "map" reads *geographic* to many |
| **Lessplex** / Lesplex / Leplex | From LessComplexity. The cleanest namespace found anywhere — `lessplex` has a free `.com` **and** GitHub handle. Rejected: names the org, not the language, and "less complex" reads as an ease-of-use claim on a research compiler |
| **Kleis** | **An existing language** — kleis.io, "Universal Verification Platform", Z3 + LAPACK. Adjacent niche: the worst kind of collision |
| **Quiver** | Technically ideal — a quiver *is* a directed multigraph, and FRAMEWORK already says `Trn` is a quiver over `Dat`. Crate and npm taken; `varkor/quiver` is a 3,595★ commutative-diagram editor |
| **Cadere** (Latin, "to fall" → cascade) | Full registry trifecta free, good lineage. Four plausible pronunciations, and **Cadence Design Systems** sits one syllable away in this project's own neighbourhood |
| **Eutectic** | The exact composition where dissimilar metals melt as one uniform solid — a fine metaphor for many backends, one semantics. Four syllables and a clumsy extension |
| **Floph / Flaph / Flaf** | Free, but `fl-` + short vowel + soft `f` is English's "unsteady or failed" cluster (flop, flap, flab, faff), and **FLOP** is this project's own headline unit. A performance compiler cannot be a near-homograph of "failure" |
| **Garrow** (graph + arrow) | `garrow_`/`GArrow*` is Apache Arrow's GLib C API — a collision inside the ecosystem this project benchmarks against |
| **Mapold** (map + fold) | Free, but reads as "map, *old*" |
| **Koski** (fi, rapids) · **Aruvi** (ta, waterfall) · **Nurt** (pl, current) · **Wailele** (haw) | The best of the cross-linguistic sweep, all fully free. Behind Mapal: surname-first, unfamiliar, or too long — and none carries `map` |
| **Afik** (אפיק) · **Penstock** · **Binah** (בינה) · **Detent** · **Nahar** (נהר) · **Eciton** · **Erewhon** · **Cotile** | Clean metaphors, each a rung below on legibility or lineage |
| **Taki** (ja) · **Virta** (fi) · **Potok** (sl) · **Dhara** (sa) · **Arus** (ms) · **Agos** (tl) · **Selale** (tr) | Namespace collisions — npm, PyPI, or a notable repo. `dhara` in particular is a 496★ flash-translation library |
| **BAPIR** (Backend-Agnostic Parallel IR) | Names the implementation, not the language; users write programs, not IRs. Acronyms do not survive speech — and LLVM, the one that won here, officially **retired its expansion** for becoming inaccurate. Kept as a tagline |
| Metals — braze, solder, temper, carbide, ingot, corundum, amalgam | Every one taken. Mineral and metal names have been mined by language authors for two decades (Ruby, Crystal, Onyx, Garnet, Obsidian, Flint, Alloy, Nickel, Steel, Zircon) |
| Biology — stigmergy, mycelium, rhizome, hypha, physarum | **Stigmergy** — coordination with no coordinator, exactly this runtime — is taken on crates.io *and* npm. So are the rest |
| Keeping **Flow** | The Context table. `flowc` is someone else's compiler for someone else's dataflow language |

## Costs accepted

- **MAPAL Dr. Kress** is a real brand with 5,000 employees. Different sector, no software
  namespace overlap, and no trademark class collision expected — but the name is not unique in
  the world, and a search for "mapal" returns cutting tools first until this project outranks
  them.
- **PyPI `mapal` is taken.** Irrelevant today; it would matter if Python bindings were ever
  published, in which case the package would need a suffix.
- **`mapal.com` is unavailable.** The canonical domain will be `mapal.dev` or `mapal.io`.
- **The `map` coincidence cuts both ways** — a reader may assume the language is only about
  mapping, when `fold`, `zip`, `scan` and loops matter just as much.
- **Recorded results now use two vocabularies** (`flow-llvm-*` in old CSVs, `mapal-llvm-*` in
  new ones). Deliberate, per D3.

## What would falsify this

- A trademark claim from a software vendor named Mapal. None found; MAPAL Dr. Kress is in
  metalworking tools.
- The name proving to be an adoption barrier, *measured* — contributors saying they could not
  tell what the project does. That argues for a better tagline, not another rename.
- Two renames would be worse than one imperfect name. This ADR is therefore also a commitment:
  the next rename needs an ADR superseding this one and an argument stronger than taste.

## Acceptance

- [x] `cargo build --workspace --release` clean after every phase of the rename.
- [ ] `cargo test --workspace --release` green, with the differential proving observable
      behaviour unchanged (goldens change; behaviour does not).
- [ ] `sh editors/test.sh` green with filetype `mapal` and scope `source.mapal`.
- [ ] No `flow` identifier, symbol, environment variable, extension, crate or scope name left
      in living code or docs — every remaining occurrence is either the dataflow *construct*
      (per D2) or inside the immutable set (D3).
- [ ] Repository renamed to `LessComplexity/mapal` — **Sapir's action**; the README badges point
      at the new path and will 404 until it happens. GitHub redirects the old URL.
- [ ] Wordmark text updated to `mapal` (done) and `MapalIcons.ttf` regenerated so the font's
      *internal* family name matches its filename — follow-up, tracked rather than faked.
