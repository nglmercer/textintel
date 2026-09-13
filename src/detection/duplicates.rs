use crate::core::types::{DuplicateResult, MessageFingerprint};
use crate::comparison::scorer::score_fingerprints;
use crate::core::config::SimilarityWeights;

pub fn duplicate_result(
    left: &MessageFingerprint,
    right: &MessageFingerprint,
    threshold: f64,
    weights: &SimilarityWeights,
) -> DuplicateResult {
    if left.raw == right.raw {
        return DuplicateResult { duplicate: true, score: 1.0, reason: "exact raw match".to_string() };
    }
    if left.normalized == right.normalized && left.normalized.is_some() {
        return DuplicateResult { duplicate: true, score: 1.0, reason: "normalized match".to_string() };
    }
    let comparison = score_fingerprints(left, right, weights);
    let score = comparison.score;
    let reason = if comparison.visual.unwrap_or(0.0) >= threshold {
        "visual/confusable match".to_string()
    } else if comparison.decoded_similarity.unwrap_or(0.0) >= threshold {
        "decoded representation match".to_string()
    } else {
        "combined fingerprint similarity".to_string()
    };
    DuplicateResult { duplicate: score >= threshold, score, reason }
}

