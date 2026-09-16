//! Explicit store capabilities: diagnostics report the store type,
//! persistence, and ANN status from [`VectorStoreCapabilities`], never by
//! inferring from provider names.

use textintel::core::providers::VectorStore;
use textintel::storage::{JsonFileStore, MemoryStore};
use textintel::TextIntelligence;

#[test]
fn memory_store_reports_type_and_channels_without_ann() {
    let store = MemoryStore::default();
    let capabilities = store.store_capabilities();
    assert_eq!(capabilities.store_type, "memory");
    assert!(!capabilities.persistent);
    assert!(!capabilities.ann_enabled);
    assert_eq!(capabilities.ann_dimensions, None);
    assert_eq!(capabilities.ann_entries, 0);
    assert!(capabilities
        .indexed_channels
        .contains(&"lexical".to_string()));
    assert!(
        !capabilities
            .indexed_channels
            .contains(&"semantic_ann".to_string()),
        "semantic_ann must only appear when an index is serving"
    );
}

#[test]
fn json_store_reports_persistence_without_ann() {
    let path =
        std::env::temp_dir().join(format!("textintel-store-caps-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let store = JsonFileStore::open(&path).unwrap();
    let capabilities = store.store_capabilities();
    assert_eq!(capabilities.store_type, "json");
    assert!(capabilities.persistent);
    assert!(!capabilities.ann_enabled);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn engine_diagnostics_match_store_capabilities() {
    // Regression test: `ann_enabled` must reflect actual ANN availability,
    // not `store.provider == "hnsw_ann"` (which no store reports).
    let engine = TextIntelligence::default();
    let diagnostics = engine.diagnostics();
    assert!(!diagnostics.ann_enabled);
    assert_eq!(diagnostics.store_capabilities.store_type, "memory");
    assert!(!diagnostics.store_capabilities.persistent);
    assert!(!diagnostics.store_capabilities.ann_enabled);
    assert!(
        diagnostics
            .degraded
            .iter()
            .any(|item| item.capability == "retrieval"),
        "index scan must be reported as degraded retrieval"
    );

    let path = std::env::temp_dir().join(format!(
        "textintel-store-caps-engine-{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let engine = TextIntelligence::builder()
        .json_store(&path)
        .build()
        .unwrap();
    let diagnostics = engine.diagnostics();
    assert!(!diagnostics.ann_enabled);
    assert_eq!(diagnostics.store_capabilities.store_type, "json");
    assert!(diagnostics.store_capabilities.persistent);
    assert!(
        diagnostics
            .degraded
            .iter()
            .all(|item| item.capability != "persistence"),
        "a persistent store must not report degraded persistence"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn diagnostics_expose_candidate_budgets() {
    // §11: production diagnostics report the retrieval budgets behind the
    // candidate union (per-channel caps, ANN cap, union cut).
    let engine = TextIntelligence::builder()
        .config(textintel::EngineConfig {
            max_search_candidates: 120,
            max_per_channel_candidates: 34,
            max_ann_candidates: 12,
            ..Default::default()
        })
        .build()
        .unwrap();
    let budgets = engine.diagnostics().candidate_budgets;
    assert_eq!(budgets.max_search_candidates, 120);
    assert_eq!(budgets.max_per_channel_candidates, 34);
    assert_eq!(budgets.max_ann_candidates, 12);

    let defaults = TextIntelligence::default().diagnostics().candidate_budgets;
    let config = textintel::EngineConfig::default();
    assert_eq!(defaults.max_search_candidates, config.max_search_candidates);
    assert_eq!(
        defaults.max_per_channel_candidates,
        config.max_per_channel_candidates
    );
    assert_eq!(defaults.max_ann_candidates, config.max_ann_candidates);
}

#[test]
fn diagnostics_never_carry_raw_user_text() {
    // §11: diagnostics hold counts, names, and revisions — analyzing a
    // distinctive message must not leak it into the report.
    let engine = TextIntelligence::default();
    let sentinel = "quixotic zebra xylophone waltz 741";
    let _ = engine.analyze(sentinel).unwrap();
    let rendered = serde_json::to_string(&engine.diagnostics()).unwrap();
    assert!(
        !rendered.contains("quixotic"),
        "diagnostics leaked user text: {rendered}"
    );
}

#[test]
#[cfg(not(feature = "ann-hnsw"))]
fn json_store_with_ann_fails_loudly_without_the_feature() {
    let path = std::env::temp_dir().join(format!(
        "textintel-store-caps-noann-{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let error = match TextIntelligence::builder()
        .json_store_with_ann(&path, 32, 100)
        .build()
    {
        Ok(_) => panic!("json_store_with_ann must fail without the ann-hnsw feature"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("ann-hnsw"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_file(&path);
}
