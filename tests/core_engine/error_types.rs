//! `core::{error, types}` coverage: error display/conversion and the
//! shared DTO builders.

use std::collections::BTreeMap;

use textintel::{
    ChannelAvailability, DecodedCandidate, DuplicateMode, FINGERPRINT_SCHEMA_VERSION,
    LanguageCandidate, ProviderError, StageTimings, SymbolConcept, SymbolReading, TextIntelError,
    TextIntelligence, Transformation,
};

#[test]
fn provider_error_carries_provider_message_and_display() {
    let err = ProviderError::new("embedding", "boom");
    assert_eq!(err.provider, "embedding");
    assert_eq!(err.message, "boom");
    assert_eq!(format!("{err}"), "embedding provider failed: boom");
}

#[test]
fn text_intel_error_display_covers_every_variant() {
    let cases: Vec<(TextIntelError, &str)> = vec![
        (
            TextIntelError::InputTooLong {
                length: 9,
                maximum: 8,
            },
            "input length 9 exceeds max_input_length=8",
        ),
        (
            TextIntelError::TooManySegments { maximum: 4 },
            "message exceeds max_segments=4",
        ),
        (
            TextIntelError::InvalidConfiguration("bad".to_string()),
            "invalid configuration: bad",
        ),
        (
            TextIntelError::Provider(ProviderError::new("p", "m")),
            "p provider failed: m",
        ),
        (
            TextIntelError::Storage("disk".to_string()),
            "storage error: disk",
        ),
        (
            TextIntelError::Serialization("json".to_string()),
            "serialization error: json",
        ),
    ];
    for (err, expected) in cases {
        assert_eq!(format!("{err}"), expected);
    }
}

#[test]
fn text_intel_error_converts_from_provider_and_json_errors() {
    let converted = TextIntelError::from(ProviderError::new("p", "m"));
    assert!(matches!(converted, TextIntelError::Provider(_)));

    let json_err = serde_json::from_str::<serde_json::Value>("{bad").unwrap_err();
    let converted = TextIntelError::from(json_err);
    assert!(matches!(converted, TextIntelError::Serialization(_)));
}

// ---------------------------------------------------------------------------
// Core: types
// ---------------------------------------------------------------------------

#[test]
fn language_candidate_and_symbol_builders_set_fields() {
    let candidate = LanguageCandidate::new("en", 0.9);
    assert_eq!(candidate.language, "en");
    assert_eq!(candidate.probability, 0.9);

    let reading = SymbolReading::new("house", Some("en"), 0.8, "name").with_source("pack:test");
    assert_eq!(reading.text, "house");
    assert_eq!(reading.language.as_deref(), Some("en"));
    assert_eq!(reading.source.as_deref(), Some("pack:test"));

    let concept = SymbolConcept::new("concept:building.house", 0.7).with_source("builtin:test");
    assert_eq!(concept.id, "concept:building.house");
    assert_eq!(concept.source.as_deref(), Some("builtin:test"));
}

#[test]
fn transformation_builders_clamp_confidence_and_explain() {
    let base = Transformation::new("4", "a", "leetspeak");
    assert_eq!(base.start, None);
    assert_eq!(base.confidence, None);
    assert_eq!(base.explain(), "4 = a (leetspeak)");

    let spanned = Transformation::new("x", "y", "symbol").with_span(0, 1, "🏠");
    assert_eq!(spanned.explain(), "🏠 = y (symbol)");

    let full = Transformation::new("x", "y", "symbol")
        .with_span(0, 1, "🏠")
        .with_language("en")
        .with_provider("rebus")
        .with_confidence(0.9);
    assert_eq!(full.explain(), "🏠 = y (symbol; en)");
    assert_eq!(full.confidence, Some(0.9));

    assert_eq!(
        Transformation::new("a", "b", "t")
            .with_confidence(1.5)
            .confidence,
        Some(1.0)
    );
    assert_eq!(
        Transformation::new("a", "b", "t")
            .with_confidence(-2.0)
            .confidence,
        Some(0.0)
    );
}

