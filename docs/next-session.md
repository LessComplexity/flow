# Next Session (S48)

Written: 2026-07-31 · end of S44–S47 · by: Claude (orchestrator; category-architect skill)
Session log: `sessions/2026-07-31-s44-s47-conflict-not-capacity.md` — **read it**
Previous: S43 (`sessions/2026-07-31-s43-the-pack-was-serial.md`).
Records that govern S48: **`docs/performance/s44-conflict-not-capacity.md`** and
`docs/performance/s43-residency-and-the-thermal-artifact.md`.

## READ THIS FIRST

**`main` @ `0f40ce0`, pushed, clean. Gate 1047 passed / 0 failed, fmt clean.** Everything from
S44–S47 is merged; nothing is waiting in a worktree except `nc` blocking (below).

**The compiler detects the machine by default.** `EmitOpts::default().target` is `"native"`;
`resolve("native")` falls back to `GENERIC` when a fact cannot be read. Consequence: **emission is
now machine-dependent.** Anything that needs reproducible *emitted text* across machines must pin
`target: "generic"` by name — six tests already do, and they say why at the site.

**Two published numbers were retracted in S43 and stay retracted:** the `1864` GF/s L1 ceiling
(thermal drift) and `1043 GF/s` as the N=4096 cell (it is 803).

**`benches/emit_sweep_ab.sh` has had THREE silent-pass paths, all closed.** A zsh shebang that
passed no flags and exited 0; a failed emission hashing empty output so two broken runs "matched";
and pointing at the wrong `emit` binary (cuda and llvm both build one, and the cuda one rejects
`--contract`). It now preflights the binary and hard-errors on failures. **3 of 159 cells always
fail** — `examples/vector.mapal` does not parse.

**Every timed run goes through `benches/perflock.sh`** (exit 75 = retry, command did not run).

## S48 opens on — the single-thread matmul gap

### 1. P0 — one thread is ~2× behind numpy, and every blocking knob is spent

~800 GF/s against numpy's ~1640 at N=4096. The 1.71× operand-residency win is real and
assembly-verified but **unclaimed**, and the three obvious levers are measured and refuted:

| lever | 1 thread | threaded |
| --- | ---: | ---: |
| `kc` blocking | +6.1% | −25.5% |
| operand residency (window instrument) | **+71%** | +5% |
| `nc` blocking | +18.7% | parity |

What this machine actually charges for: **nothing** for L1-vs-L2; a real price for falling out of the
16 MB shared L2; a **1.571×** penalty for crossing ~2k–4k pages of TLB reach; and — new in S44 —
a large price for **set-index conflict**, which is a different axis from all of the above.

The 1.71× is confounded between capacity and TLB reach and **no in-kernel arm can separate them**
(`inbounds` bounds the offsets to ≤32 pages, refuted on arithmetic). So a design that captures it
must change the **reuse structure**, not add a blocking level. At one thread there is no Amdahl term
to remove, which is what makes this harder than the threaded problem S43 solved.

### 2. P1 — the queue

- **`nc` blocking** is built, swept, gated green and ships OFF, still in worktree
  `agent-a03f9b23183f1440c`. Merge as a documented lever or discard it.
- **`B` leaves 1.15–1.27× on the i9.** S47 proved it is not derivable from readable facts: the i9
  wants a block 4–8× larger while every readable fact is larger on the M4. Revisit only if the
  benefit becomes predictable. The ceiling is documented at `move_block`.
- **Width 1536 declines** rather than winning, costing the i9 5.7%/4.2% there. Same unreadable
  quantity.
- `examples/vector.mapal` does not parse · delete or justify `kc_nest` · executing SME value check in
  `cargo test` — all three unchanged since S42.

### 3. Measurement rules — S44–S47 added one, and amended it

