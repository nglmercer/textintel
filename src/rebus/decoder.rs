use crate::core::config::EngineConfig;
use crate::core::providers::SymbolKnowledgeProvider;
use crate::core::types::{DecodedCandidate, Transformation};
use crate::normalization::confusables::skeleton;
use crate::normalization::leetspeak::apply_leet;
use crate::normalization::repetition::collapse_repetition;
use crate::normalization::unicode::casefold_text;
use crate::normalization::whitespace::normalize_whitespace;
use crate::rebus::beam_search::beam_decode_with_provider;
use crate::rebus::scorer::score_candidate;
use crate::symbols::knowledge::DefaultSymbolKnowledge;

#[derive(Debug, Clone)]
pub struct RebusDecoder {
    pub config: EngineConfig,
}

impl Default for RebusDecoder {
    fn default() -> Self {
        Self { config: EngineConfig::default() }
    }
}

impl RebusDecoder {
    pub fn new(config: EngineConfig) -> Self {
        Self { config }
    }

    pub fn decode(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
    ) -> Vec<DecodedCandidate> {
        self.decode_with_provider(text, languages, max_candidates, &DefaultSymbolKnowledge)
    }

    pub fn decode_with_provider(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
        provider: &dyn SymbolKnowledgeProvider,
    ) -> Vec<DecodedCandidate> {
        let limit = max_candidates.unwrap_or(self.config.max_candidates).max(1);
        let nodes = beam_decode_with_provider(
            text,
            self.config.beam_width,
            (limit * 3).max(self.config.beam_width),
            self.config.max_symbol_readings,
            provider,
        );
        let language = languages.and_then(|values| values.first()).cloned();
        let compact = |value: String| value.chars().filter(|ch| !ch.is_whitespace()).collect::<String>();
        let extra_views = [
            normalize_whitespace(text),
            casefold_text(text),
            apply_leet(&casefold_text(text)),
            collapse_repetition(&apply_leet(&casefold_text(text)), 1),
            collapse_repetition(&casefold_text(text), 1),
            skeleton(text),
            collapse_repetition(&skeleton(&apply_leet(&casefold_text(text))), 1),
        ];
        let mut candidates = std::collections::BTreeMap::<String, DecodedCandidate>::new();
        for node in nodes {
            let (score, lexical, phonetic, context) = score_candidate(&node.text, node.score);
            if node.text.is_empty() {
                continue;
            }
            let key = casefold_text(&node.text);
            let candidate = DecodedCandidate {
                text: node.text,
                score,
                transformations: node
                    .transforms
                    .into_iter()
                    .map(|(source, replacement, transformation_type)| Transformation { source, replacement, transformation_type })
                    .collect(),
                language: node.language.or_else(|| language.clone()),
                lexical_score: lexical,
                phonetic_score: phonetic,
                context_score: context,
                symbol_score: node.score.min(1.0),
            };
            if candidates.get(&key).is_none_or(|old| candidate.score > old.score) {
                candidates.insert(key, candidate);
            }
        }
        for view in extra_views {
            let value = compact(view);
            if value.is_empty() {
                continue;
            }
            let (score, lexical, phonetic, context) = score_candidate(&value, 0.4);
            let candidate = DecodedCandidate {
                text: value.clone(),
                score: score * 0.9,
                transformations: Vec::new(),
                language: language.clone(),
                lexical_score: lexical,
                phonetic_score: phonetic,
                context_score: context,
                symbol_score: 0.3,
            };
            let key = casefold_text(&value);
            if candidates.get(&key).is_none_or(|old| candidate.score > old.score) {
                candidates.insert(key, candidate);
            }
        }
        let mut ranked: Vec<_> = candidates.into_values().filter(|candidate| candidate.score >= 0.08).collect();
        ranked.sort_by(|left, right| right.score.total_cmp(&left.score));
        ranked.truncate(limit);
        ranked
    }
}

