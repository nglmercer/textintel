//! Stage timing diagnostics: every pipeline stage is timed, totals are
//! consistent, and timings never carry user text.

use textintel::core::types::StageTimings;
use textintel::{EngineConfig, TextIntelligence};

#[test]
fn stage_names_cover_the_pipeline() {
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
            "total",
        ]
    );
}

#[test]
fn analyze_timing_reports_all_stages() {
    let engine = TextIntelligence::new(EngineConfig::default());
    let (fingerprint, timings) = engine
        .analyze_with_timing("Fra🏠do secret-payload")
        .unwrap();
    assert_eq!(fingerprint.raw, "Fra🏠do secret-payload");
    let map = timings.as_map();
    assert_eq!(map.len(), 8);
    for stage in StageTimings::stage_names() {
        assert!(map.contains_key(*stage), "missing stage {stage}");
        assert!(map[*stage] >= 0.0, "stage {stage} has negative duration");
    }
    assert!(timings.total_micros > 0.0);
    // Comparison is zero for analyze (no pair scored); semantic/phonetic are
    // zero with the default opt-out config.
    assert_eq!(timings.comparison_micros, 0.0);
}

#[test]
fn compare_timing_sums_both_sides_plus_scoring() {
    let engine = TextIntelligence::new(EngineConfig::default());
    let (result, timings) = engine
        .compare_with_timing("hello world", "hello there")
        .unwrap();
    assert!(result.score > 0.0);
    assert!(timings.comparison_micros > 0.0, "scoring must be timed");
    assert!(timings.total_micros > 0.0);
    let parts = timings.normalization_micros
        + timings.language_micros
        + timings.symbols_micros
        + timings.rebus_micros
        + timings.semantic_micros
        + timings.phonetic_micros
        + timings.comparison_micros;
    // Total wall time covers the parts plus orchestration overhead.
    assert!(
        timings.total_micros + 1.0 >= parts,
        "total {} should cover parts sum {parts}",
        timings.total_micros
    );
}

#[test]
fn timings_never_carry_user_text() {
    let engine = TextIntelligence::new(EngineConfig::default());
    let secret = "s3cr3t-payload-zz9";
    let (_, analyze_timings) = engine.analyze_with_timing(secret).unwrap();
    let (_, compare_timings) = engine.compare_with_timing(secret, "other").unwrap();
    for timings in [analyze_timings, compare_timings] {
        let json = serde_json::to_string(&timings).unwrap();
        assert!(
            !json.contains("s3cr3t"),
            "timings must not leak input text: {json}"
        );
        let debug = format!("{timings:?}");
        assert!(
            !debug.contains("s3cr3t"),
            "timings Debug must not leak input text: {debug}"
        );
    }
}

#[test]
fn rebus_skip_empties_decoded_evidence() {
    let text = "Fr4🏠d0 secret-payload";
    let full = TextIntelligence::new(EngineConfig::default());
    let (decoded, _) = full.analyze_with_timing(text).unwrap();
    // The text must actually decode, or the skip assertions below are vacuous.
    assert!(
        !decoded.rebus_candidates.is_empty(),
        "fixture text must decode under the default config"
    );
    assert_eq!(
        decoded.metadata.get("rebus_enabled").map(String::as_str),
        Some("true")
    );
    let skipped = TextIntelligence::new(EngineConfig {
        rebus: false,
        ..Default::default()
    });
    let (fingerprint, timings) = skipped.analyze_with_timing(text).unwrap();
    assert!(fingerprint.rebus_candidates.is_empty());
    assert!(fingerprint.spoken_candidates.is_empty());
    assert_eq!(timings.rebus_micros, 0.0);
    assert_eq!(
        fingerprint
            .metadata
            .get("rebus_enabled")
            .map(String::as_str),
        Some("false")
    );
    assert_eq!(
        fingerprint
            .channel_availability
            .get("decoded")
            .map(|channel| channel.available),
        Some(false)
    );
    assert!((0.0..=1.0).contains(&fingerprint.lexicon_coverage));
    // Stored configs predate the toggle: a missing key keeps decoding on.
    let legacy: EngineConfig = serde_json::from_str("{}").unwrap();
    assert!(legacy.rebus);
}

#[test]
fn semantic_stage_is_timed_when_enabled() {
    let config = EngineConfig {
        semantic: true,
        ..Default::default()
    };
    let engine = TextIntelligence::new(config).with_embedding_provider(
        textintel::semantic::FeatureHashEmbeddingProvider::new(32).unwrap(),
    );
    let (_, timings) = engine.analyze_with_timing("hello world").unwrap();
    assert!(
        timings.semantic_micros > 0.0,
        "enabled semantic embedding must be timed"
    );
}
