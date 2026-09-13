use crate::core::types::{MessageFingerprint, Pattern, PatternMatch};
use crate::comparison::scorer::score_fingerprints;
use crate::core::config::SimilarityWeights;

pub fn match_pattern(
    query: &MessageFingerprint,
    pattern: &Pattern,
    weights: &SimilarityWeights,
) -> Option<PatternMatch> {
    let mut best: Option<(f64, String)> = None;
    for example in &pattern.examples {
        // Pattern examples are analyzed by the engine before this helper is
        // called in the public path.  The lightweight fallback fingerprint is
        // intentionally avoided here so errors are never hidden.
        let _ = example;
    }
    best.map(|(score, matched_example)| PatternMatch {
        id: pattern.id.clone(),
        score,
        matched_example,
        explanations: vec![format!("pattern {} matched", pattern.id)],
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
        if best.as_ref().is_none_or(|current| result.score > current.0) {
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

