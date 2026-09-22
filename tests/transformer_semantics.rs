//! Transformer-backed semantic evidence (§4, §7, §10).
//!
//! Requires the `semantic-transformer` feature and the tiny deterministic
//! fixture in `tests/fixtures/mini-transformer` (random weights: routing and
//! mechanics only, no semantics). These tests prove the contextual semantic
//! features are live functions of transformer evidence — routed by quality
//! tier, language scope, lexical support, and cross-script gates — rather
//! than dead production features silently stuck at zero. Paraphrase quality
//! needs a real checkpoint and stays out of the test suite.

#![cfg(feature = "semantic-transformer")]

use std::collections::BTreeMap;

use textintel::core::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
use textintel::core::providers::EmbeddingProvider;
use textintel::semantic::FeatureHashEmbeddingProvider;
use textintel::{
    EngineConfig, TextIntelligence, TransformerEmbeddingProvider, language_agreement,
    transliteration_evidence,
};

const FIXTURE: &str = "tests/fixtures/mini-transformer";

fn transformer_engine() -> TextIntelligence {
    let provider = TransformerEmbeddingProvider::open(FIXTURE).expect("fixture must open");
    TextIntelligence::new(EngineConfig {
        semantic: true,
        ..EngineConfig::default()
    })
    .with_embedding_provider(provider)
}

fn feature_hash_engine() -> TextIntelligence {
    TextIntelligence::new(EngineConfig {
        semantic: true,
        ..EngineConfig::default()
    })
    .with_embedding_provider(FeatureHashEmbeddingProvider::new(256).unwrap())
}

/// Controlled backend with exact per-text vectors (fallback default keeps
/// every embedding input non-empty so analysis never errors).
#[derive(Debug, Clone)]
struct MapEmbedding {
    values: BTreeMap<String, Vec<f32>>,
    default: Vec<f32>,
}

impl EmbeddingProvider for MapEmbedding {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, textintel::ProviderError> {
        Ok(texts
            .iter()
            .map(|text| {
                self.values
                    .get(text)
                    .cloned()
                    .unwrap_or_else(|| self.default.clone())
            })
            .collect())
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("map_test_embedding")
            .with_dimensions(self.default.len())
            .with_quality(CapabilityLevel::Basic)
    }

    fn model_metadata(&self) -> Option<ModelMetadata> {
        Some(ModelMetadata {
            model_id: "map-test-v1".to_string(),
            revision: Some("test".to_string()),
            dimensions: self.default.len(),
            normalized: true,
            languages: vec!["multilingual".to_string()],
            source: Some("test".to_string()),
            license: Some("MIT".to_string()),
        })
    }
}

#[test]
fn transformer_fingerprints_carry_production_provenance() {
    let engine = transformer_engine();
    let fingerprint = engine.analyze("hello world").unwrap();
    assert_eq!(
        fingerprint
            .metadata
            .get("semantic_quality")
            .map(String::as_str),
        Some("production"),
        "transformer vectors must be marked production quality"
    );
    assert_eq!(
        fingerprint
            .metadata
            .get("semantic_model")
            .map(String::as_str),
        Some("textintel-mini-bert-fixture")
    );
    assert_eq!(
        fingerprint
            .metadata
            .get("semantic_revision")
            .map(String::as_str),
        Some("fixture-1")
    );
    assert_eq!(
        fingerprint
            .metadata
            .get("semantic_dimensions")
            .map(String::as_str),
        Some("16")
    );
    let diagnostics = engine.diagnostics();
    assert_eq!(diagnostics.embedding.quality, CapabilityLevel::Production);
    let model = diagnostics.embedding_model.expect("model metadata");
    assert_eq!(model.model_id, "textintel-mini-bert-fixture");
    assert_eq!(model.revision.as_deref(), Some("fixture-1"));
    assert_eq!(model.dimensions, 16);
    assert!(
        diagnostics
            .degraded
            .iter()
            .all(|note| note.capability != "semantic"),
        "healthy transformer must not report degraded semantics: {:?}",
        diagnostics.degraded
    );
}

