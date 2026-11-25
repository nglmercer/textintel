pub mod decision;
pub mod profile;
pub mod scorer;

pub use profile::{ProfileSimilarityScorer, SimilarityProfile, score_fingerprints_with_profile};
pub use scorer::{combine_scores, score_fingerprints};

/// Applies the baked decision ensemble exactly when the configured weights
/// are the default. Both the engine comparison path and duplicate detection
/// route through this helper so `compare` and `duplicate` can never drift
/// into different scoring semantics for the same pair.
pub(crate) fn apply_decision_if_default(
    result: &mut crate::core::types::ComparisonResult,
    weights: &crate::core::config::SimilarityWeights,
) {
    if *weights == crate::core::config::SimilarityWeights::default() {
        decision::apply_decision_layer(result);
    }
}
