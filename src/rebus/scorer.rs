use crate::lexical::character::combined_character_similarity;
use crate::normalization::repetition::collapse_repetition;
use crate::symbols::knowledge::WORDLIST;

fn known(text: &str) -> bool {
    WORDLIST.contains(&text)
}

fn can_split_known(text: &str, depth: usize, max_depth: usize) -> bool {
    if depth > max_depth || text.is_empty() {
        return false;
    }
    if known(text) {
        return true;
    }
    (2..text.len().saturating_sub(1)).any(|index| {
        text.is_char_boundary(index)
            && known(&text[..index])
            && can_split_known(&text[index..], depth + 1, max_depth)
    })
}

pub fn lexical_plausibility_with_limit(text: &str, max_recursion: usize) -> f64 {
    let folded = collapse_repetition(&text.to_lowercase(), 1);
    let compact: String = folded.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.is_empty() {
        return 0.0;
    }
    if known(&compact) || can_split_known(&compact, 0, max_recursion) {
        return 1.0;
    }
    let parts = folded.split_whitespace().collect::<Vec<_>>();
    let hits = parts.iter().filter(|part| known(part)).count();
    if !parts.is_empty() && hits > 0 {
        return 0.4 + 0.6 * hits as f64 / parts.len() as f64;
    }
    if WORDLIST
        .iter()
        .any(|word| word.starts_with(&compact) || compact.starts_with(word))
    {
        return 0.55;
    }
    0.15
}

pub fn score_candidate(surface: &str, prior: f64) -> (f64, f64, f64, f64) {
    score_candidate_with_limit(surface, prior, 4)
}

pub fn lexical_plausibility(text: &str) -> f64 {
    lexical_plausibility_with_limit(text, 4)
}

pub fn score_candidate_with_limit(
    surface: &str,
    prior: f64,
    max_recursion: usize,
) -> (f64, f64, f64, f64) {
    let lexical = lexical_plausibility_with_limit(surface, max_recursion);
    let collapsed = collapse_repetition(&surface.to_lowercase(), 1);
    let phonetic = combined_character_similarity(&surface.to_lowercase(), &collapsed);
    let context = (0.5 + 0.5 * lexical).min(1.0);
    let symbol = prior.clamp(0.0, 1.0);
    let total = (0.45 * lexical + 0.2 * phonetic + 0.15 * context + 0.2 * symbol).clamp(0.0, 1.0);
    (total, lexical, phonetic, context)
}
