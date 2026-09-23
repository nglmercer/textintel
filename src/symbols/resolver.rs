use crate::core::providers::SymbolKnowledgeProvider;
use crate::core::types::{MessageFingerprint, SymbolInstance};
use crate::language::segmentation::segment_message;
use crate::lexical::character::combined_character_similarity;
use crate::symbols::knowledge::DefaultSymbolKnowledge;

pub fn resolve_symbols_with_provider(
    text: &str,
    max_readings: usize,
    provider: &dyn SymbolKnowledgeProvider,
) -> Vec<SymbolInstance> {
    segment_message(text, 4096)
        .into_iter()
        .filter(|segment| matches!(segment.segment_type.as_str(), "emoji" | "symbol" | "number"))
        .map(|segment| {
            let mut readings = provider.readings(&segment.text, max_readings);
            if readings.is_empty() && segment.segment_type == "emoji" {
                readings.push(
                    crate::core::types::SymbolReading::new("unknown", Some("und"), 0.3, "unknown")
                        .with_source("builtin:symbol-fallback"),
                );
            }
            if readings.is_empty() {
                readings.push(
                    crate::core::types::SymbolReading::new(
                        &segment.text,
                        Some("und"),
                        1.0,
                        "identity",
                    )
                    .with_source("builtin:symbol-fallback"),
                );
            }
            SymbolInstance {
                raw: segment.text.clone(),
                start: segment.start,
                end: segment.end,
                kind: segment.segment_type,
                unicode_name: provider.unicode_name(&segment.text),
                concepts: provider.concepts(&segment.text),
                readings,
            }
        })
        .collect()
}

pub fn resolve_symbols(text: &str, max_readings: usize) -> Vec<SymbolInstance> {
    resolve_symbols_with_provider(text, max_readings, &DefaultSymbolKnowledge)
}

/// Compare symbol concepts/readings with the other message's lexical surface.
/// If both messages have no symbols, this channel reduces to normalized
/// identity; otherwise it measures the best supported cross-view match.
pub fn symbolic_similarity(a: &MessageFingerprint, b: &MessageFingerprint) -> f64 {
    if a.symbols.is_empty() && b.symbols.is_empty() {
        return if a.unicode_features.casefolded == b.unicode_features.casefolded {
            1.0
        } else {
            0.0
        };
    }
    let compact = |text: &str| {
        text.chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>()
    };
    let b_text = compact(b.normalized.as_deref().unwrap_or(&b.raw));
    let a_text = compact(a.normalized.as_deref().unwrap_or(&a.raw));
    // Unique reading texts per side, in first-occurrence order: string
    // similarity is pure in its inputs and `max` is order-independent,
    // so scoring each distinct pair once (32×32 occurrences collapse to
    // 15×15 here) yields the same maximum with ~4× fewer comparisons.
    // The probability channel below stays per-occurrence — the same
    // text can carry different probabilities per symbol.
    fn unique_readings(symbols: &[SymbolInstance]) -> Vec<&str> {
        let mut unique = Vec::new();
        for symbol in symbols {
            for reading in &symbol.readings {
                if !unique.contains(&reading.text.as_str()) {
                    unique.push(reading.text.as_str());
                }
            }
        }
        unique
    }
    let readings_a = unique_readings(&a.symbols);
    let readings_b = unique_readings(&b.symbols);
    let mut best: f64 = 0.0;
    for reading in &readings_a {
        best = best.max(combined_character_similarity(reading, &b_text));
    }
    for reading in &readings_b {
        best = best.max(combined_character_similarity(reading, &a_text));
    }
    for left in &readings_a {
        for right in &readings_b {
            best = best.max(combined_character_similarity(left, right));
        }
    }
    // Lowercases folded once per unique text; occurrences share them.
    let lower_a: std::collections::HashMap<&str, String> = readings_a
        .iter()
        .map(|text| (*text, text.to_lowercase()))
        .collect();
    let lower_b: std::collections::HashMap<&str, String> = readings_b
        .iter()
        .map(|text| (*text, text.to_lowercase()))
        .collect();
    for symbol in &a.symbols {
        for reading in &symbol.readings {
            if b.lexical_features
                .jaccard_ready
                .contains(&lower_a[reading.text.as_str()])
            {
                best = best.max(reading.probability);
            }
        }
    }
    for symbol in &b.symbols {
        for reading in &symbol.readings {
            if a.lexical_features
                .jaccard_ready
                .contains(&lower_b[reading.text.as_str()])
            {
                best = best.max(reading.probability);
            }
        }
    }
    // Direct text still contributes when a symbol is surrounded by ordinary
    // words, but it cannot erase uncertainty from the symbol channel.
    let direct = combined_character_similarity(&a_text, &b_text);
    best.max(direct * 0.55).clamp(0.0, 1.0)
}
