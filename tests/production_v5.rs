//! Production v5 behaviors: explicit semantic fallback, revision-aware
//! caches, entity evidence, contextual semantic features, transliteration
//! false-friend gating, bounded multi-channel retrieval, and ANN revision
//! validation.
//!
//! Uses synthetic strings only (never dataset content) plus controlled
//! [`StaticEmbeddingProvider`] vectors so each signal is inspectable in
//! isolation.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use textintel::core::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
use textintel::core::providers::{EmbeddingProvider, VectorStore};
use textintel::semantic::{
    CachedEmbeddingProvider, FallbackEmbeddingProvider, FeatureHashEmbeddingProvider,
};
use textintel::storage::MemoryStore;
use textintel::{EngineConfig, TextIntelligence};

/// Controlled semantic backend: every text maps to one fixed vector, so
/// pair cosine is exactly known in isolation tests.
#[derive(Debug, Clone)]
struct ConstEmbedding {
    vector: Vec<f32>,
}

impl EmbeddingProvider for ConstEmbedding {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, textintel::ProviderError> {
        Ok(texts.iter().map(|_| self.vector.clone()).collect())
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("const_test_embedding")
            .with_dimensions(self.vector.len())
            .with_quality(CapabilityLevel::Basic)
    }
    fn model_metadata(&self) -> Option<ModelMetadata> {
        Some(ModelMetadata {
            model_id: "const-test-v1".to_string(),
            revision: Some("test".to_string()),
            dimensions: self.vector.len(),
            normalized: true,
            languages: vec!["multilingual".to_string()],
            source: Some("test".to_string()),
            license: Some("MIT".to_string()),
        })
    }
}

fn semantic_engine(vector: Vec<f32>) -> TextIntelligence {
    TextIntelligence::new(EngineConfig {
        semantic: true,
        phonetic: true,
        ..EngineConfig::default()
    })
    .with_embedding_provider(ConstEmbedding { vector })
}

#[test]
fn fallback_serves_primary_until_it_errors() {
    #[derive(Debug)]
    struct FailingProvider;
    impl EmbeddingProvider for FailingProvider {
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, textintel::ProviderError> {
            let _ = texts;
            Err(textintel::ProviderError::new("failing", "boom"))
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities::new("failing_primary")
                .with_dimensions(2)
                .with_quality(CapabilityLevel::Production)
        }
        fn model_metadata(&self) -> Option<ModelMetadata> {
            Some(ModelMetadata {
                model_id: "failing-v1".to_string(),
                revision: Some("r1".to_string()),
                dimensions: 2,
                normalized: true,
                languages: vec!["multilingual".to_string()],
                source: None,
                license: None,
            })
        }
    }

    let provider = FallbackEmbeddingProvider::new(
        FailingProvider,
        FeatureHashEmbeddingProvider::new(2).unwrap(),
    );
    assert!(!provider.is_degraded());
    let vectors = provider.embed(&["hello".to_string()]).unwrap();
    assert_eq!(vectors.len(), 1);
    assert_eq!(vectors[0].len(), 2);
    assert!(provider.is_degraded());
    // Degradation is explicit in capabilities and metadata.
    let capabilities = provider.capabilities();
    assert_eq!(capabilities.provider, "feature_hash_embedding");
    assert!(
        capabilities
            .fallback
            .as_deref()
            .unwrap_or_default()
            .contains("failing_primary")
    );
    assert_eq!(
        provider.model_metadata().map(|metadata| metadata.model_id),
        Some("feature-hash-v1".to_string())
    );
}

#[test]
fn fallback_triggers_on_dimension_mismatch() {
    #[derive(Debug)]
    struct LyingProvider;
    impl EmbeddingProvider for LyingProvider {
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, textintel::ProviderError> {
            Ok(texts.iter().map(|_| vec![1.0, 0.0, 0.0]).collect())
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities::new("lying_primary")
                .with_dimensions(2)
                .with_quality(CapabilityLevel::Production)
        }
        fn model_metadata(&self) -> Option<ModelMetadata> {
            Some(ModelMetadata {
                model_id: "lying-v1".to_string(),
                revision: None,
                dimensions: 2,
                normalized: true,
                languages: Vec::new(),
                source: None,
                license: None,
            })
        }
    }

    let provider = FallbackEmbeddingProvider::new(
        LyingProvider,
        FeatureHashEmbeddingProvider::new(2).unwrap(),
    );
    // Three-wide vectors against two-wide metadata: the primary is rejected
    // and the fallback serves instead of erroring the analysis.
    let vectors = provider.embed(&["hello".to_string()]).unwrap();
    assert_eq!(vectors[0].len(), 2);
    assert!(provider.is_degraded());
}

