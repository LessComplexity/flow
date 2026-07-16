# Design note — dynamic sizes, size-generics, and execution-graph deduction

**Status:** non-binding design exploration (VISION.md precedent: direction changes happen
by ADR, never by this file). · **Date:** 2026-07-16 · **Session 09** · Discussed with Sapir.
**Scope guard:** nothing here touches Flow-Core (HANDOFF §4) or the M5 path. The spec is
frozen; every "proposal" below is an ADR *candidate*, sequenced post-milestone.

Motivating artifacts: `examples/vector.flow` (the wanted form — size-generic `zip<A,B,N>`,
index-capturing map) vs `examples/vector_add.flow` (the Core-honest unrolled literal).

---

## 1. Dynamic sizes — the missing tier ladder

### What the corpus already contains (scattered, not unified)

| Fragment | Where | What it gives |
| --- | --- | --- |
| `[T]` "dynamic array / slice" | user-guide §2.1 | surface notation, `.len()`, `arr[mid]` (§8.5), head/tail destructuring loops (§3.5) |
| `Ty::Array { size: Option<usize> }` | category-ir §3.4 | **the IR type already admits unknown size** (`None`) |
| `Vec<T>`/`String`/`Buffer` heap types | arch §3.4, guide §6.2 | Alloc + last-use frontier + escape analysis already speced for reference types |
| `Stream<T>` endofunctor (+ `stream_map` fusion law) | category-ir §6.1.2, guide §9.3 | the sequential-access unbounded collection, used on FPGA (`Stream<RGB>`) |
| Channels, KPN semantics | E2/ADR-0003 | deterministic unbounded FIFOs, `ChannelSend` as escape sink |
| `@device/@shared/@bram` placement attrs | guide §9.2 | per-target memory placement in embryo |
| Bit-width-typed wire bundles | arch §4.3 (Clocked-Cat) | **the hard constraint: FPGA objects need static width** |
| "Dependent types (size-indexed arrays)" | arch §10.3 | size parameters already named as future work |

So "sizes don't exist" is true only of Flow-Core (deliberately). The v0.2 spec sketches
*three different* dynamic collections (`[T]`, `Vec<T>`, `Stream<T>`) with no unifying
semantics and no per-target realization story. That unification is the actual design task.

### The proposal shape: a four-tier size ladder

Semantically, a sized value is a **dependent pair** `(n, data)`; the tiers differ only in
what is known about `n` at compile time, and each backend functor chooses a realization.

