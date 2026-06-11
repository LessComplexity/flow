# ADR-0008: Editor tooling — Vim/Neovim syntax now, planned `flow-lsp` later, with constraints imposed on P1/P3 now

Date: 2026-06-11 · Status: accepted

## Context (what forced the decision; spec refs)

Developer experience for `.flow` files was requested by Sapir (Session 01). Modern editors get
their language smarts over the **Language Server Protocol** (LSP), and the cost of a good LSP is
not paid by the LSP — it is paid (or made impossible) by the parser and diagnostic architecture
underneath it. The P1 frontend (HANDOFF §8) is about to be designed, and HANDOFF §5 decision 2
already fixes a handwritten recursive-descent parser with spanned diagnostics (`SourceLoc` from
day one); decision 3's builder-enforced IR is the substrate constraint (c) below leans on. Whether an LSP is cheap to add later is therefore decided **now**, not
when someone writes the server. This ADR records two things: a tooling artifact to ship soon, and
a small set of binding constraints on the front end and checker so that the later server is a thin
adapter over libraries rather than a rewrite. No spec semantics change.

## Decision (one paragraph, imperative)

**Part 1 (now).** Ship a regex-based Vim/Neovim syntax-highlighting plugin at `editors/nvim/`.
It is a tooling artifact, not normative — the spec remains law (HANDOFF §2.2), and the plugin
tracks surface syntax on a best-effort basis. The repo layout (HANDOFF §6) gains an `editors/`
directory; this is an **addition, not a contradiction**, recorded here. A Tree-sitter grammar is
**deferred** until it can be derived from the real `flow-syntax` parser rather than hand-maintained
in parallel. **Part 2 (planned crate `flow-lsp`).** A future LSP server speaking over stdio that
consumes `flow-syntax` and `flow-check` **as libraries** (tower-lsp vs lsp-server chosen at
implementation time, not now). To make that server cheap, the following constraints bind P1/P3
**from their first design session**: **(a)** the parser must be **error-recovering** — it always
returns a parse tree *and* a diagnostics list, and never bails at the first error; **(b)**
diagnostics are **structured values** (`code`, `severity`, `SourceLoc` span, message, optional
suggested fix); rendering them to terminal text happens **only** in `flow-cli`, never inside the
parser or checker; **(c)** `flow-check` exposes **per-node type information queryable by span** (the
substrate for hover); **(d)** every pipeline stage is a pure `fn(source) -> artifacts` with no
global state — at Flow-Core scale a full-file reparse per keystroke is acceptable, so **no
incremental/edit-tracking machinery** is built. These four are the heart of this ADR; they cost the
front end a little now and pay both the CLI and the eventual LSP later.

**Capability ladder (informative).** v0: parse diagnostics (possible once P1 lands). v0.5:
type/effect/lifetime diagnostics (post-P3 / M1). v1: hover types + go-to-definition. v2: semantic
tokens + a Mermaid graph preview of the dataflow graph (reusing `flow-ir`'s Mermaid dump, HANDOFF §5
item 6). **Timing.** No `flow-lsp` skeleton earlier than **post-M1**; it is explicitly **not on the
M5 critical path** and falls under the HANDOFF §8 parking rule. `docs/components/lsp/` is created
only when work actually starts.

## Consequences (tradeoffs, implementation impact)

- The P1 parser is slightly costlier because of error recovery, but the same property gives
  `flow-cli` multi-error reporting and the future LSP its diagnostics stream for free — one
  investment, two consumers.
- Structured diagnostics with rendering confined to `flow-cli` keep `flow-syntax`/`flow-check`
  presentation-free, so the LSP can serialize the same values to LSP `Diagnostic` objects with no
  duplicated logic.
- The `editors/nvim/` syntax file is hand-maintained and must be updated whenever surface syntax
  changes (e.g. the E5 `category` → `type` rename); it can lag and is non-authoritative by design.
- Deferring Tree-sitter avoids maintaining a second grammar in lockstep with the parser; the cost
  is no Tree-sitter-based tooling until the derive-from-parser path exists.
- The LSP **cannot silently pull effort from M5**: it is parked post-M1 and off the critical path,
  so adding it never competes with the tri-target demo.

## Spec impact (exact files/sections to patch; patched? n/a)

Tooling decision — no change to the v0.2 corpus or its semantics. The only layout note is the new
`editors/` directory (and a future `crates/flow-lsp/` + `docs/components/lsp/`), an addition to
HANDOFF §6 recorded here rather than a correction. The binding record is this ADR. patched? n/a.