#[test]
fn cache_invalidates_when_fallback_degrades() {
    #[derive(Debug)]
    struct FlakyProvider {
        broken: AtomicBool,
    }
    impl EmbeddingProvider for FlakyProvider {
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, textintel::ProviderError> {
            if self.broken.load(Ordering::SeqCst) {
                return Err(textintel::ProviderError::new("flaky", "now broken"));
            }
            Ok(texts.iter().map(|_| vec![1.0, 0.0]).collect())
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities::new("flaky_primary")
                .with_dimensions(2)
                .with_quality(CapabilityLevel::Production)
        }
        fn model_metadata(&self) -> Option<ModelMetadata> {
            Some(ModelMetadata {
                model_id: "flaky-v1".to_string(),
                revision: Some("r1".to_string()),
                dimensions: 2,
                normalized: true,
                languages: Vec::new(),
                source: None,
                license: None,
            })
        }
    }

    let flaky = Arc::new(FlakyProvider {
        broken: AtomicBool::new(false),
    });
    struct SharedFlaky(Arc<FlakyProvider>);
    impl std::fmt::Debug for SharedFlaky {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.debug_struct("SharedFlaky").finish()
        }
    }
    impl EmbeddingProvider for SharedFlaky {
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, textintel::ProviderError> {
            self.0.embed(texts)
        }
        fn capabilities(&self) -> ProviderCapabilities {
            self.0.capabilities()
        }
        fn model_metadata(&self) -> Option<ModelMetadata> {
            self.0.model_metadata()
        }
    }
    let cached = CachedEmbeddingProvider::new(
        FallbackEmbeddingProvider::new(
            SharedFlaky(flaky.clone()),
            FeatureHashEmbeddingProvider::new(2).unwrap(),
        ),
        16,
    );
    let first = cached.embed(&["alpha".to_string()]).unwrap();
    assert_eq!(first[0], vec![1.0, 0.0]);
    assert_eq!(cached.diagnostics().invalidations, 0);
    flaky.broken.store(true, Ordering::SeqCst);
    // The first post-failure call serves the fallback inline; the metadata
    // switch (flaky-v1 → feature-hash-v1) is observed on the next call,
    // which invalidates the cache revision so stale primary vectors are
    // never served again.
    let second = cached.embed(&["beta".to_string()]).unwrap();
    assert_eq!(second[0].len(), 2);
    assert_ne!(second[0], vec![1.0, 0.0]);
    let _ = cached.embed(&["gamma".to_string()]).unwrap();
    assert!(cached.diagnostics().invalidations >= 1);
}

#[test]
fn cross_language_semantic_routes_by_language_scope() {
    let engine = semantic_engine(vec![1.0, 0.0]);
    let result = engine.compare("hello world", "hola mundo").unwrap();
    assert_eq!(result.semantic, Some(1.0));
    assert_eq!(result.cross_language_semantic, 1.0);
    // Static vectors are Basic quality, so transformer-only evidence stays 0.
    assert_eq!(result.contextual_semantic, 0.0);

    let same = semantic_engine(vec![1.0, 0.0]);
    let same_result = same.compare("hello world", "hello there").unwrap();
    assert_eq!(same_result.cross_language_semantic, 0.0);
}

#[test]
fn semantic_without_lexical_overlap_needs_low_overlap() {
    let engine = semantic_engine(vec![0.0, 1.0]);
    let result = engine.compare("alpha beta", "gamma delta").unwrap();
    assert!(result.lexical.unwrap_or(1.0) < 0.3);
    assert_eq!(result.semantic_without_lexical_overlap, 1.0);

    let overlapping = semantic_engine(vec![0.0, 1.0]);
    let overlap_result = overlapping
        .compare("alpha beta", "alpha beta gamma")
        .unwrap();
    assert!(overlap_result.lexical.unwrap_or(0.0) >= 0.3);
    assert_eq!(overlap_result.semantic_without_lexical_overlap, 0.0);
}

