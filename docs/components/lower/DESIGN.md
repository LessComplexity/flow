# Component: lower — DESIGN

(Not yet designed — the binding DESIGN is written in the session that first codes this
component, per HANDOFF §7.1.5. This file currently holds **pre-design notes only**.)

## §0.1 Pre-design pins from the IR increment (recorded Session 04, 2026-06-12)

These five rules were pinned during the flow-ir design review (ADR-0013; ir/DESIGN §7–§10)
because leaving them "lowering's choice" would let two lower authors produce different
graphs for the same program. They are obligations on lower, decided already:

1. **Effect signature synthesis (law, ADR-0013):** a function containing `print` (or
   calling an effectful fn) declares token-threaded: surface `A → B` ⇒ IR
   `(IoToken × A) → (IoToken × B)`, degenerating to `IoToken → IoToken`; surface
   `fn main()` declares as `main : IoToken → IoToken`, `input()` is the seed token, and
   the final token is written to Return. Tokens never die (I4b); loop-carried tokens exit
   via every `LoopExit` of that merge.
2. **Canonical ret-write (ir/DESIGN §10):** the producing primitive targets Return via
   `Dest::Ret`; `output()` only for bare `x -> ret` / `x -> ret.k` of pre-existing
   objects. Never `Fresh` + `output()` where `Dest::Ret` suffices.
3. **Negative literals fold at lower time:** `Unary(Neg, <literal>)` becomes one negated
   `Constant` object; `Neg` morphisms are emitted only for non-constant operands (the
   IEEE `fneg` case ADR-0013 keeps `Neg` for).
4. **Value-match guards lower as a right-folded Phi chain:** for arms `-k_i-> e_i` with
   default `-_-> e_d`: `cond_i = Eq(scrutinee, k_i)`; chain = `phi(e_1, phi(e_2, …
   phi(e_n, e_d, cond_n) …, cond_2), cond_1)` — arm order preserved, default innermost.
   (3-way golden pinned in ir/DESIGN §16.)
5. **Loop exit values read the merge-state view** of the iteration in which the guard
   fails (Proj of the LoopMerge or pre-update derivations), never the recomputed next
   state; back edges carry the recomputed state. `sum_to_n(10)` exits 55 — the contract
   test. Both routes share the loop guard's single `cond` object in the canonical form.

## §0 Pre-design notes: parse-tree obligations (recorded Session 03, 2026-06-12)

Provenance: extracted from `category-ir.md` §3/§4 (+§2/§5/§10/§11) and `ERRATA.md` by an
Opus reader during the Session 03 parser design, then consumed by the parser's
adversarial design review (finding: guard arms lower to **Phi or Trace routing**, §4.4 vs
§4.5 — do not feed loop-guard arms to the Phi rule). Kept here so the lower increment
starts from the same obligations the parse tree was designed against. **Not binding** —
re-verify against the spec when writing the real DESIGN; the authoritative tree shape is
`docs/components/syntax/DESIGN.md` §15 (note `Expr::Hole`: exactly one per
`StageKind::OpShorthand`, leftmost leaf = the piped value as left operand). Out-of-Core
surface never reaches lower: the parser rejects it with P-codes (syntax DESIGN §16), so
the obligations below cover Core forms only. ADR-0012 (labeled blocks `:label`) is
Core+1; lower sees only the `loop` keyword form through M5.

## 1. PARSE-TREE OBLIGATIONS

Each entry: construct → what the tree must preserve → why (the consuming lowering rule) → spec ref.

1. **Binary operations — operands kept as a left/right pair, operator identity kept.**
The tree must carry an operator node with **exactly two ordered operand subtrees** (`lhs`, `rhs`) and the operator tag. Lowering (§4.1; §11.1 `ParseNode::BinOp { op, lhs, rhs }`) does: lower `lhs`, lower `rhs`, emit a `Pair` morphism from the env object to an `i32 × i32` temporary, then emit the primitive (`Add`, etc.). The parse tree must NOT pre-flatten `a + b` into a single multi-source node — the IR invariant is "**Every morphism has exactly one source object and exactly one target object**" (§3.1), realized as "a product-pair followed by the primitive operation" (§3.1, §2.2). Operand **order is load-bearing** for non-commutative ops (`Sub`, `Div`, `Mod`, `Lt`, `Gt`, `Le`, `Ge` in the `Operation` enum, §3.3). Ref: §4.1, §3.1, §11.1.

