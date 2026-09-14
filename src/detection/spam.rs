use crate::core::capabilities::ProviderCapabilities;
use crate::core::error::ProviderError;
use crate::core::providers::SpamPredictor;
use crate::core::types::{MessageFingerprint, PatternMatch, SpamFeatures, SpamResult};

/// Extract stable, serializable abuse features. A predictor can be trained
/// over this structure without coupling model code to the fingerprint layout.
pub fn spam_features(fingerprint: &MessageFingerprint, patterns: &[PatternMatch]) -> SpamFeatures {
    let url_count = fingerprint
        .segments
        .iter()
        .filter(|segment| segment.segment_type == "url")
        .count();
    let email_count = fingerprint
        .segments
        .iter()
        .filter(|segment| segment.segment_type == "email")
        .count();
    let emoji_count = fingerprint
        .segments
        .iter()
        .filter(|segment| segment.segment_type == "emoji")
        .count();
    let length = fingerprint.char_features.length.max(1) as f64;
    let letters = fingerprint.char_features.letters.max(1) as f64;
    let pattern_score = patterns
        .iter()
        .map(|pattern| pattern.score)
        .fold(0.0, f64::max);
    let decoded_similarity = fingerprint
        .rebus_candidates
        .iter()
        .map(|candidate| candidate.score)
        .fold(0.0, f64::max);
    SpamFeatures {
        url_count,
        email_count,
        emoji_count,
        number_ratio: fingerprint.char_features.digits as f64 / length,
        emoji_ratio: emoji_count as f64 / length,
        uppercase_ratio: fingerprint
            .raw
            .chars()
            .filter(|character| character.is_uppercase())
            .count() as f64
            / letters,
        punctuation_ratio: fingerprint.char_features.punctuation as f64 / length,
        entropy: character_entropy(&fingerprint.raw),
        repetition_score: fingerprint.obfuscation_features.repetition_score,
        obfuscation_score: fingerprint.obfuscation_features.score,
        pattern_score,
        decoded_similarity,
        semantic_pattern_similarity: pattern_score,
        mixed_scripts: fingerprint.unicode_features.mixed_scripts,
        confusable_count: fingerprint.unicode_features.confusable_characters.len(),
    }
}

/// Explicitly named fallback predictor. It is deterministic and explainable,
/// but reports `calibrated=false` until an application supplies evaluation
/// data and a learned predictor.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeuristicSpamPredictor;

impl SpamPredictor for HeuristicSpamPredictor {
    fn predict(
        &self,
        fingerprint: &MessageFingerprint,
        patterns: &[PatternMatch],
    ) -> Result<SpamResult, ProviderError> {
        Ok(predict_spam(fingerprint, patterns))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("heuristic_spam_v1")
    }
}

pub fn predict_spam(fingerprint: &MessageFingerprint, patterns: &[PatternMatch]) -> SpamResult {
    let features = spam_features(fingerprint, patterns);
    let mut probability = 0.05;
    probability += (features.url_count as f64 * 0.22).min(0.44);
    probability += (features.email_count as f64 * 0.10).min(0.20);
    probability += (features.emoji_ratio * 0.6).min(0.18);
    probability += (features.number_ratio * 0.5).min(0.15);
    probability += features.obfuscation_score * 0.30;
    probability += features.repetition_score * 0.20;
    probability += features.pattern_score * 0.25;
    probability += (features.uppercase_ratio * 0.08).min(0.08);
    probability += (features.punctuation_ratio * 0.12).min(0.12);
    probability = probability.clamp(0.0, 1.0);

    let mut labels = Vec::new();
    let mut reasons = Vec::new();
    if features.url_count > 0 {
        labels.push("promotion".to_string());
        reasons.push("Contains one or more URLs".to_string());
    }
    if fingerprint.obfuscation_features.leetspeak || fingerprint.obfuscation_features.confusables {
        labels.push("obfuscated".to_string());
        reasons.push("Obfuscation signals were detected".to_string());
    }
    if features.repetition_score > 0.08 {
        labels.push("flooding".to_string());
        reasons.push("Repeated characters suggest message flooding".to_string());
    }
    if features.pattern_score >= 0.75 {
        labels.push("known_pattern".to_string());
        reasons.push("Message matched a registered pattern".to_string());
    }
    if features.emoji_count >= 2 || features.number_ratio > 0.20 {
        labels.push("attention_grabbing".to_string());
        reasons.push("High emoji or number ratio".to_string());
    }
    if features.entropy > 4.5 && fingerprint.char_features.length >= 12 {
        labels.push("high_entropy".to_string());
        reasons.push("Message has an unusually diverse character distribution".to_string());
    }
    if labels.is_empty() && probability >= 0.5 {
        labels.push("suspicious".to_string());
        reasons.push("Combined spam signals exceeded the warning threshold".to_string());
    }
    SpamResult {
        probability,
        labels,
        reasons,
        features,
        calibrated: false,
        model: "heuristic-v1".to_string(),
    }
}

fn character_entropy(text: &str) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    let mut counts = std::collections::BTreeMap::<char, usize>::new();
    for character in text.chars() {
        *counts.entry(character).or_default() += 1;
    }
    let length = text.chars().count() as f64;
    counts
        .values()
        .map(|count| {
            let probability = *count as f64 / length;
            -probability * probability.log2()
        })
        .sum()
}
