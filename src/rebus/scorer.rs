use crate::core::providers::LexiconProvider;
use crate::lexical::character::combined_character_similarity;
use crate::normalization::repetition::collapse_repetition;
use crate::resources::DefaultLexiconProvider;

fn known(text: &str, languages: Option<&[String]>, provider: &dyn LexiconProvider) -> bool {
    provider.contains(text, languages)
}

fn can_split_known(
    text: &str,
    depth: usize,
    max_depth: usize,
    languages: Option<&[String]>,
    provider: &dyn LexiconProvider,
) -> bool {
    if depth > max_depth || text.is_empty() {
        return false;
    }
    if known(text, languages, provider) {
        return true;
    }
    (2..text.len().saturating_sub(1)).any(|index| {
        text.is_char_boundary(index)
            && known(&text[..index], languages, provider)
            && can_split_known(&text[index..], depth + 1, max_depth, languages, provider)
    })
}

pub fn lexical_plausibility_with_limit(text: &str, max_recursion: usize) -> f64 {
    lexical_plausibility_with_provider(text, max_recursion, None, &DefaultLexiconProvider)
}

pub fn lexical_plausibility_with_provider(
    text: &str,
    max_recursion: usize,
    languages: Option<&[String]>,
    provider: &dyn LexiconProvider,
) -> f64 {
    let folded = collapse_repetition(&text.to_lowercase(), 1);
    let compact: String = folded.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.is_empty() {
        return 0.0;
    }
    if known(&compact, languages, provider)
        || can_split_known(&compact, 0, max_recursion, languages, provider)
    {
        return 1.0;
    }
    let parts = folded.split_whitespace().collect::<Vec<_>>();
    let hits = parts
        .iter()
        .filter(|part| known(part, languages, provider))
        .count();
    if !parts.is_empty() && hits > 0 {
        return 0.4 + 0.6 * hits as f64 / parts.len() as f64;
    }
    if provider.starts_with(&compact, languages) {
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
    score_candidate_with_provider(surface, prior, max_recursion, None, &DefaultLexiconProvider)
}

pub fn score_candidate_with_provider(
    surface: &str,
    prior: f64,
    max_recursion: usize,
    languages: Option<&[String]>,
    provider: &dyn LexiconProvider,
) -> (f64, f64, f64, f64) {
    let lexical = lexical_plausibility_with_provider(surface, max_recursion, languages, provider);
    let collapsed = collapse_repetition(&surface.to_lowercase(), 1);
    let phonetic = combined_character_similarity(&surface.to_lowercase(), &collapsed);
    let context = (0.5 + 0.5 * lexical).min(1.0);
    let symbol = prior.clamp(0.0, 1.0);
    let total = (0.45 * lexical + 0.2 * phonetic + 0.15 * context + 0.2 * symbol).clamp(0.0, 1.0);
    (total, lexical, phonetic, context)
}
