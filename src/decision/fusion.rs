//! Versioned `MessageFingerprint` feature vector for decision fusion.
//!
//! The transformer sees semantics; the fingerprint sees everything else
//! (scripts, confusables, obfuscation, entities, …). Fusion concatenates
//! both before the scoring head. This module owns the numeric side: a
//! fixed-order, versioned vector where every entry is finite. Counts are
//! raw (documented per feature); model code normalizes at training time.
//!
//! The layout is append-only within a schema version: new features extend
//! [`FUSION_FEATURES`], and any reorder or reinterpretation bumps
//! [`FUSION_FEATURE_SCHEMA_VERSION`] so old artifacts fail loudly.

use std::collections::BTreeMap;

use crate::core::types::MessageFingerprint;

/// Version of the fusion feature layout. Pinned by decision artifacts.
pub const FUSION_FEATURE_SCHEMA_VERSION: u32 = 1;

/// Fixed-order fusion feature names. [`fusion_feature_vector`] returns
/// values in exactly this order.
pub const FUSION_FEATURES: &[&str] = &[
    "language_top_probability",
    "language_candidate_count",
    "lexicon_coverage",
    "token_count",
    "char_length",
    "digit_ratio",
    "punctuation_ratio",
    "uppercase_ratio",
    "script_count",
    "mixed_scripts",
    "confusable_count",
    "suspicious_unicode_score",
    "invisible_character_count",
    "bidirectional_control_count",
    "obfuscation_score",
    "leet_score",
    "homoglyph_score",
    "repetition_score",
    "fragmentation_score",
    "rebus_top_score",
    "rebus_candidate_count",
    "entity_count",
    "url_email_count",
    "emoji_count",
    "symbol_count",
];

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

/// Named fusion features for inspection and debugging.
pub fn fusion_features(fingerprint: &MessageFingerprint) -> BTreeMap<String, f64> {
    FUSION_FEATURES
        .iter()
        .zip(fusion_values(fingerprint))
        .map(|(name, value)| ((*name).to_string(), value))
        .collect()
}

/// Fusion features as a fixed-order vector for model input. Every entry is
/// finite; the order matches [`FUSION_FEATURES`].
pub fn fusion_feature_vector(fingerprint: &MessageFingerprint) -> Vec<f64> {
    fusion_values(fingerprint).to_vec()
}

/// Shared value core: one pass over the fingerprint, no map. Both
/// public constructors read from this so they can never disagree.
fn fusion_values(fingerprint: &MessageFingerprint) -> [f64; 25] {
    let chars = &fingerprint.char_features;
    let unicode = &fingerprint.unicode_features;
    let obfuscation = &fingerprint.obfuscation_features;
    let length = chars.length.max(1) as f64;
    let letters = chars.letters.max(1) as f64;
    let uppercase = fingerprint
        .raw
        .chars()
        .filter(|character| character.is_uppercase())
        .count() as f64;
    let url_email = fingerprint
        .segments
        .iter()
        .filter(|segment| segment.segment_type == "url" || segment.segment_type == "email")
        .count();
    let emoji = fingerprint
        .segments
        .iter()
        .filter(|segment| segment.segment_type == "emoji")
        .count();
    let rebus_top = fingerprint
        .rebus_candidates
        .iter()
        .map(|candidate| candidate.score)
        .fold(0.0, f64::max);
    let language_top = fingerprint
        .language_candidates
        .first()
        .map(|candidate| candidate.probability)
        .unwrap_or(0.0);
    let values = [
        finite_or_zero(language_top).clamp(0.0, 1.0),
        fingerprint.language_candidates.len() as f64,
        finite_or_zero(fingerprint.lexicon_coverage).clamp(0.0, 1.0),
        fingerprint.tokens.len() as f64,
        chars.length as f64,
        chars.digits as f64 / length,
        chars.punctuation as f64 / length,
        uppercase / letters,
        unicode.scripts.len() as f64,
        if unicode.mixed_scripts { 1.0 } else { 0.0 },
        unicode.confusable_characters.len() as f64,
        finite_or_zero(unicode.suspicious_unicode_score).clamp(0.0, 1.0),
        unicode.invisible_characters.len() as f64,
        unicode.bidirectional_controls.len() as f64,
        finite_or_zero(obfuscation.score).clamp(0.0, 1.0),
        finite_or_zero(obfuscation.leet_score).clamp(0.0, 1.0),
        finite_or_zero(obfuscation.homoglyph_score).clamp(0.0, 1.0),
        finite_or_zero(obfuscation.repetition_score).clamp(0.0, 1.0),
        finite_or_zero(obfuscation.fragmentation_score).clamp(0.0, 1.0),
        finite_or_zero(rebus_top).clamp(0.0, 1.0),
        fingerprint.rebus_candidates.len() as f64,
        fingerprint.entities.len() as f64,
        url_email as f64,
        emoji as f64,
        fingerprint.symbols.len() as f64,
    ];
    debug_assert_eq!(values.len(), FUSION_FEATURES.len());
    values
}
