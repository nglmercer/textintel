use crate::core::providers::SymbolKnowledgeProvider;
use crate::core::types::SymbolReading;
use crate::language::segmentation::segment_message;
use crate::normalization::leetspeak::{apply_leet, leet_map};
use crate::symbols::knowledge::DefaultSymbolKnowledge;

/// Tokenize a rebus into text, number, emoji, and punctuation pieces.  The
/// language segmenter already preserves byte positions and grapheme clusters.
pub fn rebus_tokens(text: &str) -> Vec<String> {
    segment_message(text, 4096)
        .into_iter()
        .map(|segment| segment.text)
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
        .map(|value| SymbolReading::new(*value, Some("und"), 0.55, "leetspeak"))
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
    } else if let Some(readings) = chat_readings(token, max_readings) {
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

fn chat_readings(token: &str, max_readings: usize) -> Option<Vec<(String, f64, String, String)>> {
    let folded = token.to_ascii_lowercase();
    let values: &[(&str, &str, f64)] = match folded.as_str() {
        "u" => &[("you", "en", 0.55), ("tu", "es", 0.35)],
        "r" => &[("are", "en", 0.55)],
        "ur" => &[("your", "en", 0.55), ("you're", "en", 0.40)],
        "b4" => &[("before", "en", 0.60)],
        "gr8" => &[("great", "en", 0.65)],
        "l8r" => &[("later", "en", 0.65)],
        _ => return None,
    };
    Some(
        values
            .iter()
            .take(max_readings.max(1))
            .map(|(text, language, probability)| {
                (
                    (*text).to_string(),
                    *probability,
                    (*language).to_string(),
                    "chat_abbreviation".to_string(),
                )
            })
            .collect(),
    )
}

pub fn token_readings(token: &str, max_readings: usize) -> Vec<(String, f64, String, String)> {
    token_readings_with_provider(token, max_readings, &DefaultSymbolKnowledge)
}