#[test]
fn entities_agree_conflict_and_vanish_cleanly() {
    let engine = TextIntelligence::default();
    let left = engine.analyze("parcel 3310 arrived").unwrap();
    assert!(
        left.entities
            .iter()
            .any(|mention| mention.entity_type == "number" && mention.value == "3310")
    );
    let same = engine
        .compare("parcel 3310 arrived", "parcel 3310 arrived")
        .unwrap();
    assert_eq!(same.entity_agreement, 1.0);
    assert_eq!(same.entity_conflict, 0.0);

    let swapped = engine
        .compare("parcel 3310 arrived", "parcel 7721 arrived")
        .unwrap();
    assert_eq!(swapped.entity_agreement, 0.0);
    assert_eq!(swapped.entity_conflict, 1.0);

    // Missing evidence on either side never penalizes.
    let missing = engine
        .compare("parcel 3310 arrived", "the parcel arrived")
        .unwrap();
    assert_eq!(missing.entity_agreement, 0.0);
    assert_eq!(missing.entity_conflict, 0.0);
}

#[test]
fn capitalized_common_words_are_not_persons() {
    // Regression test: sentence capitalization (`Where`, `Thanks`) used to
    // extract as person mentions, manufacturing entity conflicts between
    // translations. The lexicon-backed extractor skips common words while
    // keeping genuine names.
    let engine = TextIntelligence::default();
    let common = engine.analyze("Where is the station").unwrap();
    assert!(
        common
            .entities
            .iter()
            .all(|mention| mention.entity_type != "person"),
        "capitalized common words must not extract as persons: {:?}",
        common.entities
    );
    let named = engine.analyze("lunch with Alice today").unwrap();
    assert!(
        named
            .entities
            .iter()
            .any(|mention| mention.entity_type == "person" && mention.value == "alice"),
        "genuine names must still extract: {:?}",
        named.entities
    );
    let pair = engine
        .compare("Where is the station", "Thanks for the map")
        .unwrap();
    assert_eq!(pair.entity_conflict, 0.0);
}

#[test]
fn transliteration_false_friends_are_discounted_not_destroyed() {
    // Programmatic Latin/Cyrillic lookalike (never dataset content): `cop`
    // against Cyrillic Es-O-Er (`сор`, "litter"). Language and validity are
    // set explicitly so the formula is pinned without depending on the
    // detector: look-alikes surface through transliteration views, never at
    // face value, and the validity signature discounts valid-but-different
    // words further.
    let cyrillic = format!(
        "{}{}{}",
        '\u{441}', // с
        '\u{43E}', // о
        '\u{440}', // р
    );
    let engine = TextIntelligence::default();
    let mut left = engine.analyze("cop").unwrap();
    let mut right = engine.analyze(&cyrillic).unwrap();
    assert!(!left.transliteration_views().is_empty() || !right.transliteration_views().is_empty());
    left.language_candidates = vec![textintel::LanguageCandidate::new("en", 1.0)];
    right.language_candidates = vec![textintel::LanguageCandidate::new("ru", 1.0)];
    left.lexicon_coverage = 1.0;
    right.lexicon_coverage = 1.0;
    let compatibility = textintel::transliteration_compatibility(&left, &right);
    // Language mismatch (0.7) × no-production-semantic (1.0) × no entities
    // (1.0) × both-valid-words (0.5).
    assert!(
        (compatibility - 0.35).abs() < 1e-9,
        "false-friend compatibility should be 0.35: {compatibility}"
    );
    let evidence = textintel::transliteration_evidence(&left, &right).unwrap();
    let effective = textintel::effective_transliteration_evidence(&left, &right).unwrap();
    assert!(
        (effective - evidence.weighted() * compatibility).abs() < 1e-9,
        "effective = similarity × confidence × compatibility: {effective}"
    );
    assert!(effective < evidence.weighted());

    // The same pair with one side lexicon-invalid (a genuine romanization):
    // the validity cut lifts, leaving only the mild language factor.
    right.lexicon_coverage = 0.0;
    let genuine = textintel::transliteration_compatibility(&left, &right);
    assert!(
        (genuine - 0.7).abs() < 1e-9,
        "genuine-pair compatibility should be 0.7: {genuine}"
    );
}

