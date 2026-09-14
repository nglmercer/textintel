use crate::core::providers::SymbolKnowledgeProvider;
use crate::rebus::tokenizer::{rebus_tokens, token_readings_with_provider_and_languages};
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
    beam_decode_with_provider_and_languages(
        text,
        beam_width,
        max_candidates,
        max_symbol_readings,
        provider,
        None,
        usize::MAX,
    )
}

pub fn beam_decode_with_provider_and_languages(
    text: &str,
    beam_width: usize,
    max_candidates: usize,
    max_symbol_readings: usize,
    provider: &dyn SymbolKnowledgeProvider,
    languages: Option<&[String]>,
    max_branches: usize,
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
    let branch_limit = max_branches.max(width);
    let mut beam = vec![BeamNode {
        text: String::new(),
        score: 1.0,
        transforms: Vec::new(),
        language: None,
    }];
    for (position, token) in tokens.iter().enumerate() {
        let readings = token_readings_with_provider_and_languages(
            token,
            max_symbol_readings.max(1),
            provider,
            languages,
        );
        let last = position + 1 >= tokens.len();
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
                let base_score = node.score * probability.max(0.05);
                next.push(BeamNode {
                    text: format!("{}{}", node.text, surface),
                    score: base_score,
                    transforms: transforms.clone(),
                    language: language.clone(),
                });
                // Word-boundary variants: symbol readings usually stand for
                // whole words, so also hypothesize explicit boundaries. The
                // small penalty keeps the compact form preferred unless the
                // spaced words score better downstream. Bounded to symbol
                // (non-identity) readings; beam truncation caps the rest.
                if transform_type != "identity" {
                    let mut boundary = transforms;
                    boundary.push((String::new(), " ".to_string(), "word_boundary".to_string()));
                    let left = !node.text.is_empty() && !node.text.ends_with(' ');
                    let right = !last && !surface.ends_with(' ');
                    if left {
                        next.push(BeamNode {
                            text: format!("{} {}", node.text, surface),
                            score: base_score * 0.97,
                            transforms: boundary.clone(),
                            language: language.clone(),
                        });
                    }
                    if right {
                        next.push(BeamNode {
                            text: format!("{}{} ", node.text, surface),
                            score: base_score * 0.97,
                            transforms: boundary.clone(),
                            language: language.clone(),
                        });
                    }
                    if left && right {
                        next.push(BeamNode {
                            text: format!("{} {} ", node.text, surface),
                            score: base_score * 0.94,
                            transforms: boundary,
                            language,
                        });
                    }
                }
            }
        }
        next.sort_by(|left, right| right.score.total_cmp(&left.score));
        next.truncate(width.min(branch_limit));
        beam = next;
    }
    beam.sort_by(|left, right| right.score.total_cmp(&left.score));
    beam.truncate(max_candidates.max(width).min(branch_limit));
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
