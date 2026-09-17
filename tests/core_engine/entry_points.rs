//! Engine entry points: `analyze`, `compare`, `decode`, batch variants,
//! construction errors, and accessor diagnostics.

use textintel::lexical::tokenizer::tokenize;
use textintel::{
    API_VERSION, EngineConfig, FINGERPRINT_SCHEMA_VERSION, ProviderError, StageTimings,
    TextIntelError, TextIntelligence,
};

#[test]
fn analyze_preserves_raw_and_populates_channels() {
    let engine = TextIntelligence::default();
    let raw = "Fra🏠do";
    let fp = engine.analyze(raw).unwrap();
    assert_eq!(fp.raw, raw);
    assert_eq!(fp.schema_version, FINGERPRINT_SCHEMA_VERSION);
    assert_eq!(fp.char_features.length, raw.chars().count());
    assert_eq!(fp.tokens, tokenize(raw));
    assert!(!fp.language_candidates.is_empty());
    assert!(!fp.segments.is_empty());
    assert!(!fp.unicode_features.nfc.is_empty());
    assert!(!fp.decoded_texts().collect::<Vec<_>>().is_empty());
    assert!(fp.top_language().is_some());
    // Model-free default: no embedding or phonetic providers engaged.
    assert!(fp.semantic_embeddings.is_empty());
    assert!(fp.phonetic_candidates.is_empty());
}

#[test]
fn analyze_accepts_empty_input() {
    let fp = TextIntelligence::default().analyze("").unwrap();
    assert_eq!(fp.raw, "");
    assert_eq!(fp.char_features.length, 0);
    assert!(fp.tokens.is_empty());
}

#[test]
fn analyze_rejects_input_over_char_limit() {
    let engine = TextIntelligence::new(EngineConfig {
        max_input_length: 3,
        ..Default::default()
    });
    // Boundary: exactly at the limit passes.
    assert!(engine.analyze("abc").is_ok());
    let err = engine.analyze("abcd").unwrap_err();
    match err {
        TextIntelError::InputTooLong { length, maximum } => {
            assert_eq!((length, maximum), (4, 3));
        }
        other => panic!("expected InputTooLong, got {other:?}"),
    }
}

#[test]
fn analyze_counts_multibyte_chars_as_single_units() {
    let engine = TextIntelligence::new(EngineConfig {
        max_input_length: 2,
        ..Default::default()
    });
    // Three chars in six bytes: the limit counts chars, not bytes.
    let err = engine.analyze("ééé").unwrap_err();
    match err {
        TextIntelError::InputTooLong { length, maximum } => {
            assert_eq!((length, maximum), (3, 2));
        }
        other => panic!("expected InputTooLong, got {other:?}"),
    }
    assert!(engine.analyze("éé").is_ok());
}

#[test]
fn analyze_with_tiny_segment_budget_truncates_without_error() {
    let engine = TextIntelligence::new(EngineConfig {
        max_segments: 1,
        ..Default::default()
    });
    let fp = engine.analyze("hello world foo bar").unwrap();
    assert!(fp.segments.len() <= 1, "segments: {:?}", fp.segments);
}

#[test]
fn analyze_with_timing_reports_nonnegative_stage_timings() {
    let engine = TextIntelligence::default();
    let (fp, timings) = engine.analyze_with_timing("hello world").unwrap();
    assert_eq!(fp.raw, "hello world");
    for (stage, micros) in timings.as_map() {
        assert!(
            micros >= 0.0,
            "stage {stage} reported negative timing {micros}"
        );
    }
    assert_eq!(timings.as_map().len(), StageTimings::stage_names().len());
}

#[test]
fn try_new_rejects_invalid_config_and_new_panics() {
    let bad = EngineConfig {
        max_input_length: 0,
        ..Default::default()
    };
    let err = TextIntelligence::try_new(bad).err().expect("must fail");
    assert!(
        matches!(err, TextIntelError::InvalidConfiguration(_)),
        "got {err:?}"
    );
}

#[test]
fn provider_errors_chain_through_source() {
    use std::error::Error;
    let err = TextIntelError::from(ProviderError::new("embedding", "boom"));
    let source = err.source().expect("provider chain must expose source");
    assert_eq!(source.to_string(), "embedding provider failed: boom");
    assert!(TextIntelError::Storage("x".to_string()).source().is_none());
}

