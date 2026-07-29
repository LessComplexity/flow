//! **Gate A — consumer coverage** (plan-s41 §2.2 rule 1; ADR-0033).
//!
//! The founding bet is that *one* geometric deduction on the graph pays out on
//! every backend. ADR-0033 was written because that claim had gone nine
//! sessions with exactly one consumer, and the evidence it presented was a
//! hand-run grep:
//!
//! ```text
//! tile_plan consumers:  crates/backends/llvm/src/{lib.rs,func.rs}
//! crates/backends/cuda/src/:  no hits
//! ```
//!
//! **This file is that grep, run on every build.** A geometry query going stale
//! on a backend is a *test gap*, not a consequence of how the code is packaged
//! — Sapir, S41 plan gate: *"the tests should guard it by checking all
//! consumers all the time."* Had this existed at S25 the gap could not have
//! opened, whatever the crate layout.
//!
//! Two things make it a gate rather than a report:
//!
//! 1. A backend that stops calling a geometry query **fails the suite that
//!    day**, instead of drifting silently for nine sessions.
//! 2. A backend directory that is not named in [`BACKENDS`] fails too, so
//!    adding a backend forces an explicit decision about the obligation rather
//!    than defaulting to "exempt by omission" — which is the failure mode this
//!    file exists to remove.
//!
//! **Scope: geometry only.** `tile_plan` and `elem_plan` are the queries whose
//! whole purpose is that a second backend reads them instead of re-deriving the
//! deduction. `path_plan` / `guard_plan` / `emission_plan` / `last_use_plan` /
//! `bounds_proof` are deliberately *not* gated here: consuming them is a
//! per-backend capability decision (the CUDA host emitter keeps strict
//! semantics for loop-touching guard sites by design, S40), not a genericity
//! obligation. Widening this list is a deliberate act.
//!
//! **On exemptions.** An exemption is a recorded debt with a reason and an
//! owner, visible in the suite output. It is never a way to make the gate
//! quiet. Removing one is the win; adding one should feel expensive.

use std::path::{Path, PathBuf};

/// The geometry queries a code-emitting backend is expected to consume — the
/// deduction ADR-0033 says must not be re-derived per target.
const GEOMETRY_QUERIES: [&str; 2] = ["tile_plan", "elem_plan"];

/// Why a backend does not consume a geometry query, when it does not.
enum Coverage {
    /// Must consume every query in [`GEOMETRY_QUERIES`].
    Required,
    /// Recorded debt. The string is the reason, printed on failure of the
    /// *inverse* check: an exempt backend that starts consuming should have its
    /// exemption deleted, and this gate says so rather than staying silent.
    Exempt(&'static str),
}

/// Every backend crate, and what is expected of it. A directory under
/// `crates/backends/` that is missing from this table fails the gate.
const BACKENDS: [(&str, Coverage); 3] = [
    ("llvm", Coverage::Required),
    (
        "cuda",
        Coverage::Exempt(
            "CUDA C emitter, being replaced by NVPTX (Sapir, S38 §6). It predates \
             tile_plan (landed S25; last CUDA session S23) and sits at rung 0 — no \
             __shared__, no tiling. Exemption ends when the NVPTX leg lands \
             (plan-s41); it is NOT a judgement that a C emitter cannot consume the \
             record.",
        ),
    ),
    (
        "verilog",
        Coverage::Exempt(
            "not-started stub; no code beyond the crate root (components/backend-verilog \
             STATUS: not-started). Exemption ends with its first emitter.",
        ),
    ),
];

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is <root>/crates/mapal-ir
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root is two levels above crates/mapal-ir")
        .to_path_buf()
}

