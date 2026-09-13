use crate::core::types::{MessageFingerprint, PatternMatch, SpamResult};

pub fn predict_spam(fingerprint: &MessageFingerprint, patterns: &[PatternMatch]) -> SpamResult {
    let url_count = fingerprint.segments.iter().filter(|segment| segment.segment_type == "url").count();
    let email_count = fingerprint.segments.iter().filter(|segment| segment.segment_type == "email").count();
    let emoji_count = fingerprint.segments.iter().filter(|segment| segment.segment_type == "emoji").count();
    let digits = fingerprint.char_features.digits;
    let length = fingerprint.char_features.length.max(1);
    let emoji_ratio = emoji_count as f64 / length as f64;
    let number_ratio = digits as f64 / length as f64;
    let repetition = fingerprint.obfuscation_features.repetition_score;
    let obfuscation = fingerprint.obfuscation_features.score;
    let pattern_score = patterns.iter().map(|pattern| pattern.score).fold(0.0, f64::max);
    let mut probability = 0.05;
    probability += (url_count as f64 * 0.22).min(0.44);
    probability += (email_count as f64 * 0.10).min(0.20);
    probability += (emoji_ratio * 0.6).min(0.18);
    probability += (number_ratio * 0.5).min(0.15);
    probability += obfuscation * 0.30;
    probability += repetition * 0.20;
    probability += pattern_score * 0.25;
    probability = probability.clamp(0.0, 1.0);

    let mut labels = Vec::new();
    let mut reasons = Vec::new();
    if url_count > 0 { labels.push("promotion".to_string()); reasons.push("Contains one or more URLs".to_string()); }
    if fingerprint.obfuscation_features.leetspeak || fingerprint.obfuscation_features.confusables {
        labels.push("obfuscated".to_string());
        reasons.push("Obfuscation signals were detected".to_string());
    }
    if repetition > 0.08 { labels.push("flooding".to_string()); reasons.push("Repeated characters suggest message flooding".to_string()); }
    if pattern_score >= 0.75 {
        labels.push("known_pattern".to_string());
        reasons.push("Message matched a registered pattern".to_string());
    }
    if emoji_count >= 2 || number_ratio > 0.20 {
        labels.push("attention_grabbing".to_string());
        reasons.push("High emoji or number ratio".to_string());
    }
    if labels.is_empty() && probability >= 0.5 {
        labels.push("suspicious".to_string());
        reasons.push("Combined spam signals exceeded the warning threshold".to_string());
    }
    SpamResult { probability, labels, reasons }
}

