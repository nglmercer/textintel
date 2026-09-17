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
/// interaction), the fuzzy-decode mismatch interaction (close decoded
/// overlap without exact decoding: a confusable signature), and the seven
/// contextual/entity features (transformer-only semantic, cross-language
/// semantic, entity agreement/conflict, semantic without lexical overlap,
/// and transliteration↔semantic agreement/conflict). (Validation history:
/// single-word typo, cross-script transliteration, cross-script
/// fuzzy-decoded, and single-word cross-language interactions were all tried
/// and removed before schema 9 — every rescue is zero-sum against a matched
/// negative phenomenon (rare-word confusables, exact Cyrillic false
/// friends, leet-confusables). See PRODUCTION_GAP_ANALYSIS.md.)
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
    "fuzzy_decode_mismatch",
    "contextual_semantic",
    "cross_language_semantic",
    "entity_agreement",
    "entity_conflict",
    "semantic_without_lexical_overlap",
    "transliteration_semantic_agreement",
    "transliteration_semantic_conflict",
];

/// Fuzzy decoded overlap without exact decoding:
/// `symbolic * decoded * (1 - exact_decode)`. A reading that closely but
/// inexactly matches the target while exact decoding fails (`ch34p`→`cheap`
/// against `cheep`, `gr8`→`great` against `grate`) is a confusable
/// signature; true variants decode exactly (`gr8` against `great`) and read
/// near zero. Missing channels read as 0.0, exactly as the scorer treats
/// them, so clean-text pairs (no symbolic channel) never fire it.
pub fn fuzzy_decode_mismatch(result: &ComparisonResult) -> f64 {
    (result.symbolic.unwrap_or(0.0)
        * result.decoded_similarity.unwrap_or(0.0)
        * (1.0 - result.exact_decode.clamp(0.0, 1.0)))
    .clamp(0.0, 1.0)
}

/// Mean channel confidence of a comparison. Shared by training and the
/// [`LogisticSimilarityScorer`](super::LogisticSimilarityScorer) so the
/// `channel_confidence` weight means the same in both places.
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
/// [`SimilarityScorer::score`](crate::SimilarityScorer::score); trained
/// weights are never dead.
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
    features.insert(
        "fuzzy_decode_mismatch".to_string(),
        fuzzy_decode_mismatch(result),
    );
    features.insert(
        "contextual_semantic".to_string(),
        result.contextual_semantic.clamp(0.0, 1.0),
    );
    features.insert(
        "cross_language_semantic".to_string(),
        result.cross_language_semantic.clamp(0.0, 1.0),
    );
    features.insert(
        "entity_agreement".to_string(),
        result.entity_agreement.clamp(0.0, 1.0),
    );
    features.insert(
        "entity_conflict".to_string(),
        result.entity_conflict.clamp(0.0, 1.0),
    );
    features.insert(
        "semantic_without_lexical_overlap".to_string(),
        result.semantic_without_lexical_overlap.clamp(0.0, 1.0),
    );
    features.insert(
        "transliteration_semantic_agreement".to_string(),
        result.transliteration_semantic_agreement.clamp(0.0, 1.0),
    );
    features.insert(
        "transliteration_semantic_conflict".to_string(),
        result.transliteration_semantic_conflict.clamp(0.0, 1.0),
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
    if top(left) == top(right) { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::super::TRAINING_FEATURE_SCHEMA_VERSION;
    use super::*;

    #[test]
    fn training_features_cover_schema_in_order() {
        assert_eq!(TRAINING_FEATURES.len(), 30);
        assert_eq!(TRAINING_FEATURE_SCHEMA_VERSION, 9);
        let mut sorted = TRAINING_FEATURES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), TRAINING_FEATURES.len());
    }

    #[test]
    fn fuzzy_decode_mismatch_needs_fuzzy_overlap_without_exactness() {
        // `ch34p` against `cheep`: close decoded overlap, no exact decoding.
        let confusable = ComparisonResult {
            symbolic: Some(0.41),
            decoded_similarity: Some(0.75),
            exact_decode: 0.0,
            ..Default::default()
        };
        assert!((fuzzy_decode_mismatch(&confusable) - 0.3075).abs() < 1e-9);
        // `gr8` against `great`: exact decoding suppresses the mismatch.
        let variant = ComparisonResult {
            symbolic: Some(0.30),
            decoded_similarity: Some(0.95),
            exact_decode: 0.9,
            ..Default::default()
        };
        assert!(fuzzy_decode_mismatch(&variant) < 0.05);
        // Clean text (no symbolic channel) never fires it.
        let clean = ComparisonResult {
            decoded_similarity: Some(0.8),
            ..Default::default()
        };
        assert_eq!(fuzzy_decode_mismatch(&clean), 0.0);
    }
}
