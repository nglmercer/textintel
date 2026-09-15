#![no_main]

//! Comparison-scorer hardening: scoring arbitrary fingerprint pairs (freshly
//! analyzed or JSON-deserialized) must return finite scores, never panic.
//! See `tests/fuzz_regression.rs`.

use libfuzzer_sys::fuzz_target;

use textintel::core::types::MessageFingerprint;
use textintel::{EngineConfig, SimilarityWeights, TextIntelligence};

fn check_finite(score: f64) {
    assert!(
        score.is_finite(),
        "comparison score must be finite, got {score}"
    );
}

fn engine() -> &'static TextIntelligence {
    static ENGINE: std::sync::OnceLock<TextIntelligence> = std::sync::OnceLock::new();
    ENGINE.get_or_init(|| TextIntelligence::new(EngineConfig::default()))
}

fuzz_target!(|input: String| {
    let bounded: String = input.chars().take(1024).collect();
    let engine = engine();
    // Path 1: analyze-then-compare over split halves.
    let middle = bounded
        .char_indices()
        .nth(bounded.chars().count() / 2)
        .map(|(index, _)| index)
        .unwrap_or(0);
    let (left, right) = bounded.split_at(middle);
    if let (Ok(left_fp), Ok(right_fp)) = (engine.analyze(left), engine.analyze(right)) {
        check_finite(engine.compare_fingerprints(&left_fp, &right_fp).score);
    }
    // Path 2: deserialize-then-score over JSON halves (exercises the scorer
    // over the full fingerprint shape space, including hostile vectors).
    let bytes = bounded.as_bytes();
    let (first, second) = bytes.split_at(bytes.len() / 2);
    if let (Ok(left), Ok(right)) = (
        serde_json::from_slice::<MessageFingerprint>(first),
        serde_json::from_slice::<MessageFingerprint>(second),
    ) {
        let result = textintel::comparison::scorer::score_fingerprints(
            &left,
            &right,
            &SimilarityWeights::default(),
        );
        check_finite(result.score);
    }
});
