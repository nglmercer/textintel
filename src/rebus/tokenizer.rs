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
    let symbol_readings = provider.readings(token, max_readings);
    if !symbol_readings.is_empty() {
        return symbol_readings
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

pub fn token_readings(token: &str, max_readings: usize) -> Vec<(String, f64, String, String)> {
    token_readings_with_provider(token, max_readings, &DefaultSymbolKnowledge)
}
