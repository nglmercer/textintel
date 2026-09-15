use crate::core::providers::{AbbreviationProvider, SymbolKnowledgeProvider};
use crate::core::types::SymbolReading;
use crate::language::segmentation::segment_message;
use crate::normalization::leetspeak::{apply_leet, leet_map};
use crate::resources::embedded_resources;
use crate::symbols::knowledge::DefaultSymbolKnowledge;

/// Tokenize a rebus into text, number, emoji, and punctuation pieces.  The
/// language segmenter already preserves byte positions and grapheme clusters.
pub fn rebus_tokens(text: &str) -> Vec<String> {
    rebus_tokens_with_spans(text)
        .into_iter()
        .map(|(token, _, _)| token)
        .collect()
}

/// [`rebus_tokens`] plus UTF-8 byte offsets (`start..end`) into `text`, so
/// transformation provenance can point at the original span.
///
/// Unlike the language segmenter (which drops whitespace), inter-segment
/// gaps are preserved as literal space tokens: an input space is faithful
/// structure, and its preservation must not pay the hypothesized-boundary
/// penalty.
pub fn rebus_tokens_with_spans(text: &str) -> Vec<(String, usize, usize)> {
    let segments = segment_message(text, 4096);
    let mut tokens = Vec::with_capacity(segments.len() + 2);
    let mut cursor = 0usize;
    for segment in &segments {
        if cursor < segment.start {
            tokens.push((
                text[cursor..segment.start].to_string(),
                cursor,
                segment.start,
            ));
        }
        tokens.push((segment.text.clone(), segment.start, segment.end));
        cursor = segment.end;
    }
    if cursor < text.len() {
        tokens.push((text[cursor..].to_string(), cursor, text.len()));
    }
    tokens
        .into_iter()
        .filter(|(token, _, _)| !token.is_empty())
        .collect()
}

fn fallback_leet_readings(token: &str, max_readings: usize) -> Vec<SymbolReading> {
    if token.chars().count() != 1
        || !leet_map().contains_key(&token.chars().next().unwrap_or_default())
    {
        return Vec::new();
    }
    leet_map()
        .get(&token.chars().next().unwrap_or_default())
        .into_iter()
        .flat_map(|values| values.iter())
        .take(max_readings)
        .map(|value| {
            SymbolReading::new(*value, Some("und"), 0.55, "leetspeak")
                .with_source("builtin:leet-map")
        })
        .collect()
}

pub fn token_readings_with_provider(
    token: &str,
    max_readings: usize,
    provider: &dyn SymbolKnowledgeProvider,
) -> Vec<(String, f64, String, String)> {
    token_readings_with_provider_and_languages(token, max_readings, provider, None)
}

pub fn token_readings_with_provider_and_languages(
    token: &str,
    max_readings: usize,
    provider: &dyn SymbolKnowledgeProvider,
    languages: Option<&[String]>,
) -> Vec<(String, f64, String, String)> {
    token_readings_with_abbreviation_provider(token, max_readings, provider, None, languages)
}

/// Full variant with an explicit abbreviation source. `abbreviations` of
/// `None` uses the embedded versioned abbreviation packs; custom packs let
/// applications override or extend chat mappings without touching core code.
pub fn token_readings_with_abbreviation_provider(
    token: &str,
    max_readings: usize,
    provider: &dyn SymbolKnowledgeProvider,
    abbreviations: Option<&dyn AbbreviationProvider>,
    languages: Option<&[String]>,
) -> Vec<(String, f64, String, String)> {
    // Literal whitespace (preserved input gaps) passes through with a single
    // identity reading: no casefold duplicate, no boundary penalty.
    if !token.is_empty() && token.chars().all(char::is_whitespace) {
        return vec![(
            token.to_string(),
            1.0,
            "und".to_string(),
            "identity".to_string(),
        )];
    }
    let symbol_readings = provider.readings_in_languages(token, max_readings, languages);
    let filtered = symbol_readings
        .into_iter()
        .filter(|reading| language_allowed(reading.language.as_deref(), languages))
        .collect::<Vec<_>>();
    if !filtered.is_empty() {
        return filtered
            .into_iter()
            .map(|reading| {
                (
                    reading.text,
                    reading.probability,
                    reading.language.unwrap_or_else(|| "und".to_string()),
                    reading.reading_type,
                )
            })
            .collect();
    }
    let leet_readings = fallback_leet_readings(token, max_readings);
    if !leet_readings.is_empty() {
        return leet_readings
            .into_iter()
            .map(|reading| {
                (
                    reading.text,
                    reading.probability,
                    reading.language.unwrap_or_else(|| "und".to_string()),
                    reading.reading_type,
                )
            })
            .collect();
    }

    // If a token contains digits alongside letters, keep identity and add a
    // leet view.  This creates a bounded set of alternatives without making a
    // global destructive replacement.
    if token.chars().any(char::is_numeric) && token.chars().any(char::is_alphabetic) {
        let folded = apply_leet(token);
        vec![
            (
                token.to_string(),
                0.40,
                "und".to_string(),
                "identity".to_string(),
            ),
            (
                folded.clone(),
                0.75,
                "und".to_string(),
                "leetspeak".to_string(),
            ),
            (
                folded.to_lowercase(),
                0.80,
                "und".to_string(),
                "leetspeak".to_string(),
            ),
        ]
    } else if let Some(readings) =
        abbreviation_lookup(token, max_readings, abbreviations, languages)
    {
        readings
    } else {
        vec![
            (
                token.to_string(),
                1.0,
                "und".to_string(),
                "identity".to_string(),
            ),
            (
                token.to_lowercase(),
                0.95,
                "und".to_string(),
                "casefold".to_string(),
            ),
        ]
    }
}

fn language_allowed(language: Option<&str>, languages: Option<&[String]>) -> bool {
    let Some(languages) = languages.filter(|values| !values.is_empty()) else {
        return true;
    };
    let Some(language) = language else {
        return true;
    };
    language == "und"
        || languages.iter().any(|candidate| {
            candidate.eq_ignore_ascii_case(language)
                || candidate.eq_ignore_ascii_case("unknown")
                || candidate.eq_ignore_ascii_case("und")
        })
}

/// Chat-abbreviation expansions from versioned resource packs. Core holds no
/// language-specific mapping: every `(token → reading)` pair comes from an
/// `AbbreviationProvider` (embedded packs by default, custom packs on demand).
fn abbreviation_lookup(
    token: &str,
    max_readings: usize,
    abbreviations: Option<&dyn AbbreviationProvider>,
    languages: Option<&[String]>,
) -> Option<Vec<(String, f64, String, String)>> {
    let readings = match abbreviations {
        Some(provider) => provider.abbreviation_readings(token, languages, max_readings),
        None => embedded_resources().abbreviation_readings(token, languages, max_readings),
    };
    if readings.is_empty() {
        return None;
    }
    Some(
        readings
            .into_iter()
            .map(|reading| {
                (
                    reading.text,
                    reading.probability,
                    reading.language.unwrap_or_else(|| "und".to_string()),
                    reading.reading_type,
                )
            })
            .collect(),
    )
}

pub fn token_readings(token: &str, max_readings: usize) -> Vec<(String, f64, String, String)> {
    token_readings_with_provider(token, max_readings, &DefaultSymbolKnowledge)
}
