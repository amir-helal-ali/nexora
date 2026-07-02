//! Criterion entry point — exposes all 9 suites as criterion benchmarks.

use criterion::{criterion_group, criterion_main, Criterion};
use nexora_benchmarks::suites::*;

fn print_summary() {
    println!("\n=== Nexora Benchmark Summary ===");
    for r in run_all() {
        println!("  {r}");
    }
    println!("=================================\n");
}

fn criterion_bench(c: &mut Criterion) {
    let mut g = c.benchmark_group("nxp_frame");
    g.bench_function("encode", |b| b.iter(|| bench_nxp_frame()));
    g.finish();

    let mut g = c.benchmark_group("crypto");
    g.bench_function("aead", |b| b.iter(|| bench_aead()));
    g.bench_function("ed25519", |b| b.iter(|| bench_ed25519()));
    g.finish();

    let mut g = c.benchmark_group("core");
    g.bench_function("event_bus_publish", |b| b.iter(|| bench_event_bus()));
    g.bench_function("wasm_manifest_validate", |b| b.iter(|| bench_wasm_manifest_validate()));
    g.finish();

    let mut g = c.benchmark_group("services");
    g.bench_function("auth_token", |b| b.iter(|| bench_auth_token()));
    g.bench_function("marketplace", |b| b.iter(|| bench_marketplace()));
    g.bench_function("billing", |b| b.iter(|| bench_billing()));
    g.bench_function("notification_dispatch", |b| b.iter(|| bench_notification_dispatch()));
    g.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = criterion_bench, print_summary,
}
criterion_main!(benches);
