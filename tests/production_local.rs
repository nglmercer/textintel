//! Production preset, builder, capability tiers, and diagnostics (§5, §6,
//! §13–§15, §60–§62).

use textintel::core::capabilities::CapabilityLevel;
use textintel::core::types::FINGERPRINT_SCHEMA_VERSION;
use textintel::engine::{EngineDiagnostics, TextIntelligence};

#[test]
fn production_local_builds_and_reports_graceful_fallbacks() {
    let engine = TextIntelligence::production_local().expect("production_local must not fail");
    // The preset enables the evidence channels that need backends.
    assert!(engine.config().semantic);
    assert!(engine.config().phonetic);
    // Analysis works in the degraded configuration.
    let fingerprint = engine.analyze("Fra🏠do").expect("analyze must succeed");
    assert!(!fingerprint.rebus_candidates.is_empty());

    let diagnostics = engine.diagnostics();
    assert_eq!(diagnostics.api_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(diagnostics.fingerprint_schema, FINGERPRINT_SCHEMA_VERSION);
    // espeak-ng is not installed in this environment: phonetic quality must
    // be reported as degraded, never silently presented as production.
    assert_eq!(diagnostics.g2p.provider, "rule_based_g2p");
    assert_eq!(diagnostics.g2p.quality, CapabilityLevel::Basic);
    assert!(
        diagnostics
            .degraded
            .iter()
            .any(|item| item.capability == "phonetic" && item.wanted == "espeak_ng_g2p"),
        "missing phonetic degradation note: {:?}",
        diagnostics.degraded
    );
    // The preset ships a local semantic baseline, honestly marked Basic.
    assert_eq!(diagnostics.embedding.provider, "feature_hash_embedding");
    assert_eq!(diagnostics.embedding.quality, CapabilityLevel::Basic);
    // Repository model artifacts load through the preset paths.
    assert_eq!(
        diagnostics
            .similarity
            .as_ref()
            .map(|info| info.provider.as_str()),
        Some("logistic_similarity_scorer")
    );
    assert_eq!(
        diagnostics.similarity.as_ref().map(|info| info.quality),
        Some(CapabilityLevel::Production)
    );
    assert_eq!(
        diagnostics.spam.provider.as_str(),
        "trained_spam",
        "preset should load models/spam-v1.json, got {:?}",
        diagnostics.spam
    );
}

#[test]
fn production_preset_prefers_similarity_v3() {
    // similarity-v3 (dataset 0.6.0, semantic + phonetic evidence) is the
    // production artifact; older revisions remain only as a fallback for old
    // checkouts.
    let engine = TextIntelligence::production_local().expect("production_local must not fail");
    let diagnostics = engine.diagnostics();
    let similarity = diagnostics
        .similarity
        .as_ref()
        .expect("production loads a similarity model");
    let source =
        std::fs::read_to_string("models/similarity-v3.json").expect("similarity-v3 must exist");
    let artifact = textintel::SimilarityModelArtifact::from_json(&source).unwrap();
    assert_eq!(similarity.provider, "logistic_similarity_scorer");
    assert_eq!(similarity.version.as_deref(), artifact.revision.as_deref());
    assert_eq!(artifact.dataset_version, "0.6.0");
    assert_ne!(
        artifact.weights.get("semantic").copied().unwrap_or(0.0),
        0.0,
        "v3 must carry useful semantic evidence"
    );
    assert_ne!(
        artifact.weights.get("phonetic").copied().unwrap_or(0.0),
        0.0,
        "v3 must carry useful phonetic evidence"
    );
    for metric in [
        "test_accuracy",
        "test_precision",
        "test_recall",
        "test_f1",
        "test_roc_auc",
        "test_pr_auc",
        "test_brier",
        "test_ece",
    ] {
        assert!(
            artifact.metrics.contains_key(metric),
            "v2 must record {metric}"
        );
    }
}

#[test]
fn diagnostics_cover_resources_and_missing_channels() {
    let engine = TextIntelligence::default();
    let diagnostics = engine.diagnostics();
    assert!(diagnostics.resource_languages.contains(&"es".to_string()));
    assert!(diagnostics
        .abbreviation_languages
        .contains(&"en".to_string()));
    for language in [
        "ar", "de", "en", "es", "fr", "hi", "id", "it", "ja", "ko", "nl", "pl", "pt", "ru", "tr",
        "zh",
    ] {
        assert!(
            diagnostics.symbol_languages.contains(&language.to_string()),
            "missing symbol coverage for {language}: {:?}",
            diagnostics.symbol_languages
        );
    }
    assert!(
        diagnostics
            .resource_manifest
            .iter()
            .any(|info| info.kind == "abbreviation" && info.language.as_deref() == Some("en")),
        "manifest must list abbreviation packs"
    );
    // The default engine has no embedding backend: reported unavailable and
    // listed as degraded, never scored as zero similarity.
    assert_eq!(diagnostics.embedding.quality, CapabilityLevel::Unavailable);
    assert!(
        diagnostics
            .degraded
            .iter()
            .any(|item| item.capability == "semantic"),
        "missing semantic degradation note"
    );
    assert!(
        diagnostics
            .degraded
            .iter()
            .any(|item| item.capability == "reranker"),
        "missing reranker degradation note"
    );
}

#[test]
fn diagnostics_expose_the_full_running_configuration() {
    let engine = TextIntelligence::production_local().expect("production_local must not fail");
    let diagnostics = engine.diagnostics();
    // Library version, fingerprint schema, resource revisions.
    assert_eq!(diagnostics.api_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(diagnostics.fingerprint_schema, FINGERPRINT_SCHEMA_VERSION);
    assert!(!diagnostics.resource_manifest.is_empty());
    // Providers: embedding (+model revision), G2P, transliteration,
    // similarity, spam, reranker.
    assert!(!diagnostics.embedding.provider.is_empty());
    assert!(!diagnostics.g2p.provider.is_empty());
    assert_eq!(
        diagnostics
            .transliteration
            .as_ref()
            .map(|info| info.provider.as_str()),
        Some("rule_based_transliteration")
    );
    assert!(diagnostics.similarity.is_some());
    assert_eq!(diagnostics.spam.provider, "trained_spam");
    // Store type, ANN status, cache status.
    assert_eq!(diagnostics.store_capabilities.store_type, "memory");
    assert_eq!(
        diagnostics.ann_enabled,
        diagnostics.store_capabilities.ann_enabled
    );
    for name in ["embeddings", "g2p", "language", "rebus"] {
        assert!(
            diagnostics.caches.contains_key(name),
            "missing {name} cache status"
        );
    }
    // Degraded capabilities are reported (possibly empty) and the whole
    // report round-trips through JSON.
    let _ = &diagnostics.degraded;
    let round_trip: EngineDiagnostics =
        serde_json::from_str(&serde_json::to_string(&diagnostics).unwrap()).unwrap();
    assert_eq!(round_trip, diagnostics);
}

#[test]
fn builder_rejects_invalid_configuration_without_panic() {
    let config = textintel::EngineConfig {
        max_input_length: 0,
        ..Default::default()
    };
    assert!(TextIntelligence::builder().config(config).build().is_err());
    assert!(TextIntelligence::try_new(textintel::EngineConfig {
        max_input_length: 0,
        ..Default::default()
    })
    .is_err());
}

#[test]
fn builder_model_paths_are_strict() {
    // Explicit paths: missing files fail, corrupt files fail.
    let missing = TextIntelligence::builder()
        .trained_similarity_model("models/does-not-exist.json")
        .build();
    assert!(missing.is_err(), "missing required model must fail");

    let corrupt = std::env::temp_dir().join("textintel-corrupt-similarity.json");
    std::fs::write(&corrupt, r#"{"artifact_version": 999}"#).expect("fixture write");
    let loaded = TextIntelligence::builder()
        .trained_similarity_model(&corrupt)
        .build();
    assert!(
        loaded.is_err(),
        "corrupt model must fail, not silently skipped"
    );
    let _ = std::fs::remove_file(&corrupt);

    // Valid explicit paths load trained backends.
    let engine = TextIntelligence::builder()
        .trained_similarity_model("models/similarity-v3.json")
        .trained_spam_model("models/spam-v1.json")
        .build()
        .expect("valid models must load");
    let diagnostics = engine.diagnostics();
    assert!(
        !diagnostics
            .degraded
            .iter()
            .any(|item| item.capability == "similarity"),
        "trained similarity must clear the degradation note"
    );
    assert!(
        !diagnostics
            .degraded
            .iter()
            .any(|item| item.capability == "spam"),
        "trained spam must clear the degradation note"
    );
}

#[test]
fn provider_tiers_are_honest() {
    use textintel::core::providers::{EmbeddingProvider, G2PProvider, SpamPredictor};
    use textintel::{FeatureHashEmbeddingProvider, NullEmbeddingProvider};
    use textintel::{HeuristicSpamPredictor, NullG2PProvider, RuleBasedG2PProvider};

    assert_eq!(
        NullEmbeddingProvider.capabilities().quality,
        CapabilityLevel::Unavailable
    );
    let hashed = FeatureHashEmbeddingProvider::new(64).expect("dimensions must be valid");
    assert_eq!(hashed.capabilities().quality, CapabilityLevel::Basic);

    assert_eq!(
        NullG2PProvider.capabilities().quality,
        CapabilityLevel::Unavailable
    );
    let rule_based = RuleBasedG2PProvider;
    let capabilities = G2PProvider::capabilities(&rule_based);
    assert_eq!(capabilities.quality, CapabilityLevel::Basic);
    assert!(capabilities.languages.contains(&"es".to_string()));

    // Unknown scripts get low confidence, never fabricated certainty.
    let latin = rule_based.phonemize("hola", "es").expect("phonemize");
    assert_eq!(latin.confidence, 0.65);
    assert!(!latin.phonemes.is_empty());
    let cyrillic = rule_based.phonemize("привет", "ru").expect("phonemize");
    assert!(
        cyrillic.confidence <= 0.2,
        "Cyrillic must be low confidence, got {}",
        cyrillic.confidence
    );

    assert_eq!(
        HeuristicSpamPredictor.capabilities().quality,
        CapabilityLevel::Basic
    );
    let predictor: &dyn SpamPredictor = &HeuristicSpamPredictor;
    assert_eq!(predictor.capabilities().quality, CapabilityLevel::Basic);
}