#[test]
fn channel_availability_clamps_and_marks_unavailable() {
    let available = ChannelAvailability::available("rule-g2p", 0.7);
    assert!(available.available);
    assert_eq!(available.confidence, 0.7);

    let clamped = ChannelAvailability::available("x", 9.0);
    assert_eq!(clamped.confidence, 1.0);

    let unavailable = ChannelAvailability::unavailable("http-embed");
    assert!(!unavailable.available);
    assert_eq!(unavailable.confidence, 0.0);
}

#[test]
fn decoded_candidate_confidence_discounts_contested_ranks() {
    let lonely = DecodedCandidate {
        text: "hello".to_string(),
        score: 0.8,
        transformations: Vec::new(),
        language: None,
        lexical_score: 0.0,
        phonetic_score: 0.0,
        context_score: 0.0,
        symbol_score: 0.0,
        confidence_gap: 1.0,
        strong: true,
    };
    assert!((lonely.confidence() - 0.8).abs() < 1e-12);

    let contested = DecodedCandidate {
        confidence_gap: 0.0,
        ..lonely.clone()
    };
    assert!((contested.confidence() - 0.4).abs() < 1e-12);

    // Out-of-range scores clamp instead of escaping [0, 1].
    let over = DecodedCandidate {
        score: 5.0,
        confidence_gap: 1.0,
        ..lonely.clone()
    };
    assert!((over.confidence() - 1.0).abs() < 1e-12);
}

#[test]
fn transliteration_views_filter_clamp_and_default_confidence() {
    let mut fp = TextIntelligence::default().analyze("hello").unwrap();
    fp.normalization_views = BTreeMap::from([
        ("transliteration:a".to_string(), "hola".to_string()),
        ("transliteration:b".to_string(), "allo".to_string()),
        ("transliteration:c".to_string(), "ciao".to_string()),
        ("other".to_string(), "ignored".to_string()),
    ]);
    fp.transliteration_confidence = BTreeMap::from([
        ("transliteration:a".to_string(), 2.0), // clamps to 1.0
        ("transliteration:b".to_string(), f64::NAN), // hostile -> 0.0
                                                // transliteration:c missing -> defaults to 1.0
    ]);
    let mut views = fp.transliteration_views();
    views.sort_by(|a, b| a.0.cmp(b.0));
    assert_eq!(
        views,
        vec![("allo", 0.0), ("ciao", 1.0), ("hola", 1.0)],
        "views: {views:?}"
    );
}

#[test]
fn stage_timings_map_and_names_cover_pipeline_order() {
    assert_eq!(
        StageTimings::stage_names(),
        &[
            "normalization",
            "language",
            "symbols",
            "rebus",
            "semantic",
            "phonetic",
            "comparison",
            "total"
        ]
    );
    let timings = StageTimings {
        total_micros: 3.0,
        ..StageTimings::default()
    };
    let map = timings.as_map();
    assert_eq!(map.len(), 8);
    assert_eq!(map["total"], 3.0);
    let ordered: Vec<String> = StageTimings::stage_names()
        .iter()
        .map(ToString::to_string)
        .collect();
    let mut keys: Vec<String> = map.keys().cloned().collect();
    keys.sort();
    let mut expected = ordered.clone();
    expected.sort();
    assert_eq!(keys, expected);
}

#[test]
fn duplicate_mode_defaults_to_combined() {
    assert_eq!(DuplicateMode::default(), DuplicateMode::Combined);
}

#[test]
fn fingerprint_and_comparison_results_serialize_stably() {
    let engine = TextIntelligence::default();
    let fp = engine.analyze("hello").unwrap();
    let json = serde_json::to_string(&fp).unwrap();
    let back: textintel::MessageFingerprint = serde_json::from_str(&json).unwrap();
    assert_eq!(back.raw, "hello");
    assert_eq!(back.schema_version, FINGERPRINT_SCHEMA_VERSION);

    let comparison = engine.compare("hello", "hallo").unwrap();
    let json = serde_json::to_string(&comparison).unwrap();
    assert!(json.contains("\"score\""));
}
