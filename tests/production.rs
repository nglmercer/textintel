use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use textintel::evaluation::{
    evaluate_with_options, EvaluateOptions, EvaluationCase, EvaluationDataset,
};
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
    let snapshot_path = std::env::temp_dir().join(format!(
        "textintel-patterns-production-{}.json",
        std::process::id()
    ));
    engine.save_patterns_to(&snapshot_path).unwrap();
    let restored = TextIntelligence::default();
    assert_eq!(restored.load_patterns_from(&snapshot_path).unwrap(), 1);
    let definitions = restored.pattern_definitions().unwrap();
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].id, "prize");
    assert_eq!(definitions[0].tags, vec!["promotion".to_string()]);
    // Reloaded patterns must behave exactly like the originals.
    for query in ["win a prize", "buy a prize", "win a huge prize today"] {
        assert_eq!(
            restored.match_patterns(query).unwrap(),
            engine.match_patterns(query).unwrap()
        );
    }
    assert!(restored.load_patterns_from(&snapshot_path).is_ok());
    let _ = std::fs::remove_file(&snapshot_path);
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
fn evaluation_chunks_batches_larger_than_max_batch_size() {
    // Regression test: more pairs than `max_batch_size` (and more ranking
    // documents than the batch limit) must be evaluated in chunks instead of
    // failing with `batch size N exceeds max_batch_size`. A tiny batch
    // limit keeps this regression fast: 20 pairs over a limit of 8 still
    // exercises multi-chunk pairwise and ranking evaluation.
    let mut cases = Vec::new();
    for index in 0..20 {
        let similar = index % 2 == 0;
        cases.push(EvaluationCase {
            id: format!("chunk_{index:03}"),
            a: format!("case number {index} example text"),
            b: if similar {
                format!("case number {index} example text")
            } else {
                format!("entirely unrelated content {index} zzzqqq")
            },
            languages: Vec::new(),
            split: "test".to_string(),
            difficulty: "medium".to_string(),
            category: String::new(),
            labels: [(String::from("similar"), similar)].into_iter().collect(),
            expected: Default::default(),
            tags: Vec::new(),
        });
    }
    let dataset = EvaluationDataset {
        version: "chunk-test".to_string(),
        cases,
    };
    let engine = TextIntelligence::new(EngineConfig {
        max_batch_size: 8,
        ..EngineConfig::default()
    });
    let options = EvaluateOptions {
        split: None,
        ranking_queries: 5,
        ranking_documents: 20,
        spam_corpus: None,
    };
    let report = evaluate_with_options(&engine, &dataset, &options).unwrap();
    assert_eq!(report.metrics.count, 20);
    // The `ranking_queries` cap counts measurable queries (groups with a
    // similar-labelled `b`): the first five are indices 0, 2, 4, 6, 8.
    assert_eq!(report.ranking.queries, 5);
}

#[test]
fn trained_similarity_artifact_loads_and_separates_pairs() {
    use textintel::SimilarityModelArtifact;
    use textintel::SimilarityScorer;

    let source = include_str!("../models/similarity-v4.json");
    let artifact = SimilarityModelArtifact::from_json(source).unwrap();
    assert_eq!(artifact.kind, "logistic_similarity");
    assert_eq!(
        artifact.feature_schema_version,
        textintel::TRAINING_FEATURE_SCHEMA_VERSION
    );
    assert!(artifact.metrics["test_roc_auc"] >= 0.9);
    let engine = TextIntelligence::default();
    let scorer = artifact.to_scorer();
    let same_left = engine.analyze("compra ahora").unwrap();
    let same_right = engine.analyze("compra ahora").unwrap();
    let other = engine.analyze("see you tomorrow").unwrap();
    let same = scorer.score(&same_left, &same_right).score;
    let different = scorer.score(&same_left, &other).score;
    assert!(
        same > different,
        "trained scorer must rank identical pairs first"
    );
    assert!(same > 0.5);
}

