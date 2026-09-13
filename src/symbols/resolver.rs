use crate::core::providers::SymbolKnowledgeProvider;
use crate::core::types::{MessageFingerprint, SymbolInstance};
use crate::language::segmentation::segment_message;
use crate::lexical::character::combined_character_similarity;
use crate::symbols::knowledge::{concepts_for_token, DefaultSymbolKnowledge};

fn unicode_name(token: &str) -> Option<String> {
    let name = match token {
        "🏠" => "HOUSE BUILDING",
        "🔥" => "FIRE",
        "💰" => "MONEY BAG",
        "❤" | "❤️" => "HEAVY BLACK HEART",
        _ => return None,
    };
    Some(name.to_string())
}

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
                readings.push(crate::core::types::SymbolReading::new(
                    "unknown",
                    Some("und"),
                    0.3,
                    "unknown",
                ));
            }
            if readings.is_empty() {
                readings.push(crate::core::types::SymbolReading::new(
                    &segment.text,
                    Some("und"),
                    1.0,
                    "identity",
                ));
            }
            SymbolInstance {
                raw: segment.text.clone(),
                start: segment.start,
                end: segment.end,
                kind: segment.segment_type,
                unicode_name: unicode_name(&segment.text),
                concepts: concepts_for_token(&segment.text),
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
    let mut best: f64 = 0.0;
    for symbol in &a.symbols {
        for reading in &symbol.readings {
            best = best.max(combined_character_similarity(&reading.text, &b_text));
            if b.lexical_features
                .jaccard_ready
                .contains(&reading.text.to_lowercase())
            {
                best = best.max(reading.probability);
            }
        }
    }
    for symbol in &b.symbols {
        for reading in &symbol.readings {
            best = best.max(combined_character_similarity(&reading.text, &a_text));
            if a.lexical_features
                .jaccard_ready
                .contains(&reading.text.to_lowercase())
            {
                best = best.max(reading.probability);
            }
        }
    }
    for left in &a.symbols {
        for right in &b.symbols {
            for lread in &left.readings {
                for rread in &right.readings {
                    best = best.max(combined_character_similarity(&lread.text, &rread.text));
                }
            }
        }
    }
    // Direct text still contributes when a symbol is surrounded by ordinary
    // words, but it cannot erase uncertainty from the symbol channel.
    let direct = combined_character_similarity(&a_text, &b_text);
    best.max(direct * 0.55).clamp(0.0, 1.0)
}
