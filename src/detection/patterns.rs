use crate::comparison::scorer::score_fingerprints;
use crate::core::config::SimilarityWeights;
use crate::core::types::{MessageFingerprint, Pattern, PatternMatch};
use crate::lexical::character::combined_character_similarity;
use crate::lexical::similarity::lexical_similarity;
use crate::visual::similarity::visual_similarity;

pub fn match_pattern(
    query: &MessageFingerprint,
    pattern: &Pattern,
    weights: &SimilarityWeights,
) -> Option<PatternMatch> {
    if !pattern_languages_match(query, pattern) {
        return None;
    }
    let query_text = query.normalized.as_deref().unwrap_or(&query.raw);
    let mut best: Option<(f64, String, Vec<String>)> = None;
    for example in &pattern.examples {
        let character = combined_character_similarity(query_text, example);
        let lexical = lexical_similarity(query_text, example);
        let visual = visual_similarity(query_text, example);
        let score = enabled_score(pattern, character, lexical, visual, 0.0, 0.0, weights);
        let explanations = vec![
            format!("character={character:.3}"),
            format!("lexical={lexical:.3}"),
            format!("visual={visual:.3}"),
        ];
        if best.as_ref().is_none_or(|current| score > current.0) {
            best = Some((score, example.clone(), explanations));
        }
    }
    let _ = weights;
    let (score, matched_example, mut explanations) = best?;
    let negative_score = pattern
        .negative_examples
        .iter()
        .map(|example| {
            let character = combined_character_similarity(query_text, example);
            let lexical = lexical_similarity(query_text, example);
            let visual = visual_similarity(query_text, example);
            enabled_score(pattern, character, lexical, visual, 0.0, 0.0, weights)
        })
        .fold(0.0, f64::max);
    let adjusted = (score - negative_score * 0.5).clamp(0.0, 1.0);
    explanations.push(format!("negative={negative_score:.3}"));
    (adjusted >= pattern.threshold).then_some(PatternMatch {
        id: pattern.id.clone(),
        score: adjusted,
        matched_example,
        explanations,
        negative_score,
    })
}

/// Compare a pre-analyzed pattern example.  Kept separate so storage-backed
/// pattern indexes can avoid re-analyzing examples on every query.
pub fn match_pattern_fingerprint(
    query: &MessageFingerprint,
    pattern: &Pattern,
    examples: &[(String, MessageFingerprint)],
    weights: &SimilarityWeights,
) -> Option<PatternMatch> {
    if !pattern_languages_match(query, pattern) {
        return None;
    }
    let mut best: Option<(f64, String, Vec<String>)> = None;
    for (example_text, example) in examples {
        let result = score_fingerprints(query, example, weights);
        let score = enabled_fingerprint_score(pattern, &result);
        if best.as_ref().is_none_or(|current| score > current.0) {
            best = Some((score, example_text.clone(), result.explanations));
        }
    }
    let (score, matched_example, mut explanations) = best?;
    let negative_score = pattern
        .negative_examples
        .iter()
        .map(|negative| combined_character_similarity(&query.raw, negative))
        .fold(0.0, f64::max);
    let adjusted = (score - negative_score * 0.5).clamp(0.0, 1.0);
    explanations.push(format!("negative={negative_score:.3}"));
    (adjusted >= pattern.threshold).then_some(PatternMatch {
        id: pattern.id.clone(),
        score: adjusted,
        matched_example,
        explanations,
        negative_score,
    })
}

fn pattern_languages_match(query: &MessageFingerprint, pattern: &Pattern) -> bool {
    if pattern.languages.is_empty() {
        return true;
    }
    query.language_candidates.iter().any(|candidate| {
        pattern
            .languages
            .iter()
            .any(|language| language.eq_ignore_ascii_case(&candidate.language))
    })
}

fn enabled_score(
    pattern: &Pattern,
    character: f64,
    lexical: f64,
    visual: f64,
    semantic: f64,
    decoded: f64,
    weights: &SimilarityWeights,
) -> f64 {
    if pattern.enabled_channels.is_empty() {
        return (0.45 * character + 0.30 * lexical + 0.25 * visual).clamp(0.0, 1.0);
    }
    let values = [
        ("character", character, weights.character),
        ("lexical", lexical, weights.lexical),
        ("visual", visual, weights.visual),
        ("semantic", semantic, weights.semantic),
        ("decoded", decoded, weights.decoded),
    ];
    let selected = values
        .iter()
        .filter(|(name, _, weight)| {
            *weight > 0.0 && pattern.enabled_channels.iter().any(|value| value == name)
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return 0.0;
    }
    let total = selected.iter().map(|(_, _, weight)| *weight).sum::<f64>();
    selected
        .iter()
        .map(|(_, value, weight)| *value * *weight)
        .sum::<f64>()
        / total
}

fn enabled_fingerprint_score(
    pattern: &Pattern,
    result: &crate::core::types::ComparisonResult,
) -> f64 {
    if pattern.enabled_channels.is_empty() {
        return result.score;
    }
    let values = [
        ("semantic", result.semantic),
        ("lexical", result.lexical),
        ("character", result.character),
        ("visual", result.visual),
        ("phonetic", result.phonetic),
        ("symbolic", result.symbolic),
        ("decoded", result.decoded_similarity),
        ("obfuscation", result.obfuscation_similarity),
    ];
    let selected = values
        .iter()
        .filter_map(|(name, value)| {
            pattern
                .enabled_channels
                .iter()
                .any(|enabled| enabled == name)
                .then_some((*value)?)
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        0.0
    } else {
        selected.iter().sum::<f64>() / selected.len() as f64
    }
}
