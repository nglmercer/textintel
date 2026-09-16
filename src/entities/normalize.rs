//! Shared entity-extraction helpers: span claiming, value normalization,
//! and mention construction.

use crate::core::types::EntityMention;

pub(crate) fn is_claimed(claimed: &[bool], start: usize, end: usize) -> bool {
    claimed[start..end].iter().any(|flag| *flag)
}

pub(crate) fn claim(claimed: &mut [bool], start: usize, end: usize) {
    for flag in &mut claimed[start..end] {
        *flag = true;
    }
}

pub(crate) fn trim_trailing_punctuation(value: &str) -> &str {
    value.trim_end_matches([
        '.', ',', ';', ':', '!', '?', ')', ']', '}', '\'', '"', '’', '»',
    ])
}

pub(crate) fn casefold(value: &str) -> String {
    crate::normalization::unicode::casefold_text(value)
}

pub(crate) fn push_mention(
    mentions: &mut Vec<EntityMention>,
    entity_type: &str,
    value: String,
    start: usize,
    end: usize,
    confidence: f64,
    language: Option<&str>,
) {
    if value.is_empty() || start >= end {
        return;
    }
    mentions.push(EntityMention {
        entity_type: entity_type.to_string(),
        value,
        start,
        end,
        confidence: confidence.clamp(0.0, 1.0),
        provider: "rule_based_entities".to_string(),
        language: language.map(str::to_string),
    });
}