#[test]
#[should_panic(expected = "invalid TextIntelligence configuration")]
fn new_panics_on_invalid_config() {
    let _ = TextIntelligence::new(EngineConfig {
        beam_width: 0,
        ..Default::default()
    });
}

#[test]
fn engine_accessors_report_config_capabilities_and_diagnostics() {
    let engine = TextIntelligence::default();
    assert_eq!(engine.config().max_input_length, 8_192);
    assert!(!engine.provider_capabilities().is_empty());
    assert!(engine.health_check().is_ok());
    let diagnostics = engine.diagnostics();
    assert_eq!(diagnostics.api_version.as_str(), API_VERSION);
    // Default engine ships no resource packs; the manifest call still works.
    let _ = engine.resource_manifest();
}

// ---------------------------------------------------------------------------
// Engine: batch analyze / compare
// ---------------------------------------------------------------------------

#[test]
fn analyze_batch_preserves_order_and_accepts_empty_batch() {
    let engine = TextIntelligence::default();
    let empty: Vec<String> = Vec::new();
    assert!(engine.analyze_batch(&empty).unwrap().is_empty());

    let texts = vec![
        "first".to_string(),
        "second message".to_string(),
        "".to_string(),
    ];
    let fps = engine.analyze_batch(&texts).unwrap();
    assert_eq!(fps.len(), 3);
    for (fp, raw) in fps.iter().zip(&texts) {
        assert_eq!(&fp.raw, raw);
    }
}

#[test]
fn analyze_batch_rejects_oversized_batch() {
    let engine = TextIntelligence::new(EngineConfig {
        max_batch_size: 2,
        ..Default::default()
    });
    let texts = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let err = engine.analyze_batch(&texts).unwrap_err();
    match err {
        TextIntelError::InvalidConfiguration(message) => {
            assert!(message.contains("max_batch_size"), "message: {message}");
        }
        other => panic!("expected InvalidConfiguration, got {other:?}"),
    }
}

#[test]
fn analyze_batch_propagates_per_item_length_errors() {
    let engine = TextIntelligence::new(EngineConfig {
        max_input_length: 2,
        ..Default::default()
    });
    let texts = vec!["ok".to_string(), "too long".to_string()];
    assert!(matches!(
        engine.analyze_batch(&texts).unwrap_err(),
        TextIntelError::InputTooLong { .. }
    ));
}

#[test]
fn compare_batch_matches_pairwise_compare() {
    let engine = TextIntelligence::default();
    let pairs = vec![
        ("hello".to_string(), "hello".to_string()),
        ("hello".to_string(), "goodbye".to_string()),
    ];
    let batched = engine.compare_batch(&pairs).unwrap();
    assert_eq!(batched.len(), 2);
    for ((left, right), result) in pairs.iter().zip(&batched) {
        let single = engine.compare(left, right).unwrap();
        assert!((result.score - single.score).abs() < f64::EPSILON);
    }
    assert!(engine.compare_batch(&[]).unwrap().is_empty());
}

#[test]
fn compare_batch_rejects_oversized_batch() {
    let engine = TextIntelligence::new(EngineConfig {
        max_batch_size: 1,
        ..Default::default()
    });
    let pairs = vec![
        ("a".to_string(), "a".to_string()),
        ("b".to_string(), "b".to_string()),
    ];
    assert!(matches!(
        engine.compare_batch(&pairs).unwrap_err(),
        TextIntelError::InvalidConfiguration(_)
    ));
}

// ---------------------------------------------------------------------------
// Engine: compare
// ---------------------------------------------------------------------------

#[test]
fn compare_identical_texts_scores_one() {
    let engine = TextIntelligence::default();
    let result = engine.compare("hello world", "hello world").unwrap();
    assert_eq!(result.character, Some(1.0));
    assert_eq!(result.lexical, Some(1.0));
    assert!(result.score > 0.99, "score: {}", result.score);
    // Model-free default leaves provider channels empty.
    assert_eq!(result.semantic, None);
    assert_eq!(result.phonetic, None);
}