#[test]
fn transliteration_semantic_interactions_gate_on_cross_script() {
    // Same-script pair with transliteration views (Latin→X byproducts):
    // interactions stay 0; only genuine cross-script links qualify.
    let engine = TextIntelligence::default();
    let same_script = engine.compare("complement", "compliment").unwrap();
    assert_eq!(same_script.cross_script_pair, 0.0);
    assert_eq!(same_script.transliteration_semantic_agreement, 0.0);
    assert_eq!(same_script.transliteration_semantic_conflict, 0.0);

    // Cross-script pair with identical controlled vectors: agreement carries
    // the weighted evidence, scaled by semantics.
    let xscript = semantic_engine(vec![1.0, 0.0]);
    let result = xscript.compare("privet", "привет").unwrap();
    assert_eq!(result.cross_script_pair, 1.0);
    assert!(result.transliteration_semantic_agreement > 0.0);
}

#[test]
fn candidate_union_deduplicates_ids() {
    let engine = TextIntelligence::default();
    let mut store = MemoryStore::default();
    for (id, text) in [
        ("a", "the quick brown fox"),
        ("b", "the quick brown fox jumps"),
        ("c", "unrelated zebra quantum"),
    ] {
        store
            .upsert(id.to_string(), engine.analyze(text).unwrap())
            .unwrap();
    }
    let query = engine.analyze("the quick brown fox").unwrap();
    let set = store.search_candidates_with_metadata(&query, 10);
    let mut ids: Vec<&str> = set.records.iter().map(|(id, _)| id.as_str()).collect();
    ids.sort_unstable();
    let before = ids.len();
    ids.dedup();
    assert_eq!(before, ids.len(), "candidate IDs must be deduplicated");
    assert!(!set.channels.is_empty());
}

#[test]
fn retrieval_is_bounded_per_channel() {
    let engine = TextIntelligence::default();
    let mut store = MemoryStore::default().with_retrieval_limits(2, 100);
    for index in 0..50 {
        store
            .upsert(
                format!("doc-{index:02}"),
                engine.analyze("common shared token").unwrap(),
            )
            .unwrap();
    }
    let query = engine.analyze("common shared token").unwrap();
    // Fifty documents share every posting list and the limit (40) exceeds
    // the capped union, so the union size proves the per-channel bound.
    let set = store.search_candidates_with_metadata(&query, 40);
    assert!(
        set.records.len() <= 2 * set.channels.len().max(1),
        "union must respect the per-channel bound: {} records via {:?}",
        set.records.len(),
        set.channels
    );
    assert!(
        set.records.len() < 40,
        "union should be far below the limit"
    );
}

#[test]
fn provider_degradation_is_explicit_in_diagnostics() {
    let engine = TextIntelligence::default()
        .with_embedding_provider(FeatureHashEmbeddingProvider::new(16).unwrap());
    let diagnostics = engine.diagnostics();
    assert_eq!(
        diagnostics.embedding.quality,
        CapabilityLevel::Basic,
        "feature hash must report Basic, never Production"
    );
    assert!(
        diagnostics
            .degraded
            .iter()
            .any(|note| note.capability == "semantic")
    );
    assert!(diagnostics.entities.is_some());
    assert_eq!(
        diagnostics.entities.as_ref().map(|info| info.quality),
        Some(CapabilityLevel::Basic)
    );
    // Diagnostics never carry user text: metadata holds counts and names.
    let rendered = serde_json::to_string(&diagnostics).unwrap();
    assert!(!rendered.contains("parcel"));
}

