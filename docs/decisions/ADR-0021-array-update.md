# ADR-0021: Array element update — pure `Update` op + `c[i] <- x` rebind sugar

Date: 2026-07-18 · Status: accepted — **decided with Sapir (Session 13)**: Sapir ratified `notes/array-update-design.md` Option A and requested it now ("I need the dynamic array access"); surface form + scope details decided by the orchestrator under that ratification, revisable

## Context (what forced the decision; spec refs)

Core arrays are constructible only by literal / `map` / `zip` / `enumerate` — no element write. Consequence (matmul4, S12): loop-driven *construction* of an array is inexpressible; the i,j enumeration must unroll into N² named bindings. The read side is already dynamic (`a[i * 4 + k]` — `Index` takes a runtime operand). `notes/array-update-design.md` (S12) analyzed the options; Sapir ratified **Option A** (S13). Raw mutation stays off the table on vision grounds (one-definition rule ir I3, E2 determinism, fanout safety, E3 — the note's §"Why raw mutation is off the table"; same reason ADR-0013 omits the heap quartet).

## Decision (imperative)

1. **One new pure IR op** (ADR-0013 realized-set delta +1, the ADR-0018 precedent):

   ```
   Update : (Array{T, n} × I × T) → Array{T, n}
   ```

   Source is the 3-tuple operand aggregate (§5.1 convention). `I` is a Core integer scalar — **exactly `Index`'s typing** (ir DESIGN F6 deviation: no `Nat` in Core; a negative index is an OOB *trap*, not a type error). Result: a fresh array, slot `i` replaced. OOB (`i < 0 ∨ i ≥ n`) ⇒ `Trapped(IndexOob)` — the *same trap class* as `Index` (no new trap kind; "index_oob" covers read and write). Effect-free: no token, fanout-legal, legal in map/fold bodies. **Value semantics; fixed `n` stays in the type** — `Update` needs no dynamic arrays, no E3 reopen (the heap trigger remains dynamic arrays/`Vec`, a separate later ADR).

2. **Surface = rebind sugar only**: `IDENT '[' expr ']' '<-' expr ';'` — e.g. `c[t] <- v;`. Grammar-wise this **extends `bind-stmt`** (consolidation, FRAMEWORK §3: `BindStmt` gains a partial `index?` morphism — not a parallel statement kind). `<-` is already a lexeme (mut-init); no lexer change, and no parse-fork change either: `looks_like_bind`'s bracket-depth scan already classifies `c[i] <- x` as a bind statement.
   **Rebind-rule inheritance is NOT free** (S13 design review, 2 confirmed blockers): today's `StmtKind::Bind` emit path is a *fresh shadowing declaration* (`bind_new`), never a rebind, and the syntactic rebind scanners recognize assignments only as bare-`Var` chain stages, skipping every `Bind`. An indexed bind **is a rebind** and must be wired at three points: (a) emit: `index.is_some()` ⇒ emit `Update(cur, i, x)` then the real `rebind()` (never `bind_new`); (b) loop carried-set discovery (`collect_assigns_stmt`) records the indexed bind's target — else `mut c` is dropped from the merge and the motivating matmul silently miscompiles; (c) the Phi-arm scan (`scan_stmt`) records it — else L1408 does not fire for `c[i] <- x` in a Phi arm. With those three wired, the rebind rules apply as intended: non-mut target rejected, Phi-arm assignment illegal (L1408), loop-carried arrays route through merges like tuple state.
   Constraints (parse-time): the index form takes no `mut` keyword and no type annotation; nested index targets (`c[i][j] <- x`) are **rejected** for this increment (one P-code; ceiling recorded). No expression-position `update(...)` builtin — sugar only, per the ratified note; an expression form can be added later without a new op.

3. **Rewrite rows** (layer 3, const-index only, trap-conservative). Semantic laws: (L-a) `index ∘ update` at equal const in-bounds `i` = the written value; (L-b) at unequal const in-bounds `i, j` = `index` of the base; (L-c) `update ∘ update` at equal const in-bounds slot collapses to the outer write. **Implementation scope this increment: L-a only** — it aliases the `Index` result to the already-existing written-value object, which the plan+replay `alias` channel expresses. L-b and L-c require re-sourcing a *surviving* op's operand, a channel the RewritePlan does not have (S13 review: `alias`/`constify`/`drop`/`fuse` all rewrite results, never operands); they are recorded headroom with the sketched `reoperand : MorphismId → (slot, ObjectId)` channel (rewrite suggestions), not silently claimed. `Update` joins `Index`/`Div` in the *may-trap* class: DCE keeps it unless the index is provably in-bounds (same discipline as the existing table). R1 is untouched — the laws only fire where the trap cannot be observed differently.

4. **Backends**: llvm (this increment, P5): bounds-guard → `flow_trap(index_oob)`; copy the source array slot to the target slot (`llvm.memcpy`); GEP dynamic index + store the element. CUDA/Verilog: `planned` capability rows (Verilog: a `mut` array carried through a single loop = a RAM block with one write port — the E1/FSM extension, designed at P7). **In-place lowering (skip the copy when the source's last use is the `update` — the E3 §10 last-use frontier) is an *optimization*, out of this ADR**: naive copy is the correct semantics everywhere; the in-place deduction is recorded headroom per backend.

5. **testgen** gains `Update` generation (arrays + updates in the random-program pool) — differential duty (ADR-0020 §4) covers the op from birth.

## Non-goals (asked and answered)

- **Dynamic array sizes (`[T]`)**: out, unchanged. `[T]` exists in the full-surface grammar (user-guide §2 types; parsed, P0104-rejected as out-of-Core). Exclusion is load-bearing: ADR-0004/E3 proves the memory guarantee for the fixed-size first-order core (stack/static allocation + last-use frontier); dynamic `n` requires heap allocation and reopens that proof, plus Verilog E1 RAM sizing and `n`-in-type erasure. Reopening is its own ADR (Core+1, after coproducts per HANDOFF §4.2 ordering) — `Update` deliberately does not depend on it.
- **Struct field update** (`p.x <- v`): the symmetric `Proj` case; not requested, not designed. Open question if wanted.
- **`tabulate`** (note Option B): later, if/when closures land — it would *deduce* `Update` away (`iota`/ADR-0018 precedent); adopting A now does not block B.

## Consequences

matmul and every loop-driven array construction become expressible at fixed `n` (flatten i,j to one loop; even L1504 nesting limits are untouched). Cost honestly stated: naive `Update` is O(n) copy per write in every backend until the in-place headroom lands — correctness first, the optimization is a deduction not a semantics change. One op + one sugar row added to: ir §5.1 typing table + validate + Mermaid, lower (desugar + rebind wiring per §2 + diagnostics), interp (eval arm), rewrite (**replay `emit_op` arm + `reads_packed_source` in both replay.rs and graph_rewrites.rs** — the optimizer's exhaustive op match, missed by the first draft and caught in review — plus the L-a equation + testgen), backend capability matrix (new row), backend-llvm §2 op table. Each patched with its component's docs in the same change (§6.3).

## Spec impact (exact files/sections to patch; patched? yes/no)

None at Level A (realized-set delta, same class as ADR-0013/0015/0016/0018/0020). HANDOFF §4.1 Core-surface list gains the element-write sugar — recorded here (HANDOFF is a bootstrap doc superseded by ADRs per its own §0; not edited).
