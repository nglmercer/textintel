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
    // Transliteration views join the overlap set so cross-script pairs
    // (`privet` ↔ `привет`) match through the converted view. Only
    // transliteration views participate — other normalization views stay out
    // so leet/casefold variants cannot inflate decoded similarity. Every view
    // carries its provider confidence: a view match contributes
    // `similarity * confidence`, never an unconditional 1.0.
    let views_left: Vec<(String, f64)> = a
        .transliteration_views()
        .into_iter()
        .map(|(view, confidence)| (compact(&casefold_text(view)), confidence))
        .collect();
    let views_right: Vec<(String, f64)> = b
        .transliteration_views()
        .into_iter()
        .map(|(view, confidence)| (compact(&casefold_text(view)), confidence))
        .collect();
    // Phase 1a: exact base matches (raw/normalized/rebus, no conversion
    // involved) still short-circuit at 1.0.
    for left_value in left.iter() {
        if !left_value.is_empty() && right.contains(left_value) {
            return 1.0;
        }
    }
    // Phase 1b: exact matches involving a transliteration view contribute the
    // view confidence (similarity 1.0 times provider confidence). View↔view
    // matches take the weaker confidence; view↔base matches take the view's.
    let mut best: f64 = 0.0;
    for (view, confidence) in &views_left {
        if view.is_empty() {
            continue;
        }
        if right.contains(view) {
            best = best.max(*confidence);
        }
        for (other, other_confidence) in &views_right {
            if view == other {
                best = best.max(confidence.min(*other_confidence));
            }
        }
    }
    for (view, confidence) in &views_right {
        if view.is_empty() {
            continue;
        }
        if left.contains(view) {
            best = best.max(*confidence);
        }
    }
    // Phase 2: fuzzy overlap over the base sets (no views).
    for left_value in &left {
        if left_value.is_empty() {
            continue;
        }
        for right_value in &right {
            best = best.max(combined_character_similarity(left_value, right_value));
        }
    }
    // Phase 3: view↔raw rescue pairs run only when the base overlap is
    // below near-exact. Transliteration views exist to rescue cross-script
    // pairs; restricting the extra work to view↔raw/normalized pairs (a
    // handful per comparison) keeps same-script cost flat while still
    // matching `privet` against the `привет` → `privet` view. Pairs are
    // strictly cross-side: a view matching its own raw text proves nothing.
    if best < 0.9 && (!views_left.is_empty() || !views_right.is_empty()) {
        let anchors_left = [
            compact(&casefold_text(&a.raw)),
            compact(&casefold_text(a.normalized.as_deref().unwrap_or(""))),
        ];
        let anchors_right = [
            compact(&casefold_text(&b.raw)),
            compact(&casefold_text(b.normalized.as_deref().unwrap_or(""))),
        ];
        for (views, anchors) in [(&views_left, &anchors_right), (&views_right, &anchors_left)] {
            for (view, confidence) in views.iter() {
                if view.is_empty() {
                    continue;
                }
                for anchor in anchors.iter() {
                    if anchor.is_empty() {
                        continue;
                    }
                    best = best.max(combined_character_similarity(view, anchor) * confidence);
                }
            }
        }
    }
    best.clamp(0.0, 1.0)
}

fn obfuscation_similarity(a: &MessageFingerprint, b: &MessageFingerprint) -> f64 {
    let normalize = |text: &str| {
        normalize_whitespace(&collapse_repetition(&apply_leet(&casefold_text(text)), 1))
    };
    combined_character_similarity(&normalize(&a.raw), &normalize(&b.raw))
}

fn semantic_similarity(a: &MessageFingerprint, b: &MessageFingerprint) -> Option<f64> {
    // Whole-text evidence only. Segment and decoded embeddings are indexed
    // for retrieval explainability, but max-pooling over them would let a
    // single shared stop-word segment report any two messages as identical.
    match (
        a.semantic_embeddings.get("default"),
        b.semantic_embeddings.get("default"),
    ) {
        (Some(left), Some(right)) => Some(cosine(left, right)),
        _ => None,
    }
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
        // Non-finite and non-positive weights are skipped: hostile configs
        // must not NaN the final score.
        if !weight.is_finite() || *weight <= 0.0 {
            continue;
        }
        let value = if value.is_finite() { *value } else { 0.0 };
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
    let score = score / total_weight;
    (if score.is_finite() { score } else { 0.0 }, used)
}

/// Defense in depth: every channel is sanitized so hostile fingerprints
/// (non-finite confidences, NaN vectors) yield 0/absent instead of NaN.
/// Engine-built inputs are unaffected.
fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

fn finite_or_absent(value: Option<f64>) -> Option<f64> {
    value.filter(|inner| inner.is_finite())
}

pub fn score_fingerprints(
    a: &MessageFingerprint,
    b: &MessageFingerprint,
    weights: &SimilarityWeights,
) -> ComparisonResult {
    let character = finite_or_zero(combined_character_similarity(&a.raw, &b.raw));
    let lexical = finite_or_zero(lexical_similarity(&a.raw, &b.raw));
    let visual = finite_or_zero(visual_similarity(&a.raw, &b.raw));
    let decoded = finite_or_zero(best_decoded_overlap(a, b));
    let obfuscation = finite_or_zero(obfuscation_similarity(a, b));
    let symbolic = if a.symbols.is_empty() && b.symbols.is_empty() {
        None
    } else {
        finite_or_absent(Some(symbolic_similarity(a, b)))
    };
    let semantic = finite_or_absent(semantic_similarity(a, b));
    let phonetic = finite_or_absent(phonetic_channel(a, b));
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
    let transliteration = crate::transliteration::transliteration_evidence(a, b);
    match transliteration {
        Some(tr) => {
            evidence.push(format!("transliteration={:.3}", tr.weighted()));
            evidence.push(format!("transliteration_similarity={:.3}", tr.similarity));
            evidence.push(format!("transliteration_confidence={:.3}", tr.confidence));
        }
        None => evidence.push("transliteration=absent".to_string()),
    }
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
        transliteration_similarity: transliteration.map(|tr| tr.similarity),
        transliteration_confidence: transliteration.map(|tr| tr.confidence),
        obfuscation_similarity: Some(obfuscation),
        obfuscation: Some(obfuscation),
        explanations,
        evidence,
        weights_used,
        channel_confidence,
        channel_available,
    }
}
