use crate::core::providers::{G2PProvider, LexiconProvider};
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
    score_candidate_with_g2p(
        surface,
        surface,
        prior,
        max_recursion,
        languages,
        provider,
        &crate::phonetic::g2p::RuleBasedG2PProvider,
    )
}

pub fn score_candidate_with_g2p(
    surface: &str,
    source: &str,
    prior: f64,
    max_recursion: usize,
    languages: Option<&[String]>,
    provider: &dyn LexiconProvider,
    g2p_provider: &dyn G2PProvider,
) -> (f64, f64, f64, f64) {
    let lexical = lexical_plausibility_with_provider(surface, max_recursion, languages, provider);
    let frequency = provider.frequency(surface, languages).unwrap_or(0.0);
    let frequency_bonus = (frequency.ln_1p() / 5.0).clamp(0.0, 0.25);
    let lexical = (lexical + frequency_bonus).min(1.0);
    let phonetic = g2p_similarity(surface, source, languages, g2p_provider).unwrap_or_else(|| {
        let collapsed = collapse_repetition(&surface.to_lowercase(), 1);
        combined_character_similarity(&surface.to_lowercase(), &collapsed)
    });
    let context = context_score(surface, lexical, provider, languages);
    let symbol = prior.clamp(0.0, 1.0);
    let total = (0.42 * lexical + 0.25 * phonetic + 0.13 * context + 0.20 * symbol).clamp(0.0, 1.0);
    (total, lexical, phonetic, context)
}

fn context_score(
    surface: &str,
    lexical: f64,
    provider: &dyn LexiconProvider,
    languages: Option<&[String]>,
) -> f64 {
    let compact = surface.chars().filter(|ch| !ch.is_whitespace()).count();
    let boundary = if compact == 0 {
        0.0
    } else if provider.contains(surface, languages) {
        1.0
    } else if provider.starts_with(surface, languages) {
        0.65
    } else {
        0.35
    };
    (0.55 * lexical + 0.45 * boundary).clamp(0.0, 1.0)
}

fn g2p_similarity(
    surface: &str,
    source: &str,
    languages: Option<&[String]>,
    provider: &dyn G2PProvider,
) -> Option<f64> {
    let language = languages
        .and_then(|values| values.iter().find(|value| value.as_str() != "unknown"))
        .map(String::as_str)
        .unwrap_or("und");
    let candidate = provider.phonemize(surface, language).ok()?;
    let source = provider.phonemize(source, language).ok()?;
    if candidate.phonemes.is_empty() || source.phonemes.is_empty() {
        return None;
    }
    Some(crate::phonetic::similarity::phonetic_similarity(
        &candidate, &source,
    ))
}