#[test]
fn compare_orders_identical_above_paraphrase_above_unrelated() {
    let engine = TextIntelligence::default();
    let identical = engine
        .compare("buy now cheap", "buy now cheap")
        .unwrap()
        .score;
    let overlap = engine
        .compare("buy now cheap", "buy now fast")
        .unwrap()
        .score;
    let unrelated = engine
        .compare("buy now cheap", "quantum photosynthesis")
        .unwrap()
        .score;
    assert!(identical > overlap, "{identical} vs {overlap}");
    assert!(overlap > unrelated, "{overlap} vs {unrelated}");
}

#[test]
fn compare_is_case_and_accent_insensitive_on_character_channel() {
    let engine = TextIntelligence::default();
    let result = engine.compare("COMPRA AHORA", "compra ahora").unwrap();
    assert!(result.character.unwrap() > 0.7);
    let accented = engine.compare("música", "musica").unwrap();
    assert!(accented.character.unwrap() > 0.99);
}

#[test]
fn compare_propagates_length_errors_from_either_side() {
    let engine = TextIntelligence::new(EngineConfig {
        max_input_length: 3,
        ..Default::default()
    });
    assert!(matches!(
        engine.compare("toolong", "ok").unwrap_err(),
        TextIntelError::InputTooLong { .. }
    ));
    assert!(matches!(
        engine.compare("ok", "toolong").unwrap_err(),
        TextIntelError::InputTooLong { .. }
    ));
}

#[test]
fn compare_with_timing_matches_compare_and_reports_comparison_stage() {
    let engine = TextIntelligence::default();
    let (timed, timings) = engine.compare_with_timing("hello", "hallo").unwrap();
    let plain = engine.compare("hello", "hallo").unwrap();
    assert!((timed.score - plain.score).abs() < f64::EPSILON);
    assert!(timings.comparison_micros >= 0.0);
    assert!(timings.total_micros >= 0.0);
    // Both analyze stages are summed into the compare timings.
    assert!(timings.normalization_micros >= 0.0);
}

#[test]
fn compare_fingerprints_agrees_with_compare() {
    let engine = TextIntelligence::default();
    let left = engine.analyze("hello world").unwrap();
    let right = engine.analyze("hello there").unwrap();
    let via_fps = engine.compare_fingerprints(&left, &right);
    let direct = engine.compare("hello world", "hello there").unwrap();
    assert!((via_fps.score - direct.score).abs() < f64::EPSILON);
    assert_eq!(via_fps.character, direct.character);
}

// ---------------------------------------------------------------------------
// Engine: decode
// ---------------------------------------------------------------------------

#[test]
fn decode_recovers_leet_and_symbol_readings() {
    let engine = TextIntelligence::default();
    let candidates = engine.decode("h3llo").unwrap();
    assert!(
        candidates.iter().any(|c| c.text == "hello"),
        "candidates: {:?}",
        candidates.iter().map(|c| &c.text).collect::<Vec<_>>()
    );
    let candidates = engine.decode("salU2").unwrap();
    assert!(
        candidates
            .iter()
            .any(|c| c.text.eq_ignore_ascii_case("saludos"))
    );
}

#[test]
fn decode_with_languages_respects_candidate_limit() {
    let engine = TextIntelligence::default();
    let limited = engine
        .decode_with_languages("salU2", None, Some(2))
        .unwrap();
    assert!(limited.len() <= 2 && !limited.is_empty());
    // A zero limit clamps to one candidate rather than returning nothing.
    let zero = engine
        .decode_with_languages("salU2", None, Some(0))
        .unwrap();
    assert_eq!(zero.len(), 1);
}

#[test]
fn decode_with_explicit_languages_runs_language_pinned() {
    let engine = TextIntelligence::default();
    let languages = vec!["en".to_string()];
    let candidates = engine
        .decode_with_languages("h3llo", Some(&languages), None)
        .unwrap();
    assert!(!candidates.is_empty());
    let empty: Vec<String> = Vec::new();
    assert!(
        !engine
            .decode_with_languages("h3llo", Some(&empty), None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn decode_rejects_overlong_input() {
    let engine = TextIntelligence::new(EngineConfig {
        max_input_length: 2,
        ..Default::default()
    });
    assert!(matches!(
        engine.decode("toolong").unwrap_err(),
        TextIntelError::InputTooLong { .. }
    ));
    assert!(matches!(
        engine
            .decode_with_languages("toolong", None, None)
            .unwrap_err(),
        TextIntelError::InputTooLong { .. }
    ));
}