#[test]
fn contextual_semantic_carries_transformer_evidence() {
    let engine = transformer_engine();
    // Identical texts: cosine is exactly ~1.0 even with random weights, so
    // the routed features must carry nonzero evidence, not silence.
    let result = engine
        .compare(
            "the river flows north quietly",
            "the river flows north quietly",
        )
        .unwrap();
    let semantic = result.semantic.expect("semantic channel");
    assert!(
        semantic > 0.999,
        "identical texts must be semantically identical: {semantic}"
    );
    assert!(
        (result.contextual_semantic - semantic).abs() < 1e-9,
        "contextual_semantic must route the transformer channel: {} vs {semantic}",
        result.contextual_semantic
    );
    assert!(
        result.contextual_semantic > 0.999,
        "contextual evidence must be nonzero: {}",
        result.contextual_semantic
    );
}

#[test]
fn feature_hash_pairs_keep_contextual_semantic_gated() {
    // The same routing with the Basic fallback: evidence present on the
    // channel, but explicitly gated out of the transformer-only feature.
    let engine = feature_hash_engine();
    let fingerprint = engine.analyze("hello world").unwrap();
    assert_eq!(
        fingerprint
            .metadata
            .get("semantic_quality")
            .map(String::as_str),
        Some("basic"),
        "feature-hash vectors must stay Basic"
    );
    let result = engine.compare("hello world", "hello world").unwrap();
    assert!(
        result.semantic.unwrap_or(0.0) > 0.999,
        "identical texts must match on the channel"
    );
    assert_eq!(
        result.contextual_semantic, 0.0,
        "Basic backends must not feed contextual_semantic"
    );
}

#[test]
fn cross_language_semantic_tracks_the_transformer_channel() {
    let engine = transformer_engine();
    // "hello world" / "hola mundo" deterministically disagree in language
    // (see `cross_language_semantic_routes_by_language_scope`), so the
    // feature must equal the live channel value.
    let left = engine.analyze("hello world").unwrap();
    let right = engine.analyze("hola mundo").unwrap();
    assert!(
        language_agreement(&left, &right) < 0.5,
        "test pair must disagree in language"
    );
    let result = engine.compare("hello world", "hola mundo").unwrap();
    let semantic = result.semantic.expect("semantic channel");
    assert!(
        (result.cross_language_semantic - semantic).abs() < 1e-9,
        "cross_language_semantic must route the channel: {} vs {semantic}",
        result.cross_language_semantic
    );
    assert!(
        (result.contextual_semantic - semantic).abs() < 1e-9,
        "contextual_semantic must route the channel: {} vs {semantic}",
        result.contextual_semantic
    );

    // Same-language pairs gate the cross-language split to zero.
    let same = engine.compare("hello world", "hello world").unwrap();
    assert_eq!(same.cross_language_semantic, 0.0);
}

#[test]
fn semantic_without_lexical_overlap_routes_disjoint_pairs() {
    let engine = transformer_engine();
    let result = engine.compare("alpha beta", "gamma delta").unwrap();
    assert!(
        result.lexical.unwrap_or(1.0) < 0.3,
        "test pair must be lexically disjoint"
    );
    let semantic = result.semantic.expect("semantic channel");
    assert!(
        (result.semantic_without_lexical_overlap - semantic).abs() < 1e-9,
        "disjoint pairs must route the channel: {} vs {semantic}",
        result.semantic_without_lexical_overlap
    );

    let overlapping = engine.compare("alpha beta", "alpha beta gamma").unwrap();
    assert!(overlapping.lexical.unwrap_or(0.0) >= 0.3);
    assert_eq!(
        overlapping.semantic_without_lexical_overlap, 0.0,
        "lexically supported pairs must not use the disjoint split"
    );
}

