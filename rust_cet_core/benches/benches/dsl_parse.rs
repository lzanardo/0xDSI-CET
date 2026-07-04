//! Criterion benches for the CSV pattern parser.

#![allow(missing_docs)]

use cet_dsl::parse_query;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_parse_short(c: &mut Criterion) {
    c.bench_function("dsl_parse_short", |b| {
        b.iter(|| {
            let q = parse_query(black_box("q"), black_box("A+,B,C"), 60_000, 10_000).unwrap();
            black_box(q);
        })
    });
}

fn bench_parse_wide(c: &mut Criterion) {
    // Max-length pattern (MAX_SEQ = 16).
    let pattern: String = (0..16).map(|i| format!("E{i}")).collect::<Vec<_>>().join(",");
    c.bench_function("dsl_parse_wide", |b| {
        b.iter(|| {
            let q = parse_query(black_box("q"), black_box(&pattern), 60_000, 10_000).unwrap();
            black_box(q);
        })
    });
}

criterion_group!(benches, bench_parse_short, bench_parse_wide);
criterion_main!(benches);
