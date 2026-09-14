use criterion::{black_box, criterion_group, criterion_main, Criterion};

use textintel::{TextIntelligence, FINGERPRINT_SCHEMA_VERSION};

fn deterministic_core(c: &mut Criterion) {
    let engine = TextIntelligence::default();
    c.bench_function("analyze_rebus", |bench| {
        bench.iter(|| {
            let result = engine.analyze(black_box("Fra🏠do c0mpr4 ah0r4"));
            assert!(result.is_ok());
            assert_eq!(result.unwrap().schema_version, FINGERPRINT_SCHEMA_VERSION);
        });
    });
    c.bench_function("compare_obfuscated", |bench| {
        bench.iter(|| {
            let result = engine.compare(black_box("c0mpr4 ah0r4"), black_box("compra ahora"));
            assert!(result.is_ok());
        });
    });
    c.bench_function("decode_symbol", |bench| {
        bench.iter(|| {
            let result = engine.decode(black_box("salU2"));
            assert!(result.is_ok());
        });
    });
}

criterion_group!(benches, deterministic_core);
criterion_main!(benches);