/// Every `.rs` file under a directory, recursively.
fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// The files of `backend` that contain a call to `query`.
///
/// Matched as `query(` so a mention in a doc comment does not count as
/// consumption — the gate must track calls, not prose about calls.
fn consumers(backend: &str, query: &str) -> Vec<String> {
    let src = workspace_root()
        .join("crates/backends")
        .join(backend)
        .join("src");
    let needle = format!("{query}(");
    rust_sources(&src)
        .into_iter()
        .filter(|p| {
            std::fs::read_to_string(p).is_ok_and(|t| {
                t.lines()
                    .filter(|l| {
                        // skip doc/line comments so prose never satisfies the gate
                        let t = l.trim_start();
                        !t.starts_with("//")
                    })
                    .any(|l| l.contains(&needle))
            })
        })
        .map(|p| {
            p.strip_prefix(workspace_root())
                .unwrap_or(&p)
                .display()
                .to_string()
        })
        .collect()
}

/// Every backend directory is accounted for in [`BACKENDS`]. Adding a backend
/// must be an explicit decision about the ADR-0033 obligation, never a silent
/// exemption by omission.
#[test]
fn every_backend_directory_is_accounted_for() {
    let dir = workspace_root().join("crates/backends");
    let mut found: Vec<String> = std::fs::read_dir(&dir)
        .expect("crates/backends exists")
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();

    let mut named: Vec<String> = BACKENDS.iter().map(|(n, _)| (*n).to_owned()).collect();
    named.sort();

    assert_eq!(
        found, named,
        "a backend crate is missing from BACKENDS in this file.\n\
         Add it with Coverage::Required, or with an Exempt reason that says why \
         it does not consume the geometry record and what ends the exemption.\n\
         Exemption by omission is exactly what ADR-0033 was written about."
    );
}

/// **The gate.** Every non-exempt backend consumes every geometry query.
#[test]
fn required_backends_consume_every_geometry_query() {
    let mut failures = Vec::new();
    for (backend, coverage) in &BACKENDS {
        let Coverage::Required = coverage else {
            continue;
        };
        for query in GEOMETRY_QUERIES {
            let hits = consumers(backend, query);
            if hits.is_empty() {
                failures.push(format!(
                    "  backend `{backend}` no longer calls `{query}(` anywhere in its src/"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "geometry queries lost a consumer:\n{}\n\n\
         This is the ADR-0033 failure mode, caught on the day it happened rather \
         than nine sessions later. Either restore the call, or move the backend \
         to Coverage::Exempt with a reason and an end condition.",
        failures.join("\n")
    );
}

/// The inverse: an exempt backend that has *started* consuming a query should
/// lose its exemption. Keeping a stale exemption is how a gate goes quiet.
#[test]
fn exemptions_are_still_earned() {
    let mut stale = Vec::new();
    for (backend, coverage) in &BACKENDS {
        let Coverage::Exempt(reason) = coverage else {
            continue;
        };
        for query in GEOMETRY_QUERIES {
            let hits = consumers(backend, query);
            if !hits.is_empty() {
                stale.push(format!(
                    "  backend `{backend}` is exempt but DOES call `{query}(` in: {}\n\
                         recorded reason: {reason}",
                    hits.join(", ")
                ));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "an exemption is no longer true — delete it and make the backend Required:\n{}",
        stale.join("\n")
    );
}

/// The gate can actually see the thing it claims to check. Without this, a path
/// typo would make every `Required` check vacuous and the suite would pass by
/// finding nothing — the worst possible failure for a coverage gate.
#[test]
fn the_gate_is_not_vacuous() {
    let llvm_src = workspace_root().join("crates/backends/llvm/src");
    assert!(
        llvm_src.is_dir(),
        "backend source path is wrong: {llvm_src:?}"
    );
    assert!(
        rust_sources(&llvm_src).len() > 5,
        "expected to find the llvm backend's sources; found {} files",
        rust_sources(&llvm_src).len()
    );
    for query in GEOMETRY_QUERIES {
        assert!(
            !consumers("llvm", query).is_empty(),
            "`{query}` should be visible in the llvm backend — if this fails the \
             matcher is broken, not the backend"
        );
    }
    // and a query that no backend calls must come back empty, or the matcher
    // is matching something other than call sites
    assert!(
        consumers("llvm", "definitely_not_a_query").is_empty(),
        "the matcher reports hits for a query that does not exist"
    );
}
