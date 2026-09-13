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
    let query_text = query.normalized.as_deref().unwrap_or(&query.raw);
    let mut best: Option<(f64, String, Vec<String>)> = None;
    for example in &pattern.examples {
        let character = combined_character_similarity(query_text, example);
        let lexical = lexical_similarity(query_text, example);
        let visual = visual_similarity(query_text, example);
        let score = (0.45 * character + 0.30 * lexical + 0.25 * visual).clamp(0.0, 1.0);
        let explanations = vec![
            format!("character={character:.3}"),
            format!("lexical={lexical:.3}"),
            format!("visual={visual:.3}"),
        ];
        if best.as_ref().map_or(true, |current| score > current.0) {
            best = Some((score, example.clone(), explanations));
        }
    }
    let _ = weights;
    best.map(|(score, matched_example, explanations)| PatternMatch {
        id: pattern.id.clone(),
        score,
        matched_example,
        explanations,
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
    let mut best: Option<(f64, String, Vec<String>)> = None;
    for (example_text, example) in examples {
        let result = score_fingerprints(query, example, weights);
        if best
            .as_ref()
            .map_or(true, |current| result.score > current.0)
        {
            best = Some((result.score, example_text.clone(), result.explanations));
        }
    }
    best.map(|(score, matched_example, explanations)| PatternMatch {
        id: pattern.id.clone(),
        score,
        matched_example,
        explanations,
    })
}
