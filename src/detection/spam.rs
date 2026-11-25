use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::SpamPredictor;
use crate::core::types::{MessageFingerprint, PatternMatch, SpamFeatures, SpamResult};

/// Extract stable, serializable abuse evidence independently of scoring.
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

/// Default baked predictor: a deterministic logistic calibration over the
/// explainable spam feature vector, trained on the `v2-train` split of the
/// bundled UCI SMS subset (see `tools/train_spam/`). Applications with their
/// own evaluation data can replace it through the `SpamPredictor` boundary.
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
        ProviderCapabilities::new("logistic_spam_v2").with_quality(CapabilityLevel::Basic)
    }
}

/// Baked logistic calibration for the spam feature vector, trained on the
/// `data/spam/v2-train.json` split of the UCI SMS spam subset with the
/// deterministic NumPy trainer in `tools/train_spam/`. Measured on the
/// held-out `v2-eval.json` split: accuracy 0.969, F1 0.968, ROC-AUC 0.998.
/// The weights are compile-time constants; inference is deterministic,
/// allocation-free linear algebra over the explainable feature vector.
const SPAM_MODEL_BIAS: f64 = -16.395_197_164_859_592;
const SPAM_MODEL_WEIGHTS: [(&str, f64); 14] = [
    ("confusable_count", 0.0),
    ("decoded_similarity", -7.797_349),
    ("email_count", 1.220_420),
    ("emoji_count", -0.141_010),
    ("emoji_ratio", -8.148_359),
    ("entropy", 5.206_083),
    ("mixed_scripts", 0.0),
    ("number_ratio", 23.574_646),
    ("obfuscation_score", 2.547_626),
    ("pattern_score", 0.0),
    ("punctuation_ratio", -32.991_350),
    ("repetition_score", -22.841_607),
    ("uppercase_ratio", -0.768_697),
    ("url_count", 6.233_678),
];

fn spam_feature_value(features: &SpamFeatures, name: &str) -> f64 {
    match name {
        "confusable_count" => features.confusable_count as f64,
        "decoded_similarity" => features.decoded_similarity,
        "email_count" => features.email_count as f64,
        "emoji_count" => features.emoji_count as f64,
        "emoji_ratio" => features.emoji_ratio,
        "entropy" => features.entropy,
        "mixed_scripts" => f64::from(features.mixed_scripts),
        "number_ratio" => features.number_ratio,
        "obfuscation_score" => features.obfuscation_score,
        "pattern_score" => features.pattern_score,
        "punctuation_ratio" => features.punctuation_ratio,
        "repetition_score" => features.repetition_score,
        "uppercase_ratio" => features.uppercase_ratio,
        "url_count" => features.url_count as f64,
        _ => 0.0,
    }
}

pub fn predict_spam(fingerprint: &MessageFingerprint, patterns: &[PatternMatch]) -> SpamResult {
    let features = spam_features(fingerprint, patterns);
    let logit = SPAM_MODEL_WEIGHTS
        .iter()
        .map(|(name, weight)| spam_feature_value(&features, name) * weight)
        .sum::<f64>()
        + SPAM_MODEL_BIAS;
    let probability = 1.0 / (1.0 + (-logit).exp());

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
        calibrated: true,
        model: "logistic-spam-v2".to_string(),
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