2. **Shorthand pipeline stages — the implicit-left-operand distinction must be preserved.**
A stage like `+ 5` or `* 2` (chained pipeline, §4.3; user-guide §3.3) is "syntactic sugar for `⟨·, 5⟩ ; add`... they take the piped value as the left operand and the literal as the right" (user-guide §3.3, verbatim). The tree must record (a) that this stage has an *implicit* left operand (the incoming wire) rather than two explicit operands, (b) the operator, and (c) the literal as the **right** operand. Lowering pairs the previous intermediate object with the constant and applies the primitive (§4.3 worked example: `data * 2 -> + 5 -> * 3 -> ret`). The left/right asymmetry matters because the piped value is *always* the left operand. Ref: §4.3, user-guide §3.3.

3. **Pipelines — chains kept as ordered sequences; stages are NOT semantically grouped.**
The tree must preserve the **order** of `->` stages but must NOT impose parenthesization/grouping. "Because composition in a category is associative, the IR does not record 'stages' — it records a flat sequence of morphisms that can be grouped arbitrarily for codegen" (§4.3, verbatim) and "The graph above denotes a single morphism `A → D` regardless of how you parenthesize the chain. This is why pipeline syntax `a -> f -> g -> h` is unambiguous" (§2.1.2, verbatim). So the parse tree needs an ordered list of stages; it must not commit to a nesting that lowering would have to undo. (See §3 obligation below — lowering wants the chain as a flat ordered chain.) Ref: §4.3, §2.1.2.

4. **Flow direction `->` vs `<-` — must be normalized but the binding/assignment distinction preserved.**
Both `result <- a + b` (§4.1 source) and `a + b -> result` denote the same composition. The tree must capture **source and destination** unambiguously regardless of which arrow was written. Critically, per Erratum E4 (ERRATA E4; user-guide §3.6): "**a flow is a statement, not a value-producing expression**; `->`/`<-` chains are parsed at statement level." The parser MUST parse flows at statement level, not as value-producing expressions — a flow cannot appear as an operand. Ref: ERRATA E4, user-guide §3.6/§3.2, §4.1.

5. **`ret` keyword — must be preserved as the distinguished return target, not an ordinary variable.**
"The `ret` keyword names the return object. Every morphism that writes to `ret` contributes to the function's output" (user-guide §3.2, verbatim). The tree must mark writes to `ret` distinctly so lowering produces an `Object` of `ObjectKind::Return` (§3.2 `ObjectKind` enum). Multiple writes to `ret` are legal and all contribute. Ref: §3.2 (`ObjectKind::Return`), user-guide §3.2.

6. **Tuple-indexed return targets `ret.0`, `ret.1` — index must be preserved.**
`a / b -> ret.0; a % b -> ret.1;` (user-guide §3.2, multiple-return) requires the tree to carry the **projection index** on the return target. Lowering builds a tuple-typed `Return` object; each indexed write feeds a specific component. Ref: user-guide §3.2.

7. **Variable bindings — name, mutability flag, optional type annotation, and `mut` distinction.**
The tree must carry, for `x: i32 <- 5`, `mut y: i32 <- 10`, and `value <- 42`: the binding name, whether `mut` was present, and the optional type annotation (`Ty` may be inferred when absent). Mutability is load-bearing: "Mutation — only permitted on `mut` bindings" and `x + 1 -> x` on a non-`mut` `x` is "ERROR" (user-guide §3.1). The lowering/checker needs the `mut` flag to validate re-assignment. Bindings map to `Object`s whose `kind` is `Parameter`/`Temporary` and whose `ty` is the annotation (§3.2, §3.4). Ref: user-guide §3.1, §3.2 (`Object.ty`, `ObjectKind`).

8. **Constants/literals — value must be preserved for `Const` and for `Pair`-with-constant metadata.**
Integer/float/bool/char literals become `Object`s with `kind: Constant` and `value: Some(Value)` (§3.2), or are folded into a `Pair` morphism's metadata. "The Pair operation's metadata records *which projections* of the ambient environment to bundle" and in the worked example the constant `2`/`5` ride along with the `Pair` (§4.1; Appendix B morphism table: "Pair (with constant 2)"). The serialization shows the constant inline: `"op": {"Mul": {"rhs_const": 2}}` (§5.3). So the tree must keep literal values verbatim (not just spans). Note string literals are Core-restricted — see §2 below. Ref: §3.2, §4.1, §5.3, Appendix B.