#[test]
fn transliteration_agreement_produces_nonzero_evidence() {
    // Controlled cosine 1.0 behind a genuine cross-script link: the
    // agreement interaction must fire, proving the feature is live.
    let engine = TextIntelligence::new(EngineConfig {
        semantic: true,
        ..EngineConfig::default()
    })
    .with_embedding_provider(MapEmbedding {
        values: BTreeMap::new(),
        default: vec![1.0, 0.0],
    });
    let result = engine.compare("habibi", "حبيبي").unwrap();
    assert_eq!(result.semantic, Some(1.0));
    assert_eq!(result.cross_script_pair, 1.0);
    assert!(
        result.transliteration_semantic_agreement > 0.0,
        "agreement must fire on backed cross-script evidence: {}",
        result.transliteration_semantic_agreement
    );
    assert_eq!(
        result.transliteration_semantic_conflict, 0.0,
        "full semantic backing leaves no conflict gap"
    );
}

#[test]
fn transliteration_semantic_interactions_follow_channel_evidence() {
    // Formula identity with live transformer vectors: whatever the channel
    // reports, both interactions must be exact functions of it.
    let engine = transformer_engine();
    let left = engine.analyze("habibi").unwrap();
    let right = engine.analyze("حبيبي").unwrap();
    let result = engine.compare("habibi", "حبيبي").unwrap();

    let weighted = transliteration_evidence(&left, &right)
        .map(|evidence| evidence.weighted())
        .unwrap_or(0.0);
    assert!(weighted > 0.0, "pair must carry transliteration evidence");
    let semantic = result.semantic.expect("semantic channel");
    let floor = semantic.max(0.0);
    let expected_agreement = (weighted * floor * result.cross_script_pair).clamp(0.0, 1.0);
    assert!(
        (result.transliteration_semantic_agreement - expected_agreement).abs() < 1e-9,
        "agreement formula drift: {} vs {expected_agreement}",
        result.transliteration_semantic_agreement
    );

    let gap = 1.0 - floor;
    let validity = left
        .lexicon_coverage
        .min(right.lexicon_coverage)
        .clamp(0.0, 1.0);
    let mismatch = if language_agreement(&left, &right) > 0.5 {
        0.0
    } else {
        1.0
    };
    let expected_conflict =
        (weighted * gap * validity * mismatch * result.cross_script_pair).clamp(0.0, 1.0);
    assert!(
        (result.transliteration_semantic_conflict - expected_conflict).abs() < 1e-9,
        "conflict formula drift: {} vs {expected_conflict}",
        result.transliteration_semantic_conflict
    );

    // Same-script pairs never use the cross-script interactions.
    let latin = engine.compare("habibi", "habiba").unwrap();
    assert_eq!(latin.cross_script_pair, 0.0);
    assert_eq!(latin.transliteration_semantic_agreement, 0.0);
    assert_eq!(latin.transliteration_semantic_conflict, 0.0);
}

