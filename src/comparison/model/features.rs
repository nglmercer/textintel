//! Interpretable training features for the logistic similarity scorer.
//! Fixed order shared by training and inference; see schema version in
//! the parent module.

use std::collections::BTreeMap;

use crate::core::types::{ComparisonResult, MessageFingerprint};

/// Interpretable features in fixed order: the eight evidence channels
/// (missing channels read as 0.0, exactly as the scorer treats them),
/// mean channel confidence, top-language agreement, and the twelve
/// confusable-separation features (pair lexicon validity, single-word scope,
/// swapped-word character and phonetic similarity for contextual pairs, the
/// all-valid confusable-swap interaction, the cubed swap-validity
/// interaction, normalized identity for
/// punctuation-only variants, substring containment for super/substring
/// pairs, the cross-script indicator and its agreement interaction,
/// confidence-weighted exact decoding, and the single-word exact
/// interaction).
pub const TRAINING_FEATURES: &[&str] = &[
    "semantic",
    "lexical",
    "character",
    "visual",
    "phonetic",
    "symbolic",
    "decoded",
    "obfuscation",
    "channel_confidence",
    "language_agreement",
    "lexicon_validity",
    "single_word_pair",
    "swapped_word_similarity",
    "swapped_phonetic",
    "confusable_swap",
    "valid_swap_similarity",
    "normalized_identity",
    "substring_containment",
    "cross_script_pair",
    "cross_script_agreement",
    "exact_decode",
    "single_word_exact",
];

/// Mean channel confidence of a comparison. Shared by training and the
/// [`LogisticSimilarityScorer`] so the `channel_confidence` weight means the
/// same in both places.
pub fn mean_channel_confidence(result: &ComparisonResult) -> f64 {
    if result.channel_confidence.is_empty() {
        0.0
    } else {
        (result.channel_confidence.values().sum::<f64>() / result.channel_confidence.len() as f64)
            .clamp(0.0, 1.0)
    }
}

/// Extract the training feature vector for one comparison. `language_agreement`
/// is 1.0 when both fingerprints agree on the top language, else 0.0.
/// Every feature here is also applied at inference by
/// [`LogisticSimilarityScorer::score`]; trained weights are never dead.
pub fn training_features(
    result: &ComparisonResult,
    language_agreement: f64,
) -> BTreeMap<String, f64> {
    let channels = [
        ("semantic", result.semantic),
        ("lexical", result.lexical),
        ("character", result.character),
        ("visual", result.visual),
        ("phonetic", result.phonetic),
        ("symbolic", result.symbolic),
        ("decoded", result.decoded_similarity),
        ("obfuscation", result.obfuscation_similarity),
    ];
    let mut features = BTreeMap::new();
    for (name, value) in channels {
        features.insert(name.to_string(), value.unwrap_or(0.0));
    }
    features.insert(
        "channel_confidence".to_string(),
        mean_channel_confidence(result),
    );
    features.insert(
        "language_agreement".to_string(),
        language_agreement.clamp(0.0, 1.0),
    );
    features.insert(
        "lexicon_validity".to_string(),
        result.lexicon_validity.clamp(0.0, 1.0),
    );
    features.insert(
        "single_word_pair".to_string(),
        result.single_word_pair.clamp(0.0, 1.0),
    );
    features.insert(
        "swapped_word_similarity".to_string(),
        result.swapped_word_similarity.clamp(0.0, 1.0),
    );
    features.insert(
        "swapped_phonetic".to_string(),
        result.swapped_phonetic.clamp(0.0, 1.0),
    );
    features.insert(
        "confusable_swap".to_string(),
        result.confusable_swap.clamp(0.0, 1.0),
    );
    features.insert(
        "valid_swap_similarity".to_string(),
        result.valid_swap_similarity.clamp(0.0, 1.0),
    );
    features.insert(
        "normalized_identity".to_string(),
        result.normalized_identity.clamp(0.0, 1.0),
    );
    features.insert(
        "substring_containment".to_string(),
        result.substring_containment.clamp(0.0, 1.0),
    );
    features.insert(
        "cross_script_pair".to_string(),
        result.cross_script_pair.clamp(0.0, 1.0),
    );
    features.insert(
        "cross_script_agreement".to_string(),
        result.cross_script_agreement.clamp(0.0, 1.0),
    );
    features.insert(
        "exact_decode".to_string(),
        result.exact_decode.clamp(0.0, 1.0),
    );
    features.insert(
        "single_word_exact".to_string(),
        result.single_word_exact.clamp(0.0, 1.0),
    );
    features
}

/// Top-language agreement between two fingerprints: 1.0 when the first
/// language candidates match (or both are empty), else 0.0.
pub fn language_agreement(left: &MessageFingerprint, right: &MessageFingerprint) -> f64 {
    let top = |fingerprint: &MessageFingerprint| {
        fingerprint
            .language_candidates
            .first()
            .map(|candidate| candidate.language.clone())
            .unwrap_or_else(|| "unknown".to_string())
    };
    if top(left) == top(right) {
        1.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::super::TRAINING_FEATURE_SCHEMA_VERSION;
    use super::*;

    #[test]
    fn training_features_cover_schema_in_order() {
        assert_eq!(TRAINING_FEATURES.len(), 22);
        assert_eq!(TRAINING_FEATURE_SCHEMA_VERSION, 7);
        let mut sorted = TRAINING_FEATURES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), TRAINING_FEATURES.len());
    }
}