9. **Guard/conditional blocks — guard arms must be kept ORDERED and each arm's discriminant + body preserved.**
A conditional `cond -> { -true-> ...; -false-> ...; }` (§4.4; user-guide §3.4) lowers via `Phi` for pure branches. The tree must carry: the condition subtree, and an **ordered list of arms**, each with its discriminant (`true`/`false`/integer/`_`) and its body subtree. Lowering (§4.4, §11.1 `ParseNode::If { cond, then_b, else_b }`) builds the condition morphism `→ Bool`, lowers *both* branches, forms the `i32 × i32 × Bool` triple object, and emits `Phi`. "both branches are *always computed*" for pure morphisms (§4.4). The arm bodies map to the two `T` inputs of `Phi`'s `T × T × Bool` source (§3.3 `Phi`), so the tree must keep which arm is the true-case vs false-case. Ref: §4.4, §3.3, §11.1, user-guide §3.4.

10. **Value-match guards — discriminant *values* and the default `-_->` must be preserved and ordered.**
`status_code -> { -0-> ...; -1-> ...; -2-> ...; -_-> ...; }` (user-guide §3.4). The tree must carry each integer discriminant value and flag the wildcard/default arm `-_->`. ADR-0010 fixes that a Core guard arrow is a **single lexeme** `-D->` with `D ∈ { true, false, _, [0-9]+ }`; the parser consumes `Guard` tokens (not `Minus Int Arrow`) and must preserve `D`. Over-`u64` discriminants clamp to `u64::MAX` (ADR-0010). The parser "must report stray `Guard` tokens outside guard blocks with an 'add a space' hint" (ADR-0010). Ref: user-guide §3.4, ADR-0010.