#[cfg(feature = "ann-hnsw")]
#[test]
fn ann_validates_revisions_and_skips_mismatched_queries() {
    use textintel::core::providers::VectorStore as _;

    let engine = TextIntelligence::new(EngineConfig {
        semantic: true,
        ..EngineConfig::default()
    })
    .with_embedding_provider(FeatureHashEmbeddingProvider::new(8).unwrap());
    let with_model = |text: &str, model: &str| {
        let mut fingerprint = engine.analyze(text).unwrap();
        fingerprint
            .metadata
            .insert("semantic_model".to_string(), model.to_string());
        fingerprint.metadata.remove("semantic_revision");
        fingerprint
    };
    let mut store = MemoryStore::default();
    store
        .upsert("a".to_string(), with_model("first document", "model-x"))
        .unwrap();
    store
        .upsert("b".to_string(), with_model("second document", "model-x"))
        .unwrap();
    store
        .upsert("c".to_string(), with_model("third document", "model-y"))
        .unwrap();
    // Fillers without embeddings force the union path (records > limit)
    // without joining the ANN snapshot.
    let plain = TextIntelligence::default();
    for index in 0..4 {
        store
            .upsert(
                format!("filler-{index}"),
                plain.analyze("filler text here").unwrap(),
            )
            .unwrap();
    }
    let live = store.enable_ann(8, 100).unwrap();
    assert_eq!(live, 2, "only the majority revision is indexed");
    let capabilities = store.store_capabilities();
    assert!(capabilities.ann_enabled);
    assert_eq!(capabilities.ann_dimensions, Some(8));
    assert_eq!(capabilities.ann_entries, 2);
    assert_eq!(capabilities.ann_model.as_deref(), Some("model-x"));

    // A query from the minority revision skips ANN instead of comparing
    // across revisions; fallback channels still serve.
    let minority_query = with_model("third document", "model-y");
    let minority = store.search_candidates_with_metadata(&minority_query, 2);
    assert!(!minority.channels.contains(&"semantic_ann".to_string()));
    let majority_query = with_model("first document", "model-x");
    let majority = store.search_candidates_with_metadata(&majority_query, 2);
    assert!(majority.channels.contains(&"semantic_ann".to_string()));
}

#[test]
fn v5_scorer_separates_entity_substitutions() {
    use textintel::{SimilarityModelArtifact, SimilarityScorer};

    let source = std::fs::read_to_string("models/similarity-v5.json").unwrap();
    let scorer = SimilarityModelArtifact::from_json(&source)
        .unwrap()
        .to_scorer();
    let engine = TextIntelligence::default();
    // Fresh vocabulary (never dataset content).
    let base = engine.analyze("cart 5520 checked out").unwrap();
    let same = engine.analyze("cart 5520 has checked out").unwrap();
    let swapped = engine.analyze("cart 8814 checked out").unwrap();
    let same_score = scorer.score(&base, &same).score;
    let swapped_score = scorer.score(&base, &swapped).score;
    assert!(
        same_score > swapped_score,
        "same entity ({same_score:.3}) must outscore substitution ({swapped_score:.3})"
    );
    assert!(same_score > 0.5, "same entity pair must match");
}

#[test]
fn v5_scorer_prefers_genuine_transliteration_over_lookalikes() {
    use textintel::{SimilarityModelArtifact, SimilarityScorer};

    let source = std::fs::read_to_string("models/similarity-v5.json").unwrap();
    let scorer = SimilarityModelArtifact::from_json(&source)
        .unwrap()
        .to_scorer();
    let engine = TextIntelligence::default();
    let genuine = scorer
        .score(
            &engine.analyze("privet").unwrap(),
            &engine.analyze("привет").unwrap(),
        )
        .score;
    let cyrillic = format!("{}{}{}", '\u{441}', '\u{43E}', '\u{440}');
    let lookalike = scorer
        .score(
            &engine.analyze("cop").unwrap(),
            &engine.analyze(&cyrillic).unwrap(),
        )
        .score;
    assert!(
        genuine > lookalike,
        "genuine transliteration ({genuine:.3}) must outscore the look-alike ({lookalike:.3})"
    );
}

#[cfg(feature = "semantic-transformer")]
#[test]
fn transformer_primary_serves_through_fallback_undegraded() {
    use textintel::TransformerEmbeddingProvider;
    use textintel::core::providers::EmbeddingProvider as _;

    let transformer =
        TransformerEmbeddingProvider::open("tests/fixtures/mini-transformer").unwrap();
    let dimensions = transformer.dimensions();
    let provider = FallbackEmbeddingProvider::new(
        transformer,
        FeatureHashEmbeddingProvider::new(dimensions).unwrap(),
    );
    let vectors = provider.embed(&["hello world".to_string()]).unwrap();
    assert_eq!(vectors[0].len(), dimensions);
    assert!(!provider.is_degraded());
    assert_eq!(
        provider.capabilities().quality,
        CapabilityLevel::Production,
        "healthy transformer must report Production"
    );
}
