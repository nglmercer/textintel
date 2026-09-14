use std::collections::{BTreeMap, BTreeSet};

use crate::core::config::SimilarityWeights;
use crate::core::types::{ComparisonResult, MessageFingerprint};
use crate::lexical::character::combined_character_similarity;
use crate::lexical::similarity::lexical_similarity;
use crate::normalization::leetspeak::apply_leet;
use crate::normalization::repetition::collapse_repetition;
use crate::normalization::unicode::casefold_text;
use crate::normalization::whitespace::normalize_whitespace;
use crate::phonetic::similarity::phonetic_similarity;
use crate::semantic::similarity::cosine;
use crate::symbols::resolver::symbolic_similarity;
use crate::visual::similarity::visual_similarity;

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn best_decoded_overlap(a: &MessageFingerprint, b: &MessageFingerprint) -> f64 {
    let mut left = BTreeSet::new();
    let mut right = BTreeSet::new();
    for value in [a.raw.clone(), a.normalized.clone().unwrap_or_default()] {
        left.insert(compact(&casefold_text(&value)));
    }
    for value in [b.raw.clone(), b.normalized.clone().unwrap_or_default()] {
        right.insert(compact(&casefold_text(&value)));
    }
    for candidate in &a.rebus_candidates {
        left.insert(compact(&casefold_text(&candidate.text)));
    }
    for candidate in &b.rebus_candidates {
        right.insert(compact(&casefold_text(&candidate.text)));
    }
    let mut best: f64 = 0.0;
    for left_value in &left {
        if left_value.is_empty() {
            continue;
        }
        for right_value in &right {
            if left_value == right_value {
                return 1.0;
            }
            best = best.max(combined_character_similarity(left_value, right_value));
        }
    }
    best
}

fn obfuscation_similarity(a: &MessageFingerprint, b: &MessageFingerprint) -> f64 {
    let normalize = |text: &str| {
        normalize_whitespace(&collapse_repetition(&apply_leet(&casefold_text(text)), 1))
    };
    combined_character_similarity(&normalize(&a.raw), &normalize(&b.raw))
}

fn semantic_similarity(a: &MessageFingerprint, b: &MessageFingerprint) -> Option<f64> {
    let mut best: Option<f64> = None;
    for left in a.semantic_embeddings.values() {
        for right in b.semantic_embeddings.values() {
            best = Some(best.map_or_else(
                || cosine(left, right),
                |value| value.max(cosine(left, right)),
            ));
        }
    }
    best
}

fn phonetic_channel(a: &MessageFingerprint, b: &MessageFingerprint) -> Option<f64> {
    let mut best: Option<f64> = None;
    for left in &a.phonetic_candidates {
        for right in &b.phonetic_candidates {
            if left.phonemes.is_empty() || right.phonemes.is_empty() {
                continue;
            }
            let score = phonetic_similarity(left, right);
            best = Some(best.map_or(score, |value| value.max(score)));
        }
    }
    best
}

pub fn combine_scores(
    channels: &BTreeMap<String, Option<f64>>,
    weights: &SimilarityWeights,
) -> (f64, BTreeMap<String, f64>) {
    let weight_map = weights.as_map();
    let mut total_weight = 0.0;
    let mut score = 0.0;
    let mut used = BTreeMap::new();
    for (name, value) in channels {
        let Some(value) = value else {
            continue;
        };
        let Some(weight) = weight_map.get(name) else {
            continue;
        };
        if *weight <= 0.0 {
            continue;
        }
        score += value.clamp(0.0, 1.0) * weight;
        total_weight += weight;
        used.insert(name.clone(), *weight);
    }
    if total_weight == 0.0 {
        return (0.0, used);
    }
    for value in used.values_mut() {
        *value /= total_weight;
    }
    (score / total_weight, used)
}

pub fn score_fingerprints(
    a: &MessageFingerprint,
    b: &MessageFingerprint,
    weights: &SimilarityWeights,
) -> ComparisonResult {
    let character = combined_character_similarity(&a.raw, &b.raw);
    let lexical = lexical_similarity(&a.raw, &b.raw);
    let visual = visual_similarity(&a.raw, &b.raw);
    let decoded = best_decoded_overlap(a, b);
    let obfuscation = obfuscation_similarity(a, b);
    let symbolic = if a.symbols.is_empty() && b.symbols.is_empty() {
        None
    } else {
        Some(symbolic_similarity(a, b))
    };
    let semantic = semantic_similarity(a, b);
    let phonetic = phonetic_channel(a, b);
    let channels = [
        ("semantic".to_string(), semantic),
        ("lexical".to_string(), Some(lexical)),
        ("character".to_string(), Some(character)),
        ("visual".to_string(), Some(visual)),
        ("phonetic".to_string(), phonetic),
        ("symbolic".to_string(), symbolic),
        ("decoded".to_string(), Some(decoded)),
        ("obfuscation".to_string(), Some(obfuscation)),
    ]
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    let (score, weights_used) = combine_scores(&channels, weights);
    let channel_available = channels
        .iter()
        .map(|(name, value)| (name.clone(), value.is_some()))
        .collect::<BTreeMap<_, _>>();
    let channel_confidence = channels
        .iter()
        .map(|(name, value)| {
            let confidence = if value.is_none() {
                0.0
            } else {
                let left = a
                    .channel_availability
                    .get(name)
                    .map(|state| state.confidence)
                    .unwrap_or(1.0);
                let right = b
                    .channel_availability
                    .get(name)
                    .map(|state| state.confidence)
                    .unwrap_or(1.0);
                left.min(right).clamp(0.0, 1.0)
            };
            (name.clone(), confidence)
        })
        .collect::<BTreeMap<_, _>>();
    let mut evidence = vec![
        format!("character={character:.3}"),
        format!("lexical={lexical:.3}"),
        format!("visual={visual:.3}"),
        format!(
            "symbolic={}",
            symbolic.map_or_else(|| "absent".to_string(), |value| format!("{value:.3}"))
        ),
        format!("decoded={decoded:.3}"),
        format!("obfuscation_norm={obfuscation:.3}"),
    ];
    evidence.push(match semantic {
        Some(value) => format!("semantic={value:.3}"),
        None => "semantic=absent".to_string(),
    });
    evidence.push(match phonetic {
        Some(value) => format!("phonetic={value:.3}"),
        None => "phonetic=absent".to_string(),
    });
    if a.obfuscation_features.detected {
        evidence.push(format!(
            "obfuscation_flags_a={:?}",
            a.obfuscation_features.flags
        ));
    }
    if b.obfuscation_features.detected {
        evidence.push(format!(
            "obfuscation_flags_b={:?}",
            b.obfuscation_features.flags
        ));
    }
    if !a.unicode_features.confusable_characters.is_empty()
        || !b.unicode_features.confusable_characters.is_empty()
    {
        evidence.push("confusable_unicode".to_string());
    }
    let explanations = evidence.clone();
    ComparisonResult {
        score,
        semantic,
        lexical: Some(lexical),
        character: Some(character),
        visual: Some(visual),
        phonetic,
        symbolic,
        decoded_similarity: Some(decoded),
        obfuscation_similarity: Some(obfuscation),
        obfuscation: Some(obfuscation),
        explanations,
        evidence,
        weights_used,
        channel_confidence,
        channel_available,
    }
}
