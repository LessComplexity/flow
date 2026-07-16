# ADR-0017: Category-architect docs tree adopted (extends ADR-0014)

Date: 2026-07-16 · Status: accepted (user-directed — Sapir invoked `/category-architect init`; revisable)

## Context (what forced the decision; spec refs)

ADR-0014 adopted `FRAMEWORK.md` as the Level-B modeling method and mandated the
`## Categorical model (Dat + Trn)` DESIGN lead-section plus the §8 reconcile gate.
It did **not** give the model a code map, a derived-suggestions channel, or an
immutable per-session record: the functor model → `file:symbol` lived implicitly in
DESIGN prose; CT-derived improvement candidates lived in `categorical-model.md` §7.5
and next-session carry-over lines; session history lived only as one-line rows in
`docs/STATUS.md` while `docs/next-session.md` is overwritten every session. The
category-architect skill (same `FRAMEWORK.md`, byte-identical) defines those three
missing artifact types and a session protocol around them.

## Decision (one paragraph, imperative)

Extend the ADR-0014 docs system with the category-architect tree, mapped onto the
repo's existing vocabulary — never forked from it (FRAMEWORK §3). Per component under
`docs/components/<name>/`: keep `DESIGN.md` as the component's ARCHITECTURE document
(its categorical-model lead section **is** the model; no parallel `ARCHITECTURE.md` is
ever created); add `IMPLEMENTATION.md` (the functor: every model object/morphism/rule
→ realising `file:symbol`, State = built/partial/planned), `suggestions.md`
(improvements derived from FRAMEWORK §3/§4.5/§5 rules only, each citing its rule), and
`plans/`, `reviews/`, `general/` folders. At top level: `docs/architecture-map.md`
(the whole-system §4 map + §4.5 coherence checklist, linking down),
`docs/IMPLEMENTATION.md` (whole-system functor, deduced from component maps),
`docs/suggestions.md` (roll-up, deduced), and `docs/sessions/YYYY-MM-DD-<slug>.md`
(immutable handoff logs — append-only, never edited after the session).
`docs/next-session.md` remains the mutable resume pointer (HANDOFF §7.1.4) and
`docs/STATUS.md` remains the global roll-up (HANDOFF §7.1.1); the session log is the
immutable record behind both. `docs/architecture/INDEX.md` remains the model index.

## Consequences (tradeoffs, implementation impact)

- The reconcile gate (HANDOFF §7.2 step 7) now also updates the touched component's
  `IMPLEMENTATION.md` in the same change as the code (FRAMEWORK §6.3) — a new
  field/pass is a new row.
- Step 8 (hand off) now writes **two** artifacts: overwrite `next-session.md` and
  append an immutable `docs/sessions/` log. Past session logs are never edited;
  corrections are a new log + living-doc reconcile.
- `IMPLEMENTATION.md` State columns are the ground truth component STATUS aggregates;
  drift there is the earliest code↔model signal.
- Suggestions are CT-derived only (no taste), and never re-litigate ledgered
  decisions (syntax W-, ir D-, lower LD-, interp IN-ledgers, categorical-model §7).
- Unmodeled components (check, rewrite, backends, cli) carry stub maps until their
  DESIGN model exists — model-first (FRAMEWORK §6.1) is unchanged.

## Spec impact (exact files/sections to patch; patched? yes/no)

Level-A spec: untouched (methodology only). `HANDOFF.md` §6 (repo layout) and §7.2
step 8 (session protocol) patched in this change — yes.
