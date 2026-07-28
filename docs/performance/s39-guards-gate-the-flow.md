# S39 performance — guards gate the flow

Date: 2026-07-28 · session log: `../sessions/2026-07-28-s39-guards-gate-the-flow.md`
Change under test: `plan-s39-guards-are-conditional` — an arm that is not taken does not run.
PRE = `main@8b40442` · POST = the S39 working tree.
(`8b40442` is `24f52c9` plus one commit that only renames `matmul4096.mapal` into `examples/` —
0 lines changed, nothing under `crates/`, so the two compile identically.)
Raw data: `benches/results-s39/` (13 files: 12 timing series + `machine.txt`).

> **Gate state when these numbers were taken: 992 passed, 2 failed.** The two failures are one S39
> defect (gating is not stable across `LiftLoops`, session log §4a) reachable only from directly-built
> IR — no surface program is affected, the 1,280-run differential passed, and every emission compared
> below is unaffected by it.

## Verdict

**No performance change, and the timing runs are not what proves it.**

The compiler emits **byte-identical LLVM IR** for every benchmark shape and every matmul, and those
link to **byte-identical binaries**. Identical machine code cannot run at a different speed. The
timings below exist only to measure this machine's noise, and they are reported for that.

## 1. The real test — emitted-artifact A/B

Every `.mapal` under `benches/shapes/`, `benches/matmul/` and `examples/`, emitted by both compilers
in three faces (`raw`, `--rewrite`, `--rewrite --contract`) and compared byte for byte.

| | |
| --- | --- |
| byte-identical | **103** |
| different | **1** — `examples/calc.mapal` (raw) |
| new emit failures | **0** (55 skips fail identically on both sides) |

`examples/calc.mapal` is the only file in the tree with a guard arm that can trap, so it is the only
file whose emission is *supposed* to move. Every ladder shape — saxpy, reduce, fir, transpose, gather,
conv2d — and every matmul size is unchanged.

**Binary check.** `saxpy_1048576`'s two `.ll` files compiled with `clang -O2` from the same input
filename produce byte-identical executables. (Compiling from differently-named files produces
different binaries; that difference is the embedded input path, not codegen.)

Reproduce:

```sh
# build the two emitters — COPY EACH OUT BEFORE BUILDING THE OTHER:
# mapal-backend-cuda ships an example with the same name and they collide
# at target/release/examples/emit.
git worktree add /tmp/pre 8b40442
(cd /tmp/pre && cargo build --release -p mapal-backend-llvm --example emit)
cp /tmp/pre/target/release/examples/emit /tmp/emit_pre
cargo build --release -p mapal-backend-llvm --example emit
cp target/release/examples/emit /tmp/emit_post

for f in benches/shapes/*.mapal benches/matmul/*.mapal examples/*.mapal; do
  for a in "" "--rewrite" "--rewrite --contract"; do
    /tmp/emit_pre  "$f" - $a > /tmp/a 2>/dev/null || continue
    /tmp/emit_post "$f" - $a > /tmp/b 2>/dev/null || continue
    cmp -s /tmp/a /tmp/b || echo "DIFFERS: $f ($a)"
  done
done
```

## 2. The timing runs — a noise-floor measurement, not a result

M4 Pro (14 cores, 10 performance), macOS 26.3.1, Homebrew clang 22.1.8. 51 runs per shape per side,
**alternating** PRE/POST within one pass, using each program's own `iter ms=` self-timer, on the
`--rewrite` face. Values byte-identical on every shape.

| shape | n | PRE med | POST med | Δ median | PRE min | POST min | PRE max | POST max |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| saxpy_1048576 | 51 | 0.1238 | 0.1165 | **−5.89%** | 0.0793 | 0.0830 | 0.1813 | 0.1979 |
| conv2d_512 | 51 | 0.0785 | 0.0758 | −3.34% | 0.0523 | 0.0502 | 0.1594 | 0.1378 |
| transpose_1024 | 51 | 0.3724 | 0.3622 | −2.73% | 0.3130 | 0.2978 | 1.5438 | 0.5858 |
| gather_1048576 | 51 | 0.1950 | 0.1973 | +1.18% | 0.1619 | 0.1675 | 0.4615 | 0.5212 |
| fir_65536 | 51 | 0.0801 | 0.0797 | −0.57% | 0.0565 | 0.0462 | 0.1447 | 0.1379 |
| reduce_1048576 | 51 | 0.6069 | 0.6053 | −0.27% | 0.5882 | 0.5867 | 0.6468 | 0.6458 |

All times in ms.

**Read this table as a noise measurement.** The PRE and POST binaries for these shapes are the same
bytes, so every number in the Δ column is measurement error. The largest is **−5.89% on saxpy**, which
would read as a win if the artifacts had not been compared first.

Two limits worth stating:

- **Within-side spread reaches 4.93×** (transpose PRE: 0.3130 min, 1.5438 max). This is an unpinned
  laptop with background load; the max column is not usable.
- **Medians are the only stable statistic here.** saxpy's *min* moved the opposite direction to its
  median (+4.7% vs −5.9%) between identical binaries.

**Rule this supports (S38 measurement rule 6, in its strongest form):** on this Mac, at these
sub-millisecond sizes, **anything under ~6% is nothing.** A change wanting to claim less than that
needs a pinned machine (the i9 at `100.81.226.103`, governor `performance`), not more runs here.

## 3. What was NOT measured

- **The i9.** Everything above is the M4 Pro. Since the artifacts are byte-identical there was nothing
  for a second machine to settle, so the box leg was not run.
- **`examples/calc.mapal`** — the one program whose emission changed. It is a 5-line demo with no
  timing harness; its guard now costs a branch instead of a `select` on the dispatch path. Not
  measured, and not worth measuring at that size.
- **CUDA.** No GPU available locally; no CUDA timing exists for this change, and the CUDA emitter
  change has no hardware verification at all.

## 4. Why there is no regression to explain

The gating rule is legality-then-cost (`GuardSite::gated()`): an arm that **can trap** must be gated;
a **heavy** arm (bulk op or call) is worth gating; two arms of scalar arithmetic are left alone.

That last clause is what keeps this table flat, and it was added *because* of a measurement. The first
implementation gated every guard, which branched even for `-true-> x` — an arm's work list is never
empty, it always ends with its boundary `Pair` edge. In `examples/sepia.mapal` that branch landed
inside a per-element `map` body, where it would have cost the loop its vectorization. The A/B caught
it as `abs` and `sepia` moving when the plan said they could not.
