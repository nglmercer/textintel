use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use textintel::evaluation::{evaluate, EvaluationDataset};
use textintel::phonetic::{articulatory_distance, parse_ipa};
use textintel::{
    EmbeddingProvider, EngineConfig, Pattern, ProviderCapabilities, ProviderError, ResourceLimits,
    ResourceLoader, SimilarityProfile, TextIntelligence,
};

#[derive(Debug, Clone)]
struct CountingEmbedding {
    calls: Arc<AtomicUsize>,
}

impl EmbeddingProvider for CountingEmbedding {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(texts.iter().map(|_| vec![1.0, 0.0]).collect())
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("counting").with_dimensions(2)
    }

    fn model_metadata(&self) -> Option<textintel::ModelMetadata> {
        Some(textintel::ModelMetadata {
            model_id: "counting-v1".to_string(),
            revision: Some("test".to_string()),
            dimensions: 2,
            normalized: true,
            languages: vec!["multilingual".to_string()],
            source: Some("test".to_string()),
            license: Some("MIT".to_string()),
        })
    }
}

#[test]
fn batch_analysis_uses_one_embedding_batch_and_preserves_views() {
    let calls = Arc::new(AtomicUsize::new(0));
    let engine = TextIntelligence::new(EngineConfig {
        semantic: true,
        ..EngineConfig::default()
    })
    .with_embedding_provider(CountingEmbedding {
        calls: calls.clone(),
    });
    let values = engine
        .analyze_batch(&["first".to_string(), "second".to_string()])
        .unwrap();
    assert_eq!(values.len(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(values[0].semantic_embeddings.contains_key("default"));
    assert_eq!(
        values[0].metadata.get("semantic_model"),
        Some(&"counting-v1".to_string())
    );
    assert_eq!(
        values[0].schema_version,
        textintel::FINGERPRINT_SCHEMA_VERSION
    );
}

#[test]
fn persistent_store_round_trips_versioned_fingerprints() {
    let path =
        std::env::temp_dir().join(format!("textintel-production-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let engine = TextIntelligence::default().with_json_store(&path).unwrap();
    engine.add_document("one", "gana dinero").unwrap();
    assert_eq!(engine.document_count().unwrap(), 1);
    drop(engine);
    let reopened = TextIntelligence::default().with_json_store(&path).unwrap();
    assert_eq!(reopened.document_count().unwrap(), 1);
    assert_eq!(
        reopened.find_similar("gana dinero", 1).unwrap()[0].id,
        "one"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn multilingual_no_space_segments_and_ipa_are_supported() {
    let engine = TextIntelligence::default();
    let chinese = engine.analyze("你好世界").unwrap();
    assert_eq!(chinese.top_language(), Some("zh"));
    assert_eq!(chinese.segments.len(), 1);
    assert_eq!(parse_ipa("ˈkæt"), vec!["k", "æ", "t"]);
    assert!(articulatory_distance("p", "b") < articulatory_distance("p", "a"));
}

#[test]
fn pattern_options_negatives_profiles_and_resource_limits_are_typed() {
    let engine = TextIntelligence::default();
    engine
        .add_pattern_with_options(Pattern {
            id: "prize".to_string(),
            examples: vec!["win a prize".to_string()],
            negative_examples: vec!["buy a prize".to_string()],
            threshold: 0.75,
            languages: vec!["en".to_string()],
            tags: vec!["promotion".to_string()],
            enabled_channels: vec!["character".to_string(), "lexical".to_string()],
        })
        .unwrap();
    assert!(engine.match_patterns("buy a prize").unwrap().is_empty());
    let profile = SimilarityProfile::general_similarity().with_calibration(0.2, 1.0);
    let configured = TextIntelligence::default().with_similarity_profile(profile);
    assert!(configured.compare("same", "same").unwrap().score > 0.5);

    let mut limited = ResourceLoader::with_limits(ResourceLimits {
        max_resource_bytes: 8,
        ..ResourceLimits::default()
    });
    let error = limited
        .load_language_json(r#"{"language":"xx","words":["large"]}"#, "<test>")
        .unwrap_err();
    assert!(error.to_string().contains("maximum is 8"));
}

#[test]
fn resource_hashes_are_self_contained_and_format_independent() {
    let mut pack = serde_json::json!({
        "schema_version": 1,
        "language": "EN",
        "words": ["example"],
        "revision": "test"
    });
    let canonical = serde_json::to_vec(&pack).unwrap();
    let digest = format!("{:x}", Sha256::digest(canonical));
    pack["sha256"] = serde_json::Value::String(digest);
    let source = serde_json::to_string_pretty(&pack).unwrap();

    let mut loader = ResourceLoader::default();
    loader.load_language_json(&source, "<hashed-pack>").unwrap();
    assert!(loader.contains_in_language("example", "en"));
}

#[test]
fn evaluation_reports_ranking_and_calibration_metrics() {
    let source = include_str!("../data/evaluation.json");
    let dataset = EvaluationDataset::from_json(source).unwrap();
    let report = evaluate(&TextIntelligence::default(), &dataset).unwrap();
    assert_eq!(report.metrics.count, dataset.cases.len());
    assert!(report.metrics.roc_auc >= 0.0 && report.metrics.roc_auc <= 1.0);
    assert!(report.average_compare_micros.is_finite());
}
