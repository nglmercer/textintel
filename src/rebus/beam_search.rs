use crate::core::providers::SymbolKnowledgeProvider;
use crate::rebus::tokenizer::{rebus_tokens, token_readings_with_provider};
use crate::symbols::knowledge::DefaultSymbolKnowledge;

#[derive(Debug, Clone)]
pub struct BeamNode {
    pub text: String,
    pub score: f64,
    pub transforms: Vec<(String, String, String)>,
    pub language: Option<String>,
}

pub fn beam_decode_with_provider(
    text: &str,
    beam_width: usize,
    max_candidates: usize,
    max_symbol_readings: usize,
    provider: &dyn SymbolKnowledgeProvider,
) -> Vec<BeamNode> {
    let tokens = rebus_tokens(text);
    if tokens.is_empty() {
        return vec![BeamNode {
            text: String::new(),
            score: 0.0,
            transforms: Vec::new(),
            language: None,
        }];
    }
    let width = beam_width.max(1);
    let mut beam = vec![BeamNode {
        text: String::new(),
        score: 1.0,
        transforms: Vec::new(),
        language: None,
    }];
    for token in tokens {
        let readings = token_readings_with_provider(&token, max_symbol_readings.max(1), provider);
        let mut next = Vec::new();
        for node in &beam {
            for (surface, probability, language, transform_type) in &readings {
                let mut transforms = node.transforms.clone();
                if transform_type != "identity" && surface.to_lowercase() != token.to_lowercase() {
                    transforms.push((token.clone(), surface.clone(), transform_type.clone()));
                }
                let language = if language != "und" {
                    Some(language.clone())
                } else {
                    node.language.clone()
                };
                next.push(BeamNode {
                    text: format!("{}{}", node.text, surface),
                    score: node.score * probability.max(0.05),
                    transforms,
                    language,
                });
            }
        }
        next.sort_by(|left, right| right.score.total_cmp(&left.score));
        next.truncate(width);
        beam = next;
    }
    beam.sort_by(|left, right| right.score.total_cmp(&left.score));
    beam.truncate(max_candidates.max(width));
    beam
}

pub fn beam_decode(
    text: &str,
    beam_width: usize,
    max_candidates: usize,
    max_symbol_readings: usize,
) -> Vec<BeamNode> {
    beam_decode_with_provider(
        text,
        beam_width,
        max_candidates,
        max_symbol_readings,
        &DefaultSymbolKnowledge,
    )
}
