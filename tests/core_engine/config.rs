//! `core::config` coverage: engine, similarity, rebus, and cache limits.

use textintel::{CacheLimits, EngineConfig, RebusWeights, SimilarityWeights, TextIntelligence};

#[test]
fn engine_config_default_validates() {
    assert!(EngineConfig::default().validate().is_ok());
    assert!(TextIntelligence::try_new(EngineConfig::default()).is_ok());
}

#[test]
fn engine_config_rejects_zero_limits() {
    let base = EngineConfig {
        max_input_length: 0,
        ..EngineConfig::default()
    };
    assert!(base.validate().is_err());

    for field in [
        "max_segments",
        "beam_width",
        "max_candidates",
        "max_symbol_readings",
        "max_recursion",
        "max_documents",
        "repetition_keep",
        "max_batch_size",
        "max_search_candidates",
        "max_decoded_branches",
    ] {
        let mut config = EngineConfig::default();
        match field {
            "max_segments" => config.max_segments = 0,
            "beam_width" => config.beam_width = 0,
            "max_candidates" => config.max_candidates = 0,
            "max_symbol_readings" => config.max_symbol_readings = 0,
            "max_recursion" => config.max_recursion = 0,
            "max_documents" => config.max_documents = 0,
            "repetition_keep" => config.repetition_keep = 0,
            "max_batch_size" => config.max_batch_size = 0,
            "max_search_candidates" => config.max_search_candidates = 0,
            "max_decoded_branches" => config.max_decoded_branches = 0,
            _ => unreachable!(),
        }
        assert!(config.validate().is_err(), "{field} = 0 must fail");
    }
}

#[test]
fn engine_config_validates_confidence_gap_and_language_hints() {
    for gap in [0.0, 1.0, 0.15] {
        let config = EngineConfig {
            strong_confidence_gap: gap,
            ..EngineConfig::default()
        };
        assert!(config.validate().is_ok(), "gap {gap} must pass");
    }
    for gap in [-0.1, 1.5, f64::NAN, f64::INFINITY] {
        let config = EngineConfig {
            strong_confidence_gap: gap,
            ..EngineConfig::default()
        };
        assert!(config.validate().is_err(), "gap {gap} must fail");
    }
    for hints in [vec!["".to_string()], vec!["   ".to_string()]] {
        let config = EngineConfig {
            language_hints: hints,
            ..EngineConfig::default()
        };
        assert!(config.validate().is_err());
    }
    let config = EngineConfig {
        language_hints: vec!["en".to_string(), "es".to_string()],
        ..EngineConfig::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn similarity_weights_validate_bounds_and_require_positive_total() {
    assert!(SimilarityWeights::default().validate().is_ok());
    assert_eq!(SimilarityWeights::default().as_map().len(), 8);

    let negative = SimilarityWeights {
        lexical: -0.1,
        ..SimilarityWeights::default()
    };
    assert!(negative.validate().is_err());

    let nan = SimilarityWeights {
        semantic: f64::NAN,
        ..SimilarityWeights::default()
    };
    assert!(nan.validate().is_err());

    let infinite = SimilarityWeights {
        visual: f64::INFINITY,
        ..SimilarityWeights::default()
    };
    assert!(infinite.validate().is_err());

    let zero = SimilarityWeights {
        semantic: 0.0,
        lexical: 0.0,
        character: 0.0,
        visual: 0.0,
        phonetic: 0.0,
        symbolic: 0.0,
        decoded: 0.0,
        obfuscation: 0.0,
    };
    assert!(zero.validate().is_err());
}

#[test]
fn rebus_weights_validate_every_clause() {
    assert!(RebusWeights::default().validate().is_ok());
    assert!(RebusWeights::default().channel_sum() > 0.0);

    let scale = RebusWeights {
        frequency_scale: 0.0,
        ..RebusWeights::default()
    };
    assert!(scale.validate().is_err());

    let cap = RebusWeights {
        transformation_penalty_cap: 1.0,
        ..RebusWeights::default()
    };
    assert!(cap.validate().is_err());

    let semantic = RebusWeights {
        semantic: 1.5,
        ..RebusWeights::default()
    };
    assert!(semantic.validate().is_err());

    let switch = RebusWeights {
        language_switch_penalty: 1.0,
        ..RebusWeights::default()
    };
    assert!(switch.validate().is_err());

    for discount in [0.0, -0.5, 1.5, f64::NAN] {
        let weights = RebusWeights {
            in_word_digit_discount: discount,
            ..RebusWeights::default()
        };
        assert!(weights.validate().is_err(), "discount {discount} must fail");
    }

    let channels = RebusWeights {
        lexical: 0.0,
        phonetic: 0.0,
        symbol: 0.0,
        language: 0.0,
        context: 0.0,
        ..RebusWeights::default()
    };
    assert!(channels.validate().is_err());

    let cost = RebusWeights {
        cost_symbol: -1.0,
        ..RebusWeights::default()
    };
    assert!(cost.validate().is_err());
}

#[test]
fn rebus_weights_json_roundtrip_and_reject_bad_payloads() {
    let weights = RebusWeights::default();
    let json = weights.to_json().unwrap();
    let parsed = RebusWeights::from_json(&json).unwrap();
    assert_eq!(parsed, weights);
    // Empty object picks up serde defaults and stays valid.
    assert!(RebusWeights::from_json("{}").is_ok());
    assert!(RebusWeights::from_json("not json").is_err());
    assert!(RebusWeights::from_json(r#"{"frequency_scale": 0.0}"#).is_err());
}

#[test]
fn cache_limits_production_preset_and_defaults() {
    assert!(!CacheLimits::default().any_enabled());
    assert!(CacheLimits::production().any_enabled());

    let filled = CacheLimits::default().with_production_defaults();
    assert_eq!(filled, CacheLimits::production());

    let explicit = CacheLimits {
        g2p: 8,
        ..CacheLimits::default()
    };
    let merged = explicit.with_production_defaults();
    assert_eq!(merged.g2p, 8);
    assert_eq!(merged.embeddings, CacheLimits::production().embeddings);
}