#[test]
fn trained_spam_artifact_loads_and_separates_messages() {
    use textintel::SpamModelArtifact;
    use textintel::SpamPredictor;

    let source = include_str!("../models/spam-v1.json");
    let artifact = SpamModelArtifact::from_json(source).unwrap();
    assert_eq!(artifact.kind, "logistic_spam");
    assert_eq!(
        artifact.feature_schema_version,
        textintel::SPAM_FEATURE_SCHEMA_VERSION
    );
    assert!(artifact.calibrated);
    let engine = TextIntelligence::default();
    let predictor = artifact.to_predictor();
    let spam = engine
        .analyze("LAST CHANCE!!! Your $500 bonus expires tonight, activate here: https://bonus-activate.example.com/go !!!")
        .unwrap();
    let ham = engine
        .analyze("could you review the attached notes when you have a moment")
        .unwrap();
    let spam_result = predictor.predict(&spam, &[]).unwrap();
    let ham_result = predictor.predict(&ham, &[]).unwrap();
    assert!(
        spam_result.probability > ham_result.probability,
        "trained predictor must rank blatant spam above plain ham"
    );
    assert_eq!(spam_result.labels, vec!["spam".to_string()]);
    assert!(spam_result.calibrated);
    assert!(!spam_result.reasons.is_empty());
    // Heuristic fallback stays the engine default.
    let default = engine
        .detect_spam("could you review the attached notes when you have a moment")
        .unwrap();
    assert!(!default.model.starts_with("trained-spam:"));
}

fn synthetic_metrics_case(id: &str, a: &str, b: &str, similar: bool) -> EvaluationCase {
    EvaluationCase {
        id: id.to_string(),
        a: a.to_string(),
        b: b.to_string(),
        languages: Vec::new(),
        split: "test".to_string(),
        difficulty: "medium".to_string(),
        category: String::new(),
        labels: [(String::from("similar"), similar)].into_iter().collect(),
        expected: Default::default(),
        tags: Vec::new(),
    }
}

#[test]
fn evaluation_reports_ranking_and_calibration_metrics() {
    // Synthetic extremes (identical vs topically disjoint) keep this fast:
    // the default engine scores identicals near 1.0 through lexical and
    // character channels and disjoint pairs near 0.0, exercising binary,
    // calibration, and ranking metrics without the full dataset (the
    // real-data path is guarded by
    // `evaluation_dataset_from_dir_loads_splits_and_evaluates`, and
    // full-dataset production numbers come from the `eval` CLI).
    // Similar cases come first so ranking queries 0..4 are all relevant.
    let similar_texts = [
        "the quick brown fox jumps over the lazy dog",
        "pack my box with five dozen liquor jugs",
        "how vexingly quick daft zebras jump",
        "the five boxing wizards jump quickly",
        "jackdaws love my big sphinx of quartz",
        "weave a circle round him thrice",
    ];
    let mut cases = Vec::new();
    for (index, text) in similar_texts.iter().enumerate() {
        cases.push(synthetic_metrics_case(
            &format!("syn_sim_{index:02}"),
            text,
            text,
            true,
        ));
    }
    let dissimilar_pairs = [
        (
            "quantum field theory renormalization lecture",
            "medieval sourdough bread baking recipes",
        ),
        (
            "orbital mechanics transfer window calculation",
            "handmade ceramic pottery glazing techniques",
        ),
        (
            "bayesian posterior sampling diagnostics report",
            "vintage motorcycle carburetor restoration guide",
        ),
        (
            "distributed consensus protocol safety proof",
            "alpine wildflower meadow hiking itinerary",
        ),
        (
            "phonetic transcription of tonal contrasts",
            "deep sea hydrothermal vent ecosystems",
        ),
        (
            "zero knowledge succinct argument circuits",
            "heirloom tomato seed saving workshop",
        ),
    ];
    for (index, (left, right)) in dissimilar_pairs.iter().enumerate() {
        cases.push(synthetic_metrics_case(
            &format!("syn_dis_{index:02}"),
            left,
            right,
            false,
        ));
    }
    let dataset = EvaluationDataset {
        version: "synthetic-metrics".to_string(),
        cases,
    };
    let options = EvaluateOptions {
        split: None,
        ranking_queries: 4,
        ranking_documents: 12,
        spam_corpus: None,
    };
    let report = evaluate_with_options(&TextIntelligence::default(), &dataset, &options).unwrap();
    assert_eq!(report.metrics.count, 12);
    assert_eq!(report.metrics.positives, 6);
    assert_eq!(report.metrics.negatives, 6);
    assert!(report.metrics.roc_auc >= 0.0 && report.metrics.roc_auc <= 1.0);
    assert!(report.metrics.expected_calibration_error >= 0.0);
    assert!(report.metrics.brier >= 0.0);
    assert_eq!(report.ranking.queries, 4);
    assert!(report.average_compare_micros.is_finite());
}