> **24. Classify what an optimization removes and its thread-count behaviour follows.** A serial
> fraction does nothing at 1 thread and a lot threaded. A shared bottleneck is big at 1 thread and
> gone threaded. A per-core resource conflict grows with cores. **Amendment (S46): the third holds
> only while the per-core resource still binds** — the same rung shrank 2.646× → 1.547× across 32
> i9 cores because the fixed arm hits a shared ceiling the slow arm never reaches.

Rules 19–23 from S43 stand. The one that earned the most in S44–S47 is **4 — sweep, never one
point**: both cache walls prescribed `nc` ≤ 512 and 512 lost while 1024 won by 18.7%; the predicted
block size was wrong by 4–8× on the i9; and a single-point test at either would have shipped a loss.

## FIRST commands

```sh
cargo test --workspace --release                       # 1047 passed / 0 failed
./benches/perflock.sh ./benches/matmul/numpy_ab.sh target/release/examples/emit 4096 15
git worktree list                                      # 9 agent + 3 pre-rename, all prunable
ssh 100.81.226.103 'du -sh ~/mapal-s4*'                # 1.02 GB of scratch to delete
```

## Live state at close

| Type | Handle | State | Inspect | Cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` @ `0f40ce0` | **pushed, in sync, clean** — only `oainotes.md` untracked | `git status -sb` | — |
| worktree ×9 | `.claude/worktrees/agent-*` | merged or probe-only; sources all in `main` | `git worktree list` | discardable |
| worktree ×3 | `…-Personal-**Flow**/…` | prunable, pre-rename paths | " | `git worktree prune` |
| machine | Arch box `100.81.226.103` | up, i9-14900F, **has cargo/rustc 1.90** — emission can run on the box, which detection requires | `ssh … nproc` | owned box |
| artifact | box `~/mapal-s42,44,45,46,47` | **1.02 GB** | `ssh … 'du -sh ~/mapal-s4*'` | **delete — nothing depends on them** |
| file | `oainotes.md` | untracked, deliberately uncommitted | — | Sapir's call |

**Nothing is running.** No background job, no server, no port. Measurement mutex free.

## Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | **single-thread matmul ~2× behind numpy** | §1 | a reuse-structure change, not another blocking level | 1t GF/s moves off ~800 |
| P1 | decide `nc` blocking's fate | §2 | merge as a documented lever or discard | resolved |
| P1 | `B` short by 1.15–1.27× on the i9 | §2 | not derivable; revisit if the benefit becomes predictable | derived == optimum, or closed |
| P1 | width 1536 declines | §2 | costs the i9 5.7%/4.2% | fires safely at non-pow2 widths |
| P1 | `examples/vector.mapal` does not parse | READ THIS FIRST | 3 of 159 gate cells always fail | it parses, or leaves the sweep |
| P1 | delete or justify `kc_nest` | `lib.rs::EmitOpts` | unchanged since S42 | gone, or has a written reason |
| P1 | executing SME value check in `cargo test` | `benches/sme/README.md` | unchanged since S42 | the suite runs an SME binary |
| P2 | box scratch, 1.02 GB across five dirs | live state | delete | gone |
| P2 | 12 worktrees | live state | remove agent ones, then `git worktree prune` | only `main` listed |
| P2 | f16/bf16 rung (2× MAC density) | S42 §5e | plan first | `svmopa_za32_f16_m` emitted |

## Standing direction (Sapir — unchanged)

- Compute-only legs; numpy in every verdict table; scale everything up.
- Parallel-first by construction — **threaded is the configuration that ships.**
- Backend-genericity contract (ADR-0032): mapal-ir never learns machine facts.
- **Keep stuff generic, not hardcoded** — and when a constant is swept rather than derived, say so.
- Query, not rewrite: record that something *could* be skipped; never delete it.
- Compile time decides the SIZES, runtime decides the ASSIGNMENT.
- Nothing goes in the README that a default build does not deliver.
- Proof over suggestion — a change arrives with the measurement of what it did.
- **The README is results, not method.** Tables and a pointer; explanations live in `docs/`.
- Speak simply, base claims on empirical results. **Give absolute ms, not only ratios.**