11. **Guard arms with an implicit-input body (`-false-> -> ret;`) — the bare-arrow continuation must be representable.**
In §4.5 and user-guide §3.5, an arm body can be a bare flow `-> ret;` or `-> loop;` with no explicit source (the source is the guard's incoming value). The tree must distinguish an arm whose body is *just a flow to a target* from one with a computed body. Lowering routes this to the exit edge / back edge. Ref: §4.5, user-guide §3.5.

12. **Loops — must be a distinct node carrying a (optional) label and a body; back-edge and exit must be derivable.**
`loop { ... }` (§4.5; user-guide §3.5) lowers to `Trace`. The tree needs a `Loop { label?, body }` node (§11.1 `ParseNode::Loop { body }`). Lowering (§11.1) creates a `new_loop_merge_object()` (a `LoopMerge`-kind object, the `U` in `Tr^U(f)`, §3.2/§3.3 `Trace { body, carried }`), lowers the body against that merge object, and adds the `Trace` morphism. The **loop label** (e.g. `search`, `outer`, `inner` — user-guide §3.5, §8.5) is the jump target name and MUST be preserved. Ref: §4.5, §3.3, §3.2, §11.1, user-guide §3.5.

13. **Loop control edges — continue (`-> loop;`) vs exit (`-> ret;`) must be DISTINGUISHED, and target the named label.**
"The `-> loop;` edge is the back-edge in the graph; `-> ret;` is the exit edge" (user-guide §3.5, verbatim). "`route -. 'true-case' .-> merge` ... `route -- 'false-case' --> ret`" (§4.5). The tree must record, for each control flow, **which label it targets** (`loop`, `search`, `outer`, `inner`, or `ret`), because nested loops use distinct labels: `-> inner;` (continue inner) vs `-> outer;` (break inner, restart outer) (user-guide §3.5 nested-loops). Lowering glues the "keep looping" output back to the `LoopMerge` (back edge) and the exit output to the result (§4.5: "the trace operator glues the 'keep looping' output back to `i_loop`"). The back edge "is not a special field on any morphism. ... a real edge in the adjacency list" (§4.5, §5.2) — but the parse tree must still mark *which* arm continues and *which* exits so lowering knows where to draw the edge. Ref: user-guide §3.5, §4.5, §5.2.

14. **Loop-carried state updates — assignments to `mut` loop vars must be preserved in body order.**
In `-true-> { i + 1 -> i; -> loop; }` (§4.5) and the array-sum body `total + head -> total; tail -> items; -> loop;` (user-guide §3.5/§8.2), the writes to loop variables (`i`, `total`, `items`) define the `next_state` half of `body : (input, state) ↦ (output, next_state)` (§2.7). Flow-Core restricts carried state to "scalar/tuple carried state" (ADR-0001). The tree must preserve these state-update assignments and their order so lowering can construct the `U`-typed back edge. Ref: §4.5, §2.7, ADR-0001.

15. **Fanout blocks — branches kept as an unordered-but-enumerated set, with the implicit join point marked.**
`data -> { -> process1 -> r1; -> process2 -> r2; -> process3 -> r3; }` (user-guide §3.3) is a product/parallel fanout. The tree must carry each branch (each beginning with a bare `->` taking the fanned-out value as source) and the fact that there is an **implicit join at the closing brace**. "The implicit join at the closing brace waits for all branches to complete" (user-guide §3.3). Lowering: branches have disjoint successor sets and become bifunctor-product images (§4.5 visual; §9.5 "if the two morphisms appear in the image of a bifunctor `(f × g)` ... independent ... The IR records which morphisms came from such bifunctor images"). For the memory model the join point IS the free-frontier ("the join point of a fanout *is* the frontier synchronization point", §10), so the tree must make the block boundary recoverable. Ref: user-guide §3.3/§4.5, §9.5, §10.

16. **`seq` blocks — the sequencing keyword must be preserved as a distinct fanout flavor.**
`data -> seq { ... }` (user-guide §5.2) forces sequential execution. The tree must distinguish a `seq`-block from a plain fanout block, because effectful branches (`print`) are "**Not permitted in parallel fanout** — must `seq`" (user-guide §5.4; ERRATA E2). The effect checker (built per E2) depends on knowing a fanout is `seq`-wrapped. Ref: user-guide §5.2/§5.4, ERRATA E2.

17. **`void` blocks — the discard keyword must be preserved as a distinct fanout flavor.**
`data -> void { ... }` introduces "a fanout whose results are discarded ... for side-effects-only branches" (user-guide §3.3). The tree must mark `void` distinctly; lowering maps discarded results to the terminal object `1`/`drop` (§2.4 "Terminal object `1` ... This is `drop` or 'discard the value.'"). Ref: user-guide §3.3, §2.4.

18. **Function definitions — name, ordered parameters (name+type), return type, body.**
`fn name(p1: T1, p2: T2) -> R { ... }` (user-guide §3.2). The tree must carry the function name, the **ordered** parameter list with names and types, the return type, and the body. Parameters become `ObjectKind::Parameter` objects, and multiple params are conceptually one product input ("Flow functions conceptually take one input — a product object when the function has multiple parameters", user-guide §3.2). Flow-Core requires functions be **non-recursive with an acyclic call graph** (ADR-0001) — the parser need not enforce acyclicity (a later pass does) but must preserve call names so that check is possible. Ref: user-guide §3.2, §3.2 (`ObjectKind::Parameter`), ADR-0001.

19. **Function calls — three call syntaxes must each be representable, preserving argument structure/order.**
"three syntaxes, all equivalent" (user-guide §3.2): (a) tuple input `(15, 20) -> add`; (b) named-parameter partial application `15 -> add.a; 20 -> add.b;`; (c) pipeline single input `data -> process`. The tree must preserve which form was used and the argument(s): for (a) the **ordered tuple**; for (b) the **parameter name** (`.a`, `.b`) each argument binds to; for (c) the single piped value. Lowering produces `Call(FunctionId)` (sugar for `Apply` after `curry`, §3.3) with the source being the product of arguments. The tuple-input order corresponds positionally to parameters (cf. `(v, lo, hi) -> clamp`, ERRATA LC-2). Ref: user-guide §3.2, §3.3 (`Call`, `Apply`).

20. **Member access — `.field` must be preserved with the field name; index must precede `->` per precedence.**
`x -> f.method` parses as `x -> (f.method)` (user-guide §3.6), and `px.r`, `px.g`, `px.b` (user-guide §8.3 sepia) access struct fields. Member access `.` binds tighter than everything except grouping (precedence rank 2, user-guide §3.6). The tree must carry the base subtree and the field symbol. Lowering uses `Proj(u8)` (π_i projection, §3.3) for tuple/struct field access into a named product (`Struct { name, fields }`, §3.4). Ref: user-guide §3.6/§8.3, §3.3, §3.4.

21. **Array indexing — base and index subtrees preserved; bounds-check obligation noted.**
`arr[5]` / `arr[mid]` (§4.2; user-guide §8.5). The tree carries base + index expression. Lowering pairs `arr` with the index and applies `index` (§4.2). Bounds-checking "lifts this into `Kleisli(Result)`" (§4.2) — i.e., the index morphism's target becomes `Result<T, IndexError>`. Flow-Core indexing is "bounds-checked" (ADR-0001). Ref: §4.2, ADR-0001.

22. **Operator precedence — the tree must reflect the §3.6 precedence so lowering sees correct grouping.**
Precedence (tightest→loosest, user-guide §3.6): `()` > `.` > `* / %` > `+ -` > comparisons > `&&` > `||` > `-> <-` > `?` > `;`. Per ERRATA E4, `a -> b + c -> d` ≡ `a -> (b + c) -> d` and `a + b -> c` ≡ `(a + b) -> c`. The parse tree must encode these groupings (the recursive-descent grammar enforces them); this is what guarantees the `BinOp`/pipeline lowering receives correctly-nested operands. Note `?` is rank 9 (looser than `->`) but is **out of Core** (see §2). Ref: user-guide §3.6, ERRATA E4.

23. **Struct/product construction — type name and field bindings preserved.**
`RGB { r, g, b }` (user-guide §8.3) and `type Point { x: f32, y: f32 }`, `type Color {...}` (user-guide §2.1). Construction maps to a named product `Struct { name, fields }` (§3.4); the tree must carry the type name and the field-name→value map. Field-init shorthand (`RGB { r, g, b }` where `r,g,b` are in-scope vars) must be representable. Ref: user-guide §2.1/§8.3, §3.4.

24. **Named product type declarations — `type Name { field: T, ... }` preserved (Core), enum-form rejected (see §2).**
After E5, the keyword is `type` (ERRATA E5; user-guide §2.1). Flow-Core allows **product** (struct-like) `type` declarations only; "any `category`/`type` declaration beyond product types" is out of scope (ADR-0001). The parser must accept the struct-like form and capture field names+types (→ `Ty::Struct`, §3.4), while rejecting the enum-like (coproduct) form. The keyword `category` "may be reserved-and-rejected with a helpful error" (ERRATA E5). Ref: user-guide §2.1, ERRATA E5, ADR-0001, §3.4.

25. **`map` / `fold` collection operators — postfix block with positional parameters; block is NOT an argument.**
Per ERRATA LC-2 / ADR-0009 (the "collection-operator law"): "data arrives through the wire; the inline block is **postfix operator syntax, never an argument**; the operator's input tuple corresponds positionally to the block's parameters." Canonical forms: `array -> map { item -> ... }` (array ↔ item) and `(init, array) -> fold { acc, item -> ... }` (init ↔ acc; array ↔ item). The tree must represent the block as a **postfix operator on the operator node**, NOT as a call argument, and must preserve the **ordered block parameter list** (`item`; `acc, item`) for positional correspondence with the input tuple. The block body is "**not a first-class value**" (ERRATA LC-2). Lowering is `Pair(init, array)` then the fold/map primitive (ERRATA LC-2 cites category-ir §4). Note: the earlier `fold(0, {...})` call-position form is **explicitly wrong** and patched out. Flow-Core restricts these to fixed-size arrays with inline non-first-class block bodies (ADR-0001). Ref: ERRATA LC-2, ADR-0009, ADR-0001.

26. **Source spans on every node — required for diagnostics and for `SourceLoc`/`loc` fields throughout the IR.**
Every `Object` and `Morphism` carries `loc: SourceLoc` (§3.2). The lowering signature `fn lower(pt: ParseNode, ...)` propagates location into each `add_morphism`/`new_object`. Every parse node therefore must carry a span. This is also mandatory for the "reject-with-reason" diagnostics that must "name the construct and that it is post-Core" (ADR-0001) and for the guard-arrow "add a space" hints (ADR-0010). Ref: §3.2, §11.1, ADR-0001, ADR-0010.

27. **Effectful-branch distinction is NOT a parse obligation but the effect surface (`print`) must be preserved.**
Lowering chooses Phi (§4.4, pure) vs honest coproduct split/copair (§4.6, effectful) "When a branch contains side effects". This decision is made by the **type/effect system** ("The type system tracks effects (`IO<T>` rather than `T`) to force this lowering when needed", §4.6), not the parser. The parser need not classify effects, but must preserve calls to `print` (Flow-Core's "only effect, sequential-context-only", ADR-0001) so the effect checker can run. Ref: §4.6, §4.4, ADR-0001.

---

## 3. WHAT §4 (AND ADJACENT SECTIONS) SAY ABOUT PARSE-TREE SHAPE

Specific shape constraints the lowering rules impose, stated factually:

1. **Lowering wants flow chains kept as FLAT ORDERED chains, NOT pre-grouped.** §4.3: "the IR does not record 'stages' — it records a flat sequence of morphisms that can be grouped arbitrarily for codegen." §2.1.2: associativity makes `a -> f -> g -> h` "unambiguous" regardless of parenthesization. Implication: the parse tree should hand lowering an ordered stage list; it must not encode a binding/nesting of `->` that lowering would have to flatten. (Contrast: arithmetic operands DO need explicit lhs/rhs nesting per precedence — obligation 1.)

2. **Lowering wants guard arms ORDERED and the true/false (or value) cases positionally identified.** §11.1 destructures `If { cond, then_b, else_b }` — the then/else slots are positional. §4.4's `Phi` source is `T_true × T_false × Bool` (the §3.3 `Phi` op: `T × T × Bool → T`). The tree must keep arm order / which arm is which discriminant so the two `T` inputs land in the right product slots. For value-match (`-0->`, `-1->`, `-_->`) the discriminant tags + ordering + default position must be preserved (§3.3 `Copair`/`Inject(u8)` carry an index; though full coproduct lowering is Core+1, Core value-match still lowers via Phi-chains and needs ordered discriminants).

3. **Lowering wants loop continue/exit arms DISTINGUISHED, and the loop merge object is created BEFORE the body is lowered.** §11.1 `ParseNode::Loop { body }`: `let merge = b.new_loop_merge_object(); let body_ir = lower(body, b, merge);` — the body is lowered *against* the merge object as its environment. §4.5: the continue path ("true-case") routes back to `i_loop` (`LoopMerge`), the exit path ("false-case") routes to `ret`. The tree must therefore mark, per control arm, whether it is a back-edge (`-> loop;`/`-> <label>;` to the loop's own label) or an exit (`-> ret;` or to an outer label). The merge object "has *two* incoming edges — the initial value `i₀` and the back edge" (§4.5), so the tree must let lowering identify both the loop entry (initial state) and the back-edge state-updates. Nested loops require **label-resolution** in the tree (which label a `-> X;` targets — §4.5 / user-guide §3.5 `-> inner;` vs `-> outer;`).

4. **Lowering wants binary/multi-arg ops as Pair-then-primitive — so the tree must keep operands SEPARATE and ORDERED, never as a fused n-ary node.** §3.1 core invariant + §4.1: "a product-pair followed by the primitive operation." §11.1 `BinOp { op, lhs, rhs }` lowers to `Pair` then `op.into()`. The tree's binary node = operator tag + two ordered children. The `Pair`'s metadata "records *which projections* of the ambient environment to bundle" (§4.1) — derivable from the operand subtrees, so the tree must preserve enough to identify each operand as either an environment projection (variable) or a constant.

5. **Lowering wants fanout branches as bifunctor-product images — the tree must keep branches enumerable and the block boundary (join) recoverable.** §9.5: "The IR records which morphisms came from such bifunctor images; those are parallelizable without a dataflow analysis." §10: the fanout join point IS the lifetime free-frontier. So the parse tree must delimit the fanout block (its branches and its closing-brace join), and keep `seq`/`void`/plain flavor (obligations 15–17), because that flavor changes both parallelism and effect-legality (E2).

6. **Lowering wants `map`/`fold` as Pair-then-primitive with the block as postfix operator metadata.** ERRATA LC-2: "Lowering is unchanged: `Pair(init, array)` then the fold primitive (category-ir §4)." So the tree shape for `(init, array) -> fold { acc, item -> body }` is: an operator node whose source is the tuple `(init, array)` and whose **block body + ordered block params** ride as operator metadata — NOT a call node with the block in argument position. The positional correspondence `(init↔acc, array↔item)` must be encoded so lowering binds them.

7. **The tree is thin by design — no typed-AST obligations.** §1.3: "The parse tree is deliberately thin — there is no separate typed AST phase. Type checking, lifetime analysis, and optimization all run on the graph directly" and "the parser does produce a small tree that the lowering code pattern-matches on." Implication: the tree carries syntactic structure + names + spans + literal values + mutability/keyword flags only; it must NOT attempt to resolve types, infer `Ty`, classify effects, or compute `FunctionId`/`ObjectId`/projection indices — those are assigned during lowering (`IRBuilder`) and later passes (§11.1–§11.2). Types on bindings are *annotations to carry forward*, not resolved types.

8. **No identity morphisms / no implicit `drop`s need representing.** §2.1.1: "the IR builder never emits them [identity edges] explicitly — they are implicit." The parse tree need not (and should not) materialize identity composition or implicit env-threading; lowering threads the environment object (`env`/`Γ`) itself (§11.1 passes `env: ObjectId`).

---
