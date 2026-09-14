use crate::comparison::scorer::score_fingerprints;
use crate::core::config::SimilarityWeights;
use crate::core::types::{DuplicateMode, DuplicateResult, MessageFingerprint};

pub fn duplicate_result(
    left: &MessageFingerprint,
    right: &MessageFingerprint,
    threshold: f64,
    weights: &SimilarityWeights,
) -> DuplicateResult {
    duplicate_result_with_mode(left, right, threshold, weights, DuplicateMode::Combined)
}

pub fn duplicate_result_with_mode(
    left: &MessageFingerprint,
    right: &MessageFingerprint,
    threshold: f64,
    weights: &SimilarityWeights,
    mode: DuplicateMode,
) -> DuplicateResult {
    if left.raw == right.raw {
        return DuplicateResult {
            duplicate: true,
            score: 1.0,
            reason: "exact raw match".to_string(),
            mode,
        };
    }
    if matches!(mode, DuplicateMode::Combined | DuplicateMode::NearExact)
        && left.normalized == right.normalized
        && left.normalized.is_some()
    {
        return DuplicateResult {
            duplicate: true,
            score: 1.0,
            reason: "normalized match".to_string(),
            mode,
        };
    }
    let comparison = score_fingerprints(left, right, weights);
    let (score, reason) = match mode {
        DuplicateMode::Combined => (
            comparison.score,
            if comparison.visual.unwrap_or(0.0) >= threshold {
                "visual/confusable match"
            } else if comparison.decoded_similarity.unwrap_or(0.0) >= threshold {
                "decoded representation match"
            } else {
                "combined fingerprint similarity"
            },
        ),
        DuplicateMode::NearExact => (
            comparison
                .character
                .unwrap_or(0.0)
                .max(comparison.obfuscation_similarity.unwrap_or(0.0)),
            "near-exact character match",
        ),
        DuplicateMode::Lexical => (comparison.lexical.unwrap_or(0.0), "lexical match"),
        DuplicateMode::Semantic => (comparison.semantic.unwrap_or(0.0), "semantic match"),
        DuplicateMode::Phonetic => (comparison.phonetic.unwrap_or(0.0), "phonetic match"),
        DuplicateMode::Decoded => (
            comparison.decoded_similarity.unwrap_or(0.0),
            "decoded representation match",
        ),
        DuplicateMode::Visual => (comparison.visual.unwrap_or(0.0), "visual/confusable match"),
    };
    DuplicateResult {
        duplicate: score >= threshold,
        score,
        reason: reason.to_string(),
        mode,
    }
}
