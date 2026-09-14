use crate::core::config::EngineConfig;
use crate::core::providers::{G2PProvider, LexiconProvider, SymbolKnowledgeProvider};
use crate::core::types::{DecodedCandidate, Transformation};
use crate::normalization::confusables::skeleton;
use crate::normalization::leetspeak::apply_leet;
use crate::normalization::repetition::collapse_repetition;
use crate::normalization::unicode::casefold_text;
use crate::normalization::whitespace::normalize_whitespace;
use crate::phonetic::g2p::RuleBasedG2PProvider;
use crate::rebus::beam_search::beam_decode_with_provider_and_languages;
use crate::rebus::scorer::{score_candidate_with_evidence, RebusEvidence};
use crate::resources::DefaultLexiconProvider;
use crate::symbols::knowledge::DefaultSymbolKnowledge;

#[derive(Debug, Clone, Default)]
pub struct RebusDecoder {
    pub config: EngineConfig,
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
        self.decode_with_providers(
            text,
            languages,
            max_candidates,
            provider,
            &DefaultLexiconProvider,
        )
    }

    pub fn decode_with_providers(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
        symbol_provider: &dyn SymbolKnowledgeProvider,
        lexicon_provider: &dyn LexiconProvider,
    ) -> Vec<DecodedCandidate> {
        self.decode_with_all_providers(
            text,
            languages,
            max_candidates,
            symbol_provider,
            lexicon_provider,
            &RuleBasedG2PProvider,
        )
    }

    pub fn decode_with_all_providers(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
        symbol_provider: &dyn SymbolKnowledgeProvider,
        lexicon_provider: &dyn LexiconProvider,
        g2p_provider: &dyn G2PProvider,
    ) -> Vec<DecodedCandidate> {
        self.decode_with_semantic(
            text,
            languages,
            max_candidates,
            symbol_provider,
            lexicon_provider,
            g2p_provider,
            None,
        )
    }

    /// Full pipeline with optional whole-text semantic evidence. The
    /// callback maps `(surface, source)` to a similarity in `[0.0, 1.0]`
    /// and is invoked only for survivors of the score floor (at most 12),
    /// keeping beam search bounded.
    #[allow(clippy::too_many_arguments)]
    pub fn decode_with_semantic(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
        symbol_provider: &dyn SymbolKnowledgeProvider,
        lexicon_provider: &dyn LexiconProvider,
        g2p_provider: &dyn G2PProvider,
        semantic: Option<&crate::rebus::SemanticEvidence>,
    ) -> Vec<DecodedCandidate> {
        let limit = max_candidates.unwrap_or(self.config.max_candidates).max(1);
        let nodes = beam_decode_with_provider_and_languages(
            text,
            self.config.beam_width,
            (limit * 3).max(self.config.beam_width),
            self.config.max_symbol_readings,
            symbol_provider,
            languages,
            self.config.max_decoded_branches,
        );
        let language = languages.and_then(|values| values.first()).cloned();
        let compact = |value: String| {
            value
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>()
        };
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
            let evidence = RebusEvidence {
                candidate_language: node.language.clone(),
                semantic_similarity: None,
                transformation_types: node
                    .transforms
                    .iter()
                    .map(|(_, _, kind)| kind.clone())
                    .collect(),
            };
            let (score, lexical, phonetic, context) = score_candidate_with_evidence(
                &node.text,
                text,
                node.score,
                self.config.max_recursion,
                languages,
                lexicon_provider,
                g2p_provider,
                &evidence,
            );
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
                    .map(
                        |(source, replacement, transformation_type)| Transformation {
                            source,
                            replacement,
                            transformation_type,
                        },
                    )
                    .collect(),
                language: node.language.or_else(|| language.clone()),
                lexical_score: lexical,
                phonetic_score: phonetic,
                context_score: context,
                symbol_score: node.score.min(1.0),
                confidence_gap: 0.0,
                strong: false,
            };
            if candidates
                .get(&key)
                .map_or(true, |old| candidate.score > old.score)
            {
                candidates.insert(key, candidate);
            }
        }
        for view in extra_views {
            let value = compact(view);
            if value.is_empty() {
                continue;
            }
            let (score, lexical, phonetic, context) = score_candidate_with_evidence(
                &value,
                text,
                0.4,
                self.config.max_recursion,
                languages,
                lexicon_provider,
                g2p_provider,
                &RebusEvidence::default(),
            );
            let candidate = DecodedCandidate {
                text: value.clone(),
                score: score * 0.9,
                transformations: Vec::new(),
                language: language.clone(),
                lexical_score: lexical,
                phonetic_score: phonetic,
                context_score: context,
                symbol_score: 0.3,
                confidence_gap: 0.0,
                strong: false,
            };
            let key = casefold_text(&value);
            if candidates
                .get(&key)
                .map_or(true, |old| candidate.score > old.score)
            {
                candidates.insert(key, candidate);
            }
        }
        let mut ranked: Vec<_> = candidates
            .into_values()
            .filter(|candidate| candidate.score >= 0.08)
            .collect();
        ranked.sort_by(|left, right| right.score.total_cmp(&left.score));
        if let Some(similarity) = semantic {
            // Bounded semantic rescoring: at most the top 12 survivors.
            for candidate in ranked.iter_mut().take(12) {
                if let Some(value) = similarity(&candidate.text, text) {
                    candidate.score =
                        (0.85 * candidate.score + 0.15 * value.clamp(0.0, 1.0)).clamp(0.0, 1.0);
                }
            }
            ranked.sort_by(|left, right| right.score.total_cmp(&left.score));
        }
        // Abstention: no trustworthy reading (best below the floor), or
        // an ambiguous one (weak best with no separation from the runner
        // up). Returning nothing beats returning a junk ranking.
        let ambiguous = match ranked.as_slice() {
            [best, next, ..] => best.score < 0.4 && (best.score - next.score).max(0.0) < 0.02,
            [best] => best.score < 0.15,
            [] => true,
        };
        if ambiguous {
            return Vec::new();
        }
        let best_score = ranked
            .first()
            .map(|candidate| candidate.score)
            .unwrap_or(0.0);
        let next_scores = ranked
            .iter()
            .skip(1)
            .map(|candidate| candidate.score)
            .chain(std::iter::once(0.0))
            .collect::<Vec<_>>();
        for (index, candidate) in ranked.iter_mut().enumerate() {
            let next_score = next_scores[index];
            candidate.confidence_gap = if index == 0 {
                (best_score - next_score).max(0.0)
            } else {
                0.0
            };
            candidate.strong = index == 0
                && candidate.score >= 0.5
                && candidate.confidence_gap >= self.config.strong_confidence_gap;
        }
        ranked.truncate(limit);
        ranked
    }
}
