# Security Policy

Flow is a research compiler. It reads source you supply, emits LLVM IR or CUDA C++, and links
a small runtime (`flow-rt`) into the binary you build. Knowing that shape is most of knowing
what is and is not a security issue here.

## Supported versions

| Version | Supported |
| --- | --- |
| `main` | yes — the only supported branch |
| anything else | no |

There are no releases yet. Fixes land on `main`.

## Reporting a vulnerability

**Do not open a public issue for a vulnerability.** Use either:

- **GitHub private vulnerability reporting** — the *Report a vulnerability* button under this
  repository's **Security** tab (preferred; it keeps the thread with the code), or
- **email: sapir@devshift.sg** — put `flow security` in the subject.

Please include:

- what an attacker gains, and what they need in order to get it;
- a minimal reproducer — a `.flow` source, or the exact IR/command;
- the commit you tested, your OS, and your toolchain versions (`rustc`, `clang`);
- whether it reproduces through the interpreter, the LLVM backend, the CUDA backend, or all
  three.

This is a small project, currently maintained by one person. Realistic expectations, stated
honestly rather than as a service-level promise: an acknowledgement within about a week, an
assessment after that, and a fix on `main` as soon as one exists. You will be credited in the
commit and the advisory unless you ask not to be. Please give a fix a reasonable chance to land
before publishing.

## What is in scope

- **Memory unsafety reachable from source that the compiler accepts** — the emitted program
  writing out of bounds, using freed memory, or corrupting the runtime's arena, for a program
  the front end did not reject.
- **A bounds, trap, or guard check that is elided when it should not be.** Flow removes checks
  it can *prove* redundant (`bounds_proof`). A proof that is unsound is a security bug, not
  just a correctness one.
- **Unsafety in `flow-rt`** — the work-stealing pool, the arena allocator, `reside`, the trap
  protocol.
- **The build/CI supply chain** — anything letting a pull request execute code with repository
  credentials.

## What is out of scope

- **Compiling hostile source.** Flow is a compiler, not a sandbox. A `.flow` file you did not
  write is untrusted code in the same sense a `.c` file is; there is no isolation claim to
  break.
- **A program that traps, diverges, or runs out of memory** — traps and divergence are defined
  behaviour (the interpreter is the specification).
- **Miscompilation with no memory-safety consequence.** That is a correctness bug, and this
  project takes those extremely seriously — but they belong in a public issue with a
  reproducer, where they can be fixed in the open. Use the *Correctness bug* template.
- **Anything in `benches/`, `examples/`, `editors/`, or the docs tooling.** Development
  scaffolding, not shipped surface.
- Vulnerabilities in LLVM, `clang`, `nvcc`, or Rust itself — report those upstream.

## No hardening claims

Flow makes exactly one safety-shaped guarantee, and it is narrow: **the compiled program's
observable behaviour matches the interpreter's, byte for byte, at any optimisation level or
thread count** — traps included. It makes no claim about sandboxing, isolation, constant-time
execution, side-channel resistance, or resistance to a hostile input program. Do not deploy it
where any of those matter.
