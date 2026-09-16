use std::collections::BTreeMap;

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

mod decoded;
mod swap;

use decoded::{best_decoded_overlap, exact_decode_confidence};
use swap::{
    same_language_swap, swapped_phonetic_similarity, swapped_word_similarity, swapped_words,
};
pub(crate) fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

/// Alphabetic tokens of a fingerprint: segments without a letter (URLs,
/// emoji, pure numbers) are not words and are excluded, mirroring
/// `lexicon_coverage`.
pub(crate) fn word_tokens(fingerprint: &MessageFingerprint) -> Vec<&str> {
    fingerprint
        .tokens
        .iter()
        .map(String::as_str)
        .filter(|token| token.chars().any(|ch| ch.is_alphabetic()))
        .collect()
}

fn is_cjk(character: char) -> bool {
    matches!(
        character,
        '\u{3400}'..='\u{4dbf}'
            | '\u{4e00}'..='\u{9fff}'
            | '\u{3040}'..='\u{309f}'
            | '\u{30a0}'..='\u{30ff}'
            | '\u{ac00}'..='\u{d7af}'
    )
}

/// A single alphabetic token scopes confusable evidence — except in the
/// spaceless CJK scripts, where one token of three or more characters is a
/// phrase or compound (`早上好`), not a word (CJK words run one or two
/// characters). Without this, every short CJK pair would read as a
/// single-word confusable.
fn is_single_word(fingerprint: &MessageFingerprint) -> bool {
    let words = word_tokens(fingerprint);
    if words.len() != 1 {
        return false;
    }
    words[0].chars().filter(|ch| is_cjk(*ch)).count() < 3
}

fn alphanumeric_fold(text: &str) -> String {
    casefold_text(text)
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

/// Substring containment between the alphanumeric folds: 1.0 when they
/// differ and one contains the other, else 0.0. Short-word containment is
/// meaningless (`he` in `the`), so the shorter side needs at least three
/// characters — except non-Latin text, where substring is the natural
/// overlap for unsegmented script (`早上` in `早上好`). Equal folds are
/// `normalized_identity`'s job, not this feature's. Impure by design
/// (`carpet` sits inside `carpeta`, `soy` inside `soy sauce`): the
/// learned weight prices the net value, and short confusables stay out
/// by the length gate.
fn substring_containment(a: &MessageFingerprint, b: &MessageFingerprint) -> f64 {
    let left = alphanumeric_fold(&a.raw);
    let right = alphanumeric_fold(&b.raw);
    if left.is_empty() || right.is_empty() || left == right {
        return 0.0;
    }
    let (shorter, longer) = if left.len() <= right.len() {
        (left.as_str(), right.as_str())
    } else {
        (right.as_str(), left.as_str())
    };
    if !longer.contains(shorter) {
        return 0.0;
    }
    let long_enough = shorter.chars().count() >= 3;
    let non_latin = shorter
        .chars()
        .any(|ch| ch.is_alphanumeric() && !ch.is_ascii());
    if long_enough || non_latin {
        1.0
    } else {
        0.0
    }
}

fn normalized_identity(a: &MessageFingerprint, b: &MessageFingerprint) -> f64 {
    let left = alphanumeric_fold(&a.raw);
    let right = alphanumeric_fold(&b.raw);
    if !left.is_empty() && left == right {
        1.0
    } else {
        0.0
    }
}

fn cross_script_pair(a: &MessageFingerprint, b: &MessageFingerprint) -> f64 {
    let left = crate::visual::scripts::scripts_in(&a.raw);
    let right = crate::visual::scripts::scripts_in(&b.raw);
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    if left.iter().any(|script| right.contains(script)) {
        0.0
    } else {
        1.0
    }
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
    let lexicon_validity = finite_or_zero(a.lexicon_coverage.min(b.lexicon_coverage));
    let single_word_pair = if is_single_word(a) && is_single_word(b) {
        1.0
    } else {
        0.0
    };
    let swapped_word_similarity = finite_or_zero(swapped_word_similarity(a, b));
    let swapped_phonetic_raw = finite_or_zero(swapped_phonetic_similarity(a, b));
    // Cross-language swaps (`mi`/`my`) are switches, not confusables:
    // phonetic-suspicion evidence is meaningless across languages.
    let same_language =
        swapped_words(a, b).map_or(1.0, |(position, _, _)| same_language_swap(a, b, position));
    // Gate phonetic-swap evidence on pair lexicon validity: phonemizing
    // leetspeak tokens (`gr8`→"gr eight" vs `grate`) manufactures similarity
    // out of glyph accidents, so the signal only counts when both sides read
    // as real words.
    let swapped_phonetic = swapped_phonetic_raw * lexicon_validity.clamp(0.0, 1.0) * same_language;
    // Confusable-swap interaction (see `ComparisonResult::confusable_swap`).
    // Coverage is an exact 1.0 exactly when every word is known, so the
    // 0.999 cut is an exact all-valid test with float slack.
    let all_valid = if lexicon_validity > 0.999 { 1.0 } else { 0.0 };
    let confusable_swap =
        (swapped_word_similarity * swapped_phonetic_raw * all_valid * same_language)
            .clamp(0.0, 1.0);
    let normalized_identity = normalized_identity(a, b);
    let substring_containment = substring_containment(a, b);
    let cross_script_pair = cross_script_pair(a, b);
    let cross_script_agreement =
        cross_script_pair * crate::comparison::model::language_agreement(a, b);
    let exact_decode = finite_or_zero(exact_decode_confidence(a, b));
    // Single-word exact decoding: a lone exact read (`cheque`→`check`) is
    // the variant signature, while single-word confusables (`their`/`there`)
    // decode to nothing — the linear model cannot express this interaction
    // from `single_word_pair` and `exact_decode` alone.
    let single_word_exact = (single_word_pair * exact_decode).clamp(0.0, 1.0);
    let mut evidence = vec![
        format!("character={character:.3}"),
        format!("lexical={lexical:.3}"),
        format!("visual={visual:.3}"),
        format!("lexicon_validity={lexicon_validity:.3}"),
        format!("single_word_pair={single_word_pair:.0}"),
        format!("swapped_word_similarity={swapped_word_similarity:.3}"),
        format!("swapped_phonetic={swapped_phonetic:.3}"),
        format!("confusable_swap={confusable_swap:.3}"),
        format!("normalized_identity={normalized_identity:.0}"),
        format!("substring_containment={substring_containment:.0}"),
        format!("cross_script_pair={cross_script_pair:.0}"),
        format!("cross_script_agreement={cross_script_agreement:.0}"),
        format!("exact_decode={exact_decode:.3}"),
        format!("single_word_exact={single_word_exact:.3}"),
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
        lexicon_validity,
        single_word_pair,
        swapped_word_similarity,
        swapped_phonetic,
        confusable_swap,
        normalized_identity,
        substring_containment,
        cross_script_pair,
        cross_script_agreement,
        exact_decode,
        single_word_exact,
        explanations,
        evidence,
        weights_used,
        channel_confidence,
        channel_available,
    }
}
