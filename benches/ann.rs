//! HNSW benchmark: build and query latency at 1K/10K/100K vectors.
//!
//! Recall, memory, and candidate-reduction statistics are asserted in
//! `tests/ann_search.rs`, which runs in CI; criterion here measures latency
//! distributions only. Requires the `ann-hnsw` feature, otherwise empty.

#[cfg(feature = "ann-hnsw")]
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
#[cfg(feature = "ann-hnsw")]
use textintel::HnswVectorIndex;

#[cfg(feature = "ann-hnsw")]
struct Rng(u64);

#[cfg(feature = "ann-hnsw")]
impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn next_unit(&mut self) -> f32 {
        const SCALE: f64 = 1.0 / u64::MAX as f64;
        (self.next() as f64 * SCALE) as f32
    }
}

#[cfg(feature = "ann-hnsw")]
const DIMENSIONS: usize = 64;

#[cfg(feature = "ann-hnsw")]
fn seeded_vectors(count: usize, seed: u64) -> Vec<Vec<f32>> {
    let mut rng = Rng(seed);
    (0..count)
        .map(|_| {
            let raw: Vec<f32> = (0..DIMENSIONS).map(|_| rng.next_unit()).collect();
            let norm = raw.iter().map(|value| value * value).sum::<f32>().sqrt();
            raw.into_iter().map(|value| value / norm).collect()
        })
        .collect()
}

#[cfg(feature = "ann-hnsw")]
fn build_index(count: usize) -> HnswVectorIndex {
    let index = HnswVectorIndex::new(DIMENSIONS, count).unwrap();
    for (position, vector) in seeded_vectors(count, 0xB00).iter().enumerate() {
        index.insert(&format!("doc-{position}"), vector).unwrap();
    }
    index
}

#[cfg(feature = "ann-hnsw")]
fn bench_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("ann_build");
    for size in [1_000usize, 10_000, 100_000] {
        // Large builds are slow: fewer samples, longer budget.
        group.sample_size(if size >= 100_000 { 10 } else { 30 });
        group.bench_function(BenchmarkId::from_parameter(size), |bench| {
            bench.iter(|| build_index(black_box(size)))
        });
    }
    group.finish();
}

#[cfg(feature = "ann-hnsw")]
fn bench_query(c: &mut Criterion) {
    let mut group = c.benchmark_group("ann_query");
    for size in [1_000usize, 10_000, 100_000] {
        let index = build_index(size);
        let queries = seeded_vectors(64, 0xBEEF);
        let mut round = 0usize;
        group.bench_function(BenchmarkId::from_parameter(size), |bench| {
            bench.iter(|| {
                round += 1;
                index
                    .search(black_box(&queries[round % queries.len()]), black_box(10))
                    .unwrap()
            })
        });
    }
    group.finish();
}

#[cfg(feature = "ann-hnsw")]
criterion_group!(benches, bench_build, bench_query);
#[cfg(feature = "ann-hnsw")]
criterion_main!(benches);

#[cfg(not(feature = "ann-hnsw"))]
fn main() {}
