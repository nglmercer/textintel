use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

use textintel::{
    FINGERPRINT_SCHEMA_VERSION, LogisticTrainingBatch, TextIntelligence, logistic_step,
};

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

fn training(c: &mut Criterion) {
    let features: Vec<Vec<f64>> = (0..1229)
        .map(|row| {
            (0..30)
                .map(|column| ((row * 17 + column * 13) % 100) as f64 / 100.0)
                .collect()
        })
        .collect();
    let labels: Vec<bool> = (0..features.len()).map(|row| row % 3 != 0).collect();
    let mut weights = vec![0.0; 30];
    let mut bias = 0.0;
    c.bench_function("logistic_epoch_oneshot", |bench| {
        bench.iter(|| {
            black_box(logistic_step(
                &features,
                &labels,
                &mut weights,
                &mut bias,
                0.2,
                1e-3,
            ))
        });
    });
    let mut weights = vec![0.0; 30];
    let mut bias = 0.0;
    let mut batch = LogisticTrainingBatch::new(&features, &labels);
    c.bench_function("logistic_epoch_reused", |bench| {
        bench.iter(|| black_box(batch.step(&mut weights, &mut bias, 0.2, 1e-3)));
    });
}

criterion_group!(benches, deterministic_core, training);
criterion_main!(benches);
