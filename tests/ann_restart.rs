//! ANN restart behavior: the HNSW graph is never persisted, only rebuilt
//! deterministically from the fingerprint store on startup.

#![cfg(feature = "ann-hnsw")]

use textintel::core::providers::VectorStore;
use textintel::storage::ann::HnswVectorIndex;
use textintel::storage::MemoryStore;
use textintel::{EngineConfig, TextIntelligence};

const DOCUMENTS: &[(&str, &str)] = &[
    ("doc-es", "fracasado compra ahora"),
    ("doc-en", "coffee break tomorrow"),
    ("doc-fr", "bonjour salut amour"),
    ("doc-de", "liebe kaffee morgen"),
    ("doc-it", "ciao amore caffe"),
    ("doc-pt", "ola amor cafe"),
];

fn semantic_engine() -> TextIntelligence {
    let config = EngineConfig {
        semantic: true,
        ..Default::default()
    };
    TextIntelligence::new(config).with_embedding_provider(
        textintel::semantic::FeatureHashEmbeddingProvider::new(32).unwrap(),
    )
}

fn seed_store() -> MemoryStore {
    let engine = semantic_engine();
    let mut store = MemoryStore::with_ann(32, 100).unwrap();
    for (id, text) in DOCUMENTS {
        let fingerprint = engine.analyze(text).unwrap();
        assert!(
            fingerprint.semantic_embeddings.contains_key("default"),
            "seed fingerprints must carry whole-text vectors"
        );
        store.upsert(id.to_string(), fingerprint).unwrap();
    }
    store
}

fn top_hit(store: &MemoryStore, query: &[f32]) -> String {
    let index = store.ann_index().expect("ANN configured");
    index.search(query, 3).unwrap()[0].0.clone()
}

#[test]
fn restart_rebuild_returns_identical_neighbors() {
    let engine = semantic_engine();
    let store = seed_store();
    let query = engine.analyze("coffee break today").unwrap();
    let vector = query.semantic_embeddings["default"].clone();
    let before = top_hit(&store, &vector);

    // Simulate a restart: drop the graph, snapshot the pairs, rebuild.
    let snapshot = HnswVectorIndex::snapshot_store(&store);
    assert_eq!(snapshot.len(), DOCUMENTS.len());
    drop(store);
    let rebuilt = HnswVectorIndex::rebuild(32, 100, &snapshot).unwrap();
    let after = rebuilt.search(&vector, 3).unwrap()[0].0.clone();
    assert_eq!(before, after, "restart must preserve neighbors");
    assert_eq!(before, "doc-en");
}

#[test]
fn rebuild_is_order_independent_and_compacts_removals() {
    let store = seed_store();
    let mut snapshot = HnswVectorIndex::snapshot_store(&store);
    snapshot.reverse();
    let first = HnswVectorIndex::rebuild(32, 100, &snapshot).unwrap();
    snapshot.reverse();
    let second = HnswVectorIndex::rebuild(32, 100, &snapshot).unwrap();
    let engine = semantic_engine();
    let query = engine.analyze("bonjour amour").unwrap();
    let vector = query.semantic_embeddings["default"].clone();
    assert_eq!(
        first.search(&vector, 6).unwrap(),
        second.search(&vector, 6).unwrap(),
        "insertion order must not affect the rebuilt graph"
    );

    // Removals compact on rebuild instead of lingering as tombstones.
    let mut store = seed_store();
    assert!(store.remove("doc-es").unwrap());
    let live = store.rebuild_ann().unwrap();
    assert_eq!(live, DOCUMENTS.len() - 1);
    let index = store.ann_index().unwrap();
    assert_eq!(index.len(), DOCUMENTS.len() - 1);
    let engine = semantic_engine();
    let query = engine.analyze("fracasado compra").unwrap();
    let hits = index
        .search(&query.semantic_embeddings["default"], 6)
        .unwrap();
    assert_eq!(hits.len(), DOCUMENTS.len() - 1);
    assert!(hits.iter().all(|(id, _)| id != "doc-es"));
    assert!(hits.iter().any(|(id, _)| id == "doc-en"));
}

#[test]
fn rebuild_without_ann_is_an_explicit_error() {
    let engine = semantic_engine();
    let mut store = MemoryStore::default();
    for (id, text) in DOCUMENTS {
        store
            .upsert(id.to_string(), engine.analyze(text).unwrap())
            .unwrap();
    }
    let error = store.rebuild_ann().unwrap_err();
    assert!(
        error.contains("no ANN accelerator"),
        "unexpected error: {error}"
    );
}
