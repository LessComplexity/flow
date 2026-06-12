//! `lower_scale` bench (DESIGN §14.6): lower a synthetic N-stage pipeline and an
//! N-arm value match, recording build numbers.

use criterion::{Criterion, criterion_group, criterion_main};
use flow_lower::lower;
use flow_syntax::parse;

/// A synthetic N-stage shorthand pipeline `fn f(data: i32) -> i32 { data * 2 -> + 1 -> ... -> ret; }`.
fn pipeline_src(n: usize) -> String {
    let mut s = String::from("fn f(data: i32) -> i32 {\n    data * 2\n");
    for _ in 0..n {
        s.push_str("        -> + 1\n");
    }
    s.push_str("        -> ret;\n}\n\nfn main() {\n    10 -> f -> r;\n    r -> print;\n}\n");
    s
}

/// A synthetic N-arm value match `fn vm(k: i32) -> i32 { k -> { -0-> 0; ...; -_-> 99; } -> ret; }`.
fn vmatch_src(n: usize) -> String {
    let mut s = String::from("fn vm(k: i32) -> i32 {\n    k -> {\n");
    for i in 0..n {
        s.push_str(&format!("        -{i}-> {i};\n"));
    }
    s.push_str("        -_-> 99;\n    } -> ret;\n}\n\nfn main() {\n    3 -> vm -> r;\n    r -> print;\n}\n");
    s
}

fn bench_lower(c: &mut Criterion) {
    let pipe = pipeline_src(32);
    let vm = vmatch_src(16);
    let pipe_prog = parse(&pipe);
    let vm_prog = parse(&vm);
    assert!(pipe_prog.diagnostics.is_empty());
    assert!(vm_prog.diagnostics.is_empty());

    c.bench_function("lower_pipeline_32", |b| {
        b.iter(|| {
            let _ = lower(&pipe, &pipe_prog.program);
        })
    });
    c.bench_function("lower_vmatch_16", |b| {
        b.iter(|| {
            let _ = lower(&vm, &vm_prog.program);
        })
    });
}

criterion_group!(benches, bench_lower);
criterion_main!(benches);
