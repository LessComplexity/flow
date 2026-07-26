//! Component-level benchmark (DESIGN §21): `parse` of each example plus a ~100×
//! sepia-shaped synthetic concatenation for an O(n) sanity curve.
//!
//! `criterion` is a plain dev-dependency (there is no `[workspace.dependencies]`
//! table; this mirrors the insta/proptest lines), with a `[[bench]]
//! harness = false` stanza in the manifest. The synthetic input is built by
//! string concatenation with renamed functions at bench setup, so it lexes and
//! parses cleanly.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

/// Read an example `.mapal` source by name.
fn read_example(name: &str) -> String {
    let path = format!("{}/../../examples/{name}.mapal", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// Build a ~`n`× synthetic source by concatenating sepia-shaped functions with
/// uniquely renamed `fn`/`type` identifiers so the whole thing parses cleanly.
fn synthetic_sepia(n: usize) -> String {
    let base = read_example("sepia");
    let mut out = String::with_capacity(base.len() * n);
    for i in 0..n {
        // Rename the two top-level introducers so identifiers stay unique. The
        // body is otherwise identical; we only need a large, well-formed program.
        let renamed = base
            .replace("type Pixel", &format!("type Pixel{i}"))
            .replace("Pixel {", &format!("Pixel{i} {{"))
            .replace("Pixel;", &format!("Pixel{i};"))
            .replace("[Pixel;", &format!("[Pixel{i};"))
            .replace("fn clamp", &format!("fn clamp{i}"))
            .replace("-> clamp", &format!("-> clamp{i}"))
            .replace("fn main", &format!("fn main{i}"));
        out.push_str(&renamed);
        out.push('\n');
    }
    out
}

fn bench_examples(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_examples");
    for name in ["abs", "fanout", "fir", "pipeline", "sepia", "sum_to_n"] {
        let src = read_example(name);
        group.bench_function(name, |b| {
            b.iter(|| {
                let out = mapal_syntax::parse(black_box(&src));
                black_box(out.program.items.len())
            })
        });
    }
    group.finish();
}

fn bench_synthetic(c: &mut Criterion) {
    let src = synthetic_sepia(100);
    let mut group = c.benchmark_group("parse_synthetic");
    group.bench_function("sepia_x100", |b| {
        b.iter(|| {
            let out = mapal_syntax::parse(black_box(&src));
            black_box(out.program.items.len())
        })
    });
    group.finish();
}

criterion_group!(benches, bench_examples, bench_synthetic);
criterion_main!(benches);