/// Transformer vectors drive the semantic ANN channel (§7): the index
/// adopts the transformer model identity, serves ANN candidates for
/// same-model queries, excludes wrong-dimension vectors instead of mixing
/// them, and rebuilds deterministically.
#[cfg(feature = "ann-hnsw")]
#[test]
fn transformer_vectors_drive_semantic_ann() {
    use textintel::core::providers::VectorStore;
    use textintel::storage::MemoryStore;

    let engine = transformer_engine();
    let mut store = MemoryStore::default();
    for (id, text) in [
        ("doc-a", "the river flows north quietly"),
        ("doc-b", "quantum entanglement links particles"),
        ("doc-c", "baking sourdough bread at dawn"),
    ] {
        store
            .upsert(id.to_string(), engine.analyze(text).unwrap())
            .unwrap();
    }
    let live = store.enable_ann(16, 100).unwrap();
    assert_eq!(live, 3, "all transformer vectors must index");
    let capabilities = store.store_capabilities();
    assert!(capabilities.ann_enabled);
    assert_eq!(capabilities.ann_dimensions, Some(16));
    assert_eq!(capabilities.ann_entries, 3);
    assert_eq!(
        capabilities.ann_model.as_deref(),
        Some("textintel-mini-bert-fixture@fixture-1"),
        "ANN must expose the transformer model identity"
    );

    let query = engine.analyze("the river flows north quietly").unwrap();
    let candidates = store.search_candidates_with_metadata(&query, 2);
    assert!(
        candidates.channels.contains(&"semantic_ann".to_string()),
        "same-model query must serve the ANN channel: {:?}",
        candidates.channels
    );

    // Wrong-dimension vectors are excluded, never mixed into the index.
    let mut narrow = MemoryStore::default();
    for (id, text) in [
        ("doc-a", "the river flows north"),
        ("doc-b", "baking bread"),
    ] {
        narrow
            .upsert(id.to_string(), engine.analyze(text).unwrap())
            .unwrap();
    }
    let excluded = narrow.enable_ann(8, 100).unwrap();
    assert_eq!(excluded, 0, "16-wide vectors must not join an 8-wide index");

    // Rebuild keeps the transformer model tag and the live entries.
    let rebuilt = store.rebuild_ann().unwrap();
    assert_eq!(rebuilt, 3);
    assert_eq!(
        store.store_capabilities().ann_model.as_deref(),
        Some("textintel-mini-bert-fixture@fixture-1")
    );
}

#[test]
fn parallel_encode_matches_sequential_bit_for_bit() {
    // Worker counts 1 (sequential), 2, 3, 4, and 8 (more workers than
    // texts) must agree exactly: same vectors, truncation flags, and
    // token counts in input order. Mixed lengths plus an empty string
    // and duplicates exercise grouping, order restoration, and sharing.
    let texts = [
        "the river flows north",
        "",
        "baking bread",
        "the river flows north",
        "a considerably longer piece of text with many more tokens in it",
        "z",
    ]
    .map(str::to_string);
    let sequential = TransformerEmbeddingProvider::open(FIXTURE)
        .expect("fixture must open")
        .with_max_parallel(1)
        .encode_detailed(&texts)
        .expect("sequential encode");
    for workers in [1, 2, 3, 4, 8] {
        let parallel = TransformerEmbeddingProvider::open(FIXTURE)
            .expect("fixture must open")
            .with_max_parallel(workers)
            .encode_detailed(&texts)
            .expect("parallel encode");
        assert_eq!(
            parallel.truncated, sequential.truncated,
            "workers={workers}"
        );
        assert_eq!(
            parallel.token_counts, sequential.token_counts,
            "workers={workers}"
        );
        assert_eq!(
            parallel.vectors.len(),
            sequential.vectors.len(),
            "workers={workers}"
        );
        for (index, (left, right)) in parallel
            .vectors
            .iter()
            .zip(sequential.vectors.iter())
            .enumerate()
        {
            let left_bits: Vec<u32> = left.iter().map(|value| value.to_bits()).collect();
            let right_bits: Vec<u32> = right.iter().map(|value| value.to_bits()).collect();
            assert_eq!(left_bits, right_bits, "workers={workers} text={index}");
        }
    }
    // Small max_batch forces several chunks (including a singleton,
    // which stays sequential) through the same fan-out.
    let chunked = TransformerEmbeddingProvider::open(FIXTURE)
        .expect("fixture must open")
        .with_max_batch(2)
        .with_max_parallel(4)
        .encode_detailed(&texts)
        .expect("chunked parallel encode");
    assert_eq!(chunked.truncated, sequential.truncated);
    assert_eq!(chunked.token_counts, sequential.token_counts);
    assert_eq!(chunked.vectors.len(), sequential.vectors.len());
    for (left, right) in chunked.vectors.iter().zip(sequential.vectors.iter()) {
        let left_bits: Vec<u32> = left.iter().map(|value| value.to_bits()).collect();
        let right_bits: Vec<u32> = right.iter().map(|value| value.to_bits()).collect();
        assert_eq!(left_bits, right_bits);
    }
}
