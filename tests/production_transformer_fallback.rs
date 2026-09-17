//! `production_local()` prefers transformer embeddings when a local model is
//! configured and reports the feature-hash fallback through diagnostics.

use textintel::TextIntelligence;

#[test]
fn production_local_uses_feature_hash_without_model() {
    let engine = TextIntelligence::builder()
        .production_local()
        .build()
        .unwrap();
    let diagnostics = engine.diagnostics();
    assert_eq!(
        diagnostics.embedding.provider, "feature_hash_embedding",
        "unconfigured production preset must fall back to the feature-hash baseline"
    );
    // The generic Basic fallback is still reported.
    assert!(
        diagnostics
            .degraded
            .iter()
            .any(|note| note.capability == "semantic"),
        "semantic Basic fallback must appear in diagnostics: {:?}",
        diagnostics.degraded
    );
}

#[test]
fn production_local_reports_transformer_fallback() {
    let engine = TextIntelligence::builder()
        .transformer_model("/nonexistent/transformer-model-dir")
        .production_local()
        .build()
        .unwrap();
    let diagnostics = engine.diagnostics();
    assert_eq!(diagnostics.embedding.provider, "feature_hash_embedding");
    let note = diagnostics
        .degraded
        .iter()
        .find(|note| note.wanted == "transformer_embedding")
        .expect("configured-but-unusable transformer must be reported");
    assert_eq!(note.configured, "feature_hash_embedding");
    #[cfg(feature = "semantic-transformer")]
    assert!(
        note.detail.contains("failed to open"),
        "unexpected detail: {}",
        note.detail
    );
    #[cfg(not(feature = "semantic-transformer"))]
    assert!(
        note.detail
            .contains("lacks the semantic-transformer feature"),
        "unexpected detail: {}",
        note.detail
    );
}

#[test]
fn explicit_embedding_provider_wins_over_preset() {
    let engine = TextIntelligence::builder()
        .transformer_model("/nonexistent/transformer-model-dir")
        .semantic_provider(textintel::semantic::NullEmbeddingProvider)
        .production_local()
        .build()
        .unwrap();
    // Explicit DI is respected: no transformer fallback is recorded because
    // the preset never attempts embedding selection.
    assert!(
        engine
            .diagnostics()
            .degraded
            .iter()
            .all(|note| note.wanted != "transformer_embedding")
    );
}
