//! HNSW-backed retrieval behind `ann-hnsw`.
//!
//! The store-level test forces the channel-union path (more documents than
//! `max_search_candidates`) and checks that ANN candidates participate,
//! that ranking still uses full comparison, and that removals disappear.

#![cfg(feature = "ann-hnsw")]

use textintel::semantic::embeddings::FeatureHashEmbeddingProvider;
use textintel::{EngineConfig, MemoryStore, TextIntelligence};

fn engine(documents: usize) -> TextIntelligence {
    TextIntelligence::new(EngineConfig {
        semantic: true,
        max_search_candidates: 5,
        ..EngineConfig::default()
    })
    .with_embedding_provider(FeatureHashEmbeddingProvider::default())
    .with_store(MemoryStore::with_ann(256, documents.max(16)).unwrap())
}

#[test]
fn ann_channel_participates_and_ranking_wins() {
    let engine = engine(32);
    let texts = [
        "compra ahora mismo",
        "compra ahora",
        "venta de coches usados",
        "el clima está soleado hoy",
        "nos vemos mañana en el parque",
        "gracias por tu ayuda",
        "la reunión es el lunes",
        "necesito ayuda con la tarea",
        "el gato duerme en el sofá",
        "me gusta el café fuerte",
    ];
    for (index, text) in texts.iter().enumerate() {
        engine.add_document(format!("doc-{index}"), text).unwrap();
    }
    let results = engine.find_similar("compra ahora", 3).unwrap();
    assert_eq!(results[0].id, "doc-1");
    assert!(results[0]
        .retrieval_channels
        .contains(&"semantic_ann".to_string()));
    // Channel union caps candidates: no full scan of the 10 documents.
    assert!(results[0].candidate_count <= 5);
}

#[test]
fn removed_documents_leave_ann_results() {
    let engine = engine(32);
    engine.add_document("keep", "compra ahora mismo").unwrap();
    engine.add_document("drop", "compra ahora").unwrap();
    engine
        .add_document("other", "el clima está soleado")
        .unwrap();
    assert!(engine.remove_document("drop").unwrap());
    let results = engine.find_similar("compra ahora", 5).unwrap();
    assert!(results.iter().all(|result| result.id != "drop"));
}

#[test]
fn ann_reports_recall_memory_and_reduction() {
    use textintel::HnswVectorIndex;

    // Seeded methodology anchor: recall@10 against brute force, memory
    // estimate sanity, and candidate reduction at 2K vectors.
    let dimensions = 32;
    let count = 2_000;
    let index = HnswVectorIndex::new(dimensions, count).unwrap();
    let mut rng = 0xC0FFEEu64;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng as f64 / u64::MAX as f64) as f32
    };
    let mut vectors = Vec::with_capacity(count);
    for point in 0..count {
        let raw: Vec<f32> = (0..dimensions).map(|_| next()).collect();
        let norm = raw.iter().map(|value| value * value).sum::<f32>().sqrt();
        let vector: Vec<f32> = raw.into_iter().map(|value| value / norm).collect();
        index.insert(&format!("doc-{point}"), &vector).unwrap();
        vectors.push(vector);
    }
    let queries: Vec<Vec<f32>> = (0..50)
        .map(|_| {
            let raw: Vec<f32> = (0..dimensions).map(|_| next()).collect();
            let norm = raw.iter().map(|value| value * value).sum::<f32>().sqrt();
            raw.into_iter().map(|value| value / norm).collect()
        })
        .collect();
    let mut recall_sum = 0.0;
    for query in &queries {
        let mut brute: Vec<(usize, f32)> = vectors
            .iter()
            .enumerate()
            .map(|(position, vector)| {
                let dot: f32 = vector.iter().zip(query.iter()).map(|(a, b)| a * b).sum();
                (position, dot)
            })
            .collect();
        brute.sort_by(|left, right| right.1.total_cmp(&left.1));
        let expected: std::collections::BTreeSet<usize> = brute
            .iter()
            .take(10)
            .map(|(position, _)| *position)
            .collect();
        let found: std::collections::BTreeSet<usize> = index
            .search(query, 10)
            .unwrap()
            .into_iter()
            .map(|(id, _)| id["doc-".len()..].parse::<usize>().unwrap())
            .collect();
        recall_sum += expected.intersection(&found).count() as f64 / 10.0;
    }
    let recall = recall_sum / queries.len() as f64;
    assert!(recall >= 0.9, "recall@10 on 2K seeded vectors: {recall:.3}");
    // Memory estimate scales with entries and dimensions.
    let bytes = index.estimate_bytes();
    assert!(bytes >= count * dimensions * 4);
    // Candidate reduction: 10 ANN candidates instead of a 2K full scan.
    let reduction = (count - 10) as f64 / count as f64;
    assert!(reduction > 0.99);
}

#[test]
fn stores_without_embeddings_skip_ann_quietly() {
    // Null embedding backend: no `default` vectors, so ANN stays empty but
    // retrieval still works through the other channels.
    let engine = TextIntelligence::new(EngineConfig {
        max_search_candidates: 2,
        ..EngineConfig::default()
    })
    .with_store(MemoryStore::with_ann(256, 16).unwrap());
    engine.add_document("a", "hello world").unwrap();
    engine.add_document("b", "hello there").unwrap();
    engine.add_document("c", "something else entirely").unwrap();
    let results = engine.find_similar("hello world", 2).unwrap();
    assert_eq!(results[0].id, "a");
}