| Tier | Type | `n` known | Categorically | CPU | GPU | FPGA |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | `[T; N]`, N literal | compile time | plain object (today's Core) | ✅ | ✅ | ✅ |
| A | `[T; N]`, N a **parameter** | at instantiation | N-indexed *family* of objects; functions are families of morphisms natural in the element types (§7.1's polymorphism-as-naturality, extended with a ℕ index) | ✅ | ✅ | ✅ |
| B | `[T; ≤N]` bounded-dynamic | runtime, capacity static | **finite coproduct** `Σ_{n≤N} [T;n]` ≅ `(len, [T;N])` — lands exactly in Core+1's coproduct machinery (`Option`/`Result` are its degenerate cases) | ✅ | ✅ (buffer + `n` bound — emitted kernels already carry `int n`) | ✅ (capacity-N BRAM + occupancy register: width static, Clocked-Cat satisfied) |
| C | `Vec<T>` (random access) / `Stream<T>` (sequential) | runtime, unbounded | `Vec T = Σ_{n:ℕ} [T;n]` (infinite coproduct → heap); `Stream T` = coinductive / free-monoid-flavored, size never materialized | ✅ / ✅ | ✅ / ✅ | ✋ reject / ✅ (valid/last handshake; KPN→SDF per E2) |

Key resolutions this buys:

- **The "growing size on every target" intuition resolves as a split, not one type.** On
  FPGA, "growing" is *time-multiplexing, not space*: the stream is how hardware does
  dynamic size. `Vec<T>` (space-dynamic) and `Stream<T>` (time-dynamic) are genuinely two
  objects — a §3 reduction would show their morphism sets differ (random access vs
  sequential access), so they stay two. Strings: `String` = `Vec<u8>` on CPU, `[u8; ≤N]`
  in Tier B contexts, `Stream<u8>` on wires.
- **The executor analogy made precise.** Executors realize the graph's *parallelism*
  structure (schedule/placement) without changing meaning; a **size/memory discipline**
  realizes its *extent* structure (allocation/materialization) without changing meaning.
  Same pattern as `executor` declarations (guide §5.3) and the `@device/@shared/@bram`
  attrs (§9.2): semantics fixed by the tier, realization chosen per target — heap
  ptr+len (F_LLVM), device buffer+n (F_CUDA), BRAM+occupancy or stream (F_Verilog).
- **Functoriality survives untouched.** A backend that cannot realize a tier is a
  *partial functor* on it — and "rejection is the partiality of the lowering functor" is
  already this project's principled stance (categorical-model §7.3). The capability
  matrix (HANDOFF §4.3) is the enforcement vehicle: F_Verilog total on Tiers 0/A/B and
  C-Stream (SDF-restricted, bounded buffers), rejects C-Vec with a diagnostic.
- **E3 (memory guarantee) survives through Tier B**: allocation stays fixed-size
  (capacity), the last-use frontier is unchanged. Tier C-Vec re-opens the E3 "open for
  the full language" clause — contain it there.
- **E1 interaction:** a Tier-B/C-consuming loop has a runtime trip count — exactly what
  the guarded trace / done-protocol already handles; no new mechanism.

## 2. zip / map-with-index — natural transformations first, capture as sugar

Core today: `map` is unary, its body closed (L1108: no capture), no `zip` → hence
vector_add.flow's unrolled literal. The wanted `[0..N] -> map { i -> (a[i], b[i]) }`
decomposes into two features of very different cost:

**2a. The cheap 90%: three primitive natural transformations (no capture, no closures).**

| Primitive | Signature | Naturality (free optimization) | F_Verilog | F_CUDA |
| --- | --- | --- | --- | --- |
| `zip` | `[A;N] × [B;N] → [(A,B);N]` | `zip ∘ (map f × map g) = map (f×g) ∘ zip` | wire re-bundling — **free** | fuses into consumer kernel — free after layer-1/2 |
| `enumerate` | `[A;N] → [(nat,A);N]` | slides past `map` by naturality | counter wire | `threadIdx`-derived — free |
| `iota<N>` | `1 → [nat;N]` | constant | constant wires | `threadIdx` itself |

These are exactly the §7.2 catalogue extended (each row a polymorphic family whose
naturality square is a layer-2 rewrite for free — guide Appendix A already *plans*
`zip`/`enumerate`/`chunk` as stdlib). Note `zip` is the canonical distribution of the
product over the N-indexed product: arrays are `A^N`, and `A^N × B^N ≅ (A×B)^N` — an
*isomorphism*, which is why every backend gets it for free. With just `zip`:

```flow
fn vec16_add(a: [i32; 16], b: [i32; 16]) -> [i32; 16] {
    (a, b) -> zip -> map { (x, y) -> x + y } -> ret;   // replaces 16 unrolled literals
}
```

— already legal *shape* (tuple-destructuring map bodies exist, guide §8.4); only the op
is missing. This needs **no coproducts, no generics, no capture** — a small standalone
ADR: one IR op (+§5.1 typing row + builder/validate/interp/dump), or even lowering-level
desugar. Sequencing: it is not needed for M5 (sepia is unary-map-shaped), so it parks
until the first post-M5 scope ADR — but it should be the *first* one.

**2b. The general 10%: captures — desugar to broadcast + zip, don't add closures.**
A map body capturing `a` is categorically the functor's *strength*: body `C × T → U`,
and `map_c(body) = array_map(body) ∘ zip ∘ (broadcast_C × id)`. I.e. **capture reduces
to zip with a constant-broadcast lane** — so the IR never needs closure semantics: the
captured array becomes a *real in-edge* to the Map node. That is FRAMEWORK Law 1 kept
honest (a body reading `a` that never arrived at the body's location is a data
teleport — today's L1108 ban exists precisely because of this; the broadcast edge is
the correct fix, not an exception). GPU realization: extra kernel argument. FPGA:
shared port / fanout wires. Gather patterns (`a[i]`, `a[i-1]` stencils) then come from
`enumerate`+capture — and for the project's actual domain (image/signal), the
domain-honest primitive is **`window`/`chunk`** (line buffers on FPGA, shared-memory
tiles on GPU); Appendix A already lists `chunk`.

**2c. Size-generics `fn zip<A,B,N>` (Tier A) — monomorphization.**
The spec's account of polymorphism is semantic (naturality, §7.1) with **no realization
strategy specified** — the realization ADR should pick **monomorphization** (each used
`(A,B,N)` stamped to a concrete instance): backends then see exactly today's static IR;
Clocked-Cat's static widths are satisfied; no dictionaries at runtime. vector.flow's
`zip` then becomes *writable in-language* instead of primitive — the primitive from 2a
becomes its seed/reference instance.

## 3. Execution-graph deduction — merge sort worked through

The principle first: **the execution graph is never annotated; it is deduced by three
mechanisms**, and merge sort exercises all three.

1. **Compile-time unfolding (monomorphization/inlining) turns size-static structure into
   a DAG.** With Tier-A generics, `msort<N>` calls `msort<N/2>` twice + `merge<N>`.
   Template-level recursion **does not violate the acyclic-call-graph rule**: N strictly
   decreases, so each instantiation is a distinct function and the per-monomorphized
   program's call graph is acyclic — bounded compile-time recursion, zero runtime
   recursion. The unfolded graph *is* the recursion tree: a balanced binary merge tree.
   Sibling subtrees have disjoint successor sets → parallel **by inspection** (§9.5 /
   guide §4.5 — same deduction as fibonacci's two independent calls, §8.1). Work and
   span are read off the graph: work = node count, span = longest path — and arch §4.3
   computes FPGA pipeline depth as *the same* longest-path invariant. One graph, one
   cost model, three targets.

2. **Data-dependent iteration is a trace — a finite cyclic graph whose runtime unrolling
   is the actual execution DAG.** The *merge* step is inherently data-dependent (each
   output depends on a comparison), so in pure dataflow it is a `loop`: carried state
   `(i, j, out)`, guard-first per ADR-0016, done-protocol on FPGA (E1). For dynamic N
   (Tier B/C) the whole sort flips to **bottom-up mergesort**: an outer trace over pass
   width `w = 1, 2, 4, …` whose body is one *data-parallel pass* (a map over run-pairs,
   each merged by the inner sequential loop). Static IR: one SCC wrapping a wide
   parallel stage. Runtime: the trace unrolls into the DAG the machine executes — the
   IR graph is that DAG's finite quotient. This is exactly how GPU mergesort is written
   by hand; here it is the *shape the language forces*.

3. **The hardware-canonical escape from data dependence: sorting networks.** For static
   N, replace data-dependent merges with a **Batcher bitonic / odd-even merge network**:
   compare-exchange is a pure morphism `(a,b) → (min(a,b), max(a,b))` (2-in/2-out via
   Pair/Proj — no control flow at all), wired in the Batcher pattern by Tier-A
   unfolding. The result is a *completely static* DAG, depth O(log²N): FPGA = one
   pipeline column per stage; GPU = one map per stage; CPU = branchless SIMD (guide
   §10.3's own "prefer branchless" advice). `sort<N>` as a network is the purest
   expression of the thesis: the graph *is* the algorithm, and the three backends are
   three functorial readings of the same tree.

4. **Streaming (post-M5): merge is a textbook Kahn process.** Two input channels, one
   output, blocking reads — determinism is Kahn's theorem, which E2 already bought. A
   tree of merge processes is streaming mergesort; on FPGA, merge-FIFO trees under SDF
   restrictions. No new semantics needed — this falls out of the channel decision.

## 4. ADR candidates (sequenced; none actionable pre-M5)

1. **`zip`/`enumerate`/`iota` primitives** (§2a) — first post-M5 scope ADR; independent
   of coproducts and generics; kills the vector_add unroll.
2. **Size-generics via monomorphization (Tier A)** (§2c) — unlocks in-language `zip`,
   sorting networks, static recursion trees. Pairs with the recursion (Core+1) ADR: the
   acyclicity rule is *per-monomorphized-instance*.
3. **Capture-as-broadcast desugar** (§2b) — rides on 1; keeps IR bodies closed and
   Law 1 honest. `window`/`chunk` for the image/signal domain follows.
4. **Bounded-dynamic `[T; ≤N]` (Tier B)** — rides the Core+1 coproduct wave; the one
   dynamic tier that is total on all three backends.
5. **`Vec<T>` vs `Stream<T>` split (Tier C) + per-backend partiality rows** in the
   capability matrix; strings ride this. Channels/KPN (already planned) are the
   Stream half's semantics.

Each parked in `docs/suggestions.md` spirit: recorded, rule-cited, not started.
