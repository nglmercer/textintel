//! ANN restart behavior: the HNSW graph is never persisted, only rebuilt
//! deterministically from the fingerprint store on startup.

#![cfg(feature = "ann-hnsw")]

use textintel::core::providers::VectorStore;
use textintel::storage::MemoryStore;
use textintel::storage::ann::HnswVectorIndex;
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

#[test]
fn memory_ann_is_visible_in_diagnostics() {
    let engine = semantic_engine();
    let store = seed_store();
    assert!(store.store_capabilities().ann_enabled);
    assert_eq!(
        store.store_capabilities().ann_dimensions,
        Some(32),
        "dimensions must be reported"
    );
    assert_eq!(store.store_capabilities().ann_entries, DOCUMENTS.len());
    assert!(
        store
            .store_capabilities()
            .indexed_channels
            .contains(&"semantic_ann".to_string())
    );
    let engine = engine.with_store(store);
    let diagnostics = engine.diagnostics();
    assert!(
        diagnostics.ann_enabled,
        "diagnostics must reflect the serving HNSW index"
    );
    assert_eq!(diagnostics.store_capabilities.store_type, "memory");
    assert_eq!(
        diagnostics.store_capabilities.ann_dimensions,
        Some(32),
        "diagnostics must report ANN dimensions"
    );
    assert_eq!(
        diagnostics.store_capabilities.ann_entries,
        DOCUMENTS.len(),
        "diagnostics must report ANN live entries"
    );
    assert!(
        diagnostics
            .degraded
            .iter()
            .all(|item| item.capability != "retrieval"),
        "serving ANN must not report degraded retrieval: {:?}",
        diagnostics.degraded
    );
}

#[test]
fn json_store_with_ann_rebuilds_automatically_on_open() {
    use textintel::storage::JsonFileStore;

    let path = std::env::temp_dir().join(format!(
        "textintel-ann-json-{}-{}.json",
        std::process::id(),
        "auto"
    ));
    let _ = std::fs::remove_file(&path);
    {
        let engine = semantic_engine().with_store(JsonFileStore::open(&path).unwrap());
        for &(id, text) in DOCUMENTS {
            engine.add_document(id, text).unwrap();
        }
        assert_eq!(engine.document_count().unwrap(), DOCUMENTS.len());
    }
    // Reopen with ANN: no application-level `rebuild_ann` call. The graph is
    // rebuilt from the persisted embeddings before serving.
    let engine =
        semantic_engine().with_store(JsonFileStore::open_with_ann(&path, 32, 100).unwrap());
    let diagnostics = engine.diagnostics();
    assert!(diagnostics.ann_enabled);
    assert_eq!(diagnostics.store_capabilities.store_type, "json");
    assert!(diagnostics.store_capabilities.persistent);
    assert_eq!(diagnostics.store_capabilities.ann_dimensions, Some(32));
    assert_eq!(
        diagnostics.store_capabilities.ann_entries,
        DOCUMENTS.len(),
        "every persisted embedding must be reindexed"
    );
    assert_eq!(
        engine.find_similar("coffee break today", 1).unwrap()[0].id,
        "doc-en"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
#[cfg(feature = "persist-redb")]
fn redb_store_with_ann_rebuilds_automatically_on_open() {
    let path = std::env::temp_dir().join(format!(
        "textintel-ann-redb-{}-auto.redb",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    {
        let engine =
            semantic_engine().with_store(textintel::storage::RedbStore::open(&path).unwrap());
        for &(id, text) in DOCUMENTS {
            engine.add_document(id, text).unwrap();
        }
    }
    let engine = semantic_engine()
        .with_store(textintel::storage::RedbStore::open_with_ann(&path, 32, 100).unwrap());
    let diagnostics = engine.diagnostics();
    assert!(diagnostics.ann_enabled);
    assert_eq!(diagnostics.store_capabilities.store_type, "redb");
    assert_eq!(diagnostics.store_capabilities.ann_entries, DOCUMENTS.len());
    assert_eq!(
        engine.find_similar("coffee break today", 1).unwrap()[0].id,
        "doc-en"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn ann_rebuild_failures_are_explicit() {
    use textintel::storage::JsonFileStore;

    let path = std::env::temp_dir().join(format!(
        "textintel-ann-json-{}-{}.json",
        std::process::id(),
        "full"
    ));
    let _ = std::fs::remove_file(&path);
    {
        let engine = semantic_engine().with_store(JsonFileStore::open(&path).unwrap());
        for &(id, text) in DOCUMENTS {
            engine.add_document(id, text).unwrap();
        }
    }
    // Six records cannot fit an index capped at one element.
    let error = JsonFileStore::open_with_ann(&path, 32, 1).unwrap_err();
    assert!(
        error.contains("ann"),
        "rebuild failures must name ANN: {error}"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn builder_json_store_with_ann_serves_ann() {
    let path = std::env::temp_dir().join(format!(
        "textintel-ann-json-{}-{}.json",
        std::process::id(),
        "builder"
    ));
    let _ = std::fs::remove_file(&path);
    let engine = textintel::TextIntelligence::builder()
        .config(textintel::EngineConfig {
            semantic: true,
            ..Default::default()
        })
        .semantic_provider(textintel::semantic::FeatureHashEmbeddingProvider::new(32).unwrap())
        .json_store_with_ann(&path, 32, 100)
        .build()
        .unwrap();
    for &(id, text) in DOCUMENTS {
        engine.add_document(id, text).unwrap();
    }
    let diagnostics = engine.diagnostics();
    assert!(diagnostics.ann_enabled);
    assert_eq!(diagnostics.store_capabilities.ann_entries, DOCUMENTS.len());
    let _ = std::fs::remove_file(&path);
}