#[test]
fn ranking_groups_duplicate_queries_with_graded_relevance() {
    // Regression test: cases sharing one `a` text share one ranking, so they
    // must form ONE query whose relevant set is every similar-labelled `b`.
    // One-query-per-case would force the k variants onto ranks 1..k and cap
    // a perfect ranker at MRR = H_k/k — measuring family size, not quality.
    let mut cases = Vec::new();
    let variants = [
        "the quick brown fox jumps over",
        "the quick brown fox jumps over!",
        "the quick brown fox jumps ovver",
        "the quick brown fox jumps  over",
        "the quick brown fox jumps over.",
        "the quick brown fox jumps oveer",
        "the quick brown fox jumps ovre",
        "the quick brown fox jumps overr",
        "the quick brown fox jumps ove",
        "the quick brown fox jumps overr!",
        "the quick brown fox jumps over!!",
        "the quick brown fox jumps over?",
    ];
    for (index, variant) in variants.iter().enumerate() {
        cases.push(synthetic_metrics_case(
            &format!("dup_{index:02}"),
            "the quick brown fox jumps over",
            variant,
            true,
        ));
    }
    for (index, (left, right)) in [
        (
            "quantum field theory renormalization lecture",
            "medieval sourdough bread baking recipes",
        ),
        (
            "orbital mechanics transfer window calculation",
            "handmade ceramic pottery glazing techniques",
        ),
    ]
    .iter()
    .enumerate()
    {
        cases.push(synthetic_metrics_case(
            &format!("dis_{index:02}"),
            left,
            right,
            false,
        ));
    }
    let dataset = EvaluationDataset {
        version: "dup-query-test".to_string(),
        cases,
    };
    let options = EvaluateOptions {
        split: None,
        ranking_queries: 12,
        ranking_documents: 20,
        spam_corpus: None,
    };
    let report = evaluate_with_options(&TextIntelligence::default(), &dataset, &options).unwrap();
    assert_eq!(report.ranking.queries, 1);
    assert_eq!(report.ranking.recall_at_10, 1.0);
    assert_eq!(report.ranking.mrr, 1.0);
}

#[test]
fn evaluation_dataset_from_dir_loads_splits_and_evaluates() {
    // Integration guard for the real-data path: the sharded dataset loads
    // with its version and splits intact, and a small slice evaluates.
    let dataset = EvaluationDataset::from_dir("data/evaluation").unwrap();
    assert_eq!(dataset.version, "0.7.0");
    let mut splits = std::collections::BTreeSet::new();
    for case in &dataset.cases {
        splits.insert(case.split.as_str());
    }
    assert_eq!(
        splits,
        ["test", "train", "validation"].into_iter().collect()
    );
    assert!(dataset.cases.len() > 1000);
    let slice = EvaluationDataset {
        version: dataset.version.clone(),
        cases: dataset
            .cases
            .iter()
            .filter(|case| case.split == "test")
            .take(10)
            .cloned()
            .collect(),
    };
    assert_eq!(slice.cases.len(), 10);
    let options = EvaluateOptions {
        split: None,
        ranking_queries: 2,
        ranking_documents: 10,
        spam_corpus: None,
    };
    let report = evaluate_with_options(&TextIntelligence::default(), &slice, &options).unwrap();
    assert_eq!(report.metrics.count, 10);
}
