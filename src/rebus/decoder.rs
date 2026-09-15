use crate::core::config::EngineConfig;
use crate::core::providers::{
    AbbreviationProvider, G2PProvider, LexiconProvider, SymbolKnowledgeProvider,
};
use crate::core::types::{DecodedCandidate, Transformation};
use crate::normalization::confusables::skeleton;
use crate::normalization::leetspeak::apply_leet;
use crate::normalization::repetition::collapse_repetition;
use crate::normalization::unicode::casefold_text;
use crate::normalization::whitespace::normalize_whitespace;
use crate::phonetic::g2p::RuleBasedG2PProvider;
use crate::rebus::beam_search::beam_decode_with_abbreviation_provider;
use crate::rebus::scorer::{score_candidate_with_evidence_and_weights, RebusEvidence};
use crate::resources::{embedded_resources, DefaultLexiconProvider};
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
        self.decode_with_abbreviations(
            text,
            languages,
            max_candidates,
            symbol_provider,
            lexicon_provider,
            g2p_provider,
            semantic,
            None,
        )
    }

    /// Full pipeline with an explicit abbreviation source (`None` selects the
    /// embedded versioned abbreviation packs). Applications with custom slang
    /// packs pass their `AbbreviationProvider` here.
    #[allow(clippy::too_many_arguments)]
    pub fn decode_with_abbreviations(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
        symbol_provider: &dyn SymbolKnowledgeProvider,
        lexicon_provider: &dyn LexiconProvider,
        g2p_provider: &dyn G2PProvider,
        semantic: Option<&crate::rebus::SemanticEvidence>,
        abbreviations: Option<&dyn AbbreviationProvider>,
    ) -> Vec<DecodedCandidate> {
        let limit = max_candidates.unwrap_or(self.config.max_candidates).max(1);
        let weights = &self.config.rebus_weights;
        let nodes = beam_decode_with_abbreviation_provider(
            text,
            self.config.beam_width,
            (limit * 3).max(self.config.beam_width),
            self.config.max_symbol_readings,
            symbol_provider,
            abbreviations,
            languages,
            self.config.max_decoded_branches,
            weights.language_switch_penalty,
            weights.in_word_digit_discount,
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
                candidate_languages: node.languages.clone(),
                semantic_similarity: None,
                transformation_types: node
                    .transforms
                    .iter()
                    .map(|step| step.transformation_type.clone())
                    .collect(),
            };
            let (score, lexical, phonetic, context) = score_candidate_with_evidence_and_weights(
                &node.text,
                text,
                node.score,
                self.config.max_recursion,
                languages,
                lexicon_provider,
                g2p_provider,
                &evidence,
                weights,
            );
            if node.text.is_empty() {
                continue;
            }
            let key = casefold_text(&node.text);
            let candidate = DecodedCandidate {
                text: node.text,
                score,
                transformations: node.transforms,
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
        // Word-level abbreviation expansion. Segmentation splits digit/letter
        // boundaries (`gr8` → `gr` + `8`), so multi-class slang tokens never
        // consult the abbreviation packs token-by-token. Each whitespace word
        // gets one provider lookup; hits are scored like any other candidate.
        // Bounded (one lookup per word, provider-capped readings) and fully
        // resource-driven: removing a pack removes the expansion.
        {
            let mut search_from = 0usize;
            for word in text.split_whitespace().take(self.config.max_segments) {
                let readings = match abbreviations {
                    Some(provider) => provider.abbreviation_readings(word, languages, 4),
                    None => embedded_resources().abbreviation_readings(word, languages, 4),
                };
                let span = text[search_from..].find(word).map(|relative| {
                    let start = search_from + relative;
                    search_from = start + word.len();
                    (start, search_from)
                });
                for reading in readings {
                    if reading.text.eq_ignore_ascii_case(word) {
                        continue;
                    }
                    let evidence = RebusEvidence {
                        candidate_language: reading.language.clone(),
                        candidate_languages: reading.language.clone().into_iter().collect(),
                        semantic_similarity: None,
                        transformation_types: vec![reading.reading_type.clone()],
                    };
                    let (score, lexical, phonetic, context) =
                        score_candidate_with_evidence_and_weights(
                            &reading.text,
                            text,
                            reading.probability,
                            self.config.max_recursion,
                            languages,
                            lexicon_provider,
                            g2p_provider,
                            &evidence,
                            weights,
                        );
                    let mut step = Transformation::new(
                        word,
                        reading.text.clone(),
                        reading.reading_type.clone(),
                    )
                    .with_provider("rebus")
                    .with_confidence(reading.probability);
                    if let Some((start, end)) = span {
                        step = step.with_span(start, end, word);
                    }
                    if let Some(language) = reading.language.clone() {
                        if language != "und" {
                            step = step.with_language(language);
                        }
                    }
                    let candidate = DecodedCandidate {
                        text: reading.text.clone(),
                        score,
                        transformations: vec![step],
                        language: reading.language.clone().or_else(|| language.clone()),
                        lexical_score: lexical,
                        phonetic_score: phonetic,
                        context_score: context,
                        symbol_score: reading.probability.min(1.0),
                        confidence_gap: 0.0,
                        strong: false,
                    };
                    let key = casefold_text(&reading.text);
                    if candidates
                        .get(&key)
                        .map_or(true, |old| candidate.score > old.score)
                    {
                        candidates.insert(key, candidate);
                    }
                }
            }
        }
        // Normalization-ladder views: each ladder rung contributes its compact
        // form plus, when the input carries spacing, the spaced form. The
        // spaced form preserves faithful input structure (`c0mpr4 ah0r4` →
        // `compra ahora`); both are discounted equally and lose to any
        // higher-scoring beam derivation.
        let mut ladder = Vec::with_capacity(extra_views.len() * 2);
        for view in &extra_views {
            let spaced = normalize_whitespace(view);
            let flat = compact(view.clone());
            if !flat.is_empty() {
                ladder.push(flat);
            }
            if !spaced.is_empty() && spaced != compact(spaced.clone()) {
                ladder.push(spaced);
            }
        }
        for value in ladder {
            let (score, lexical, phonetic, context) = score_candidate_with_evidence_and_weights(
                &value,
                text,
                0.4,
                self.config.max_recursion,
                languages,
                lexicon_provider,
                g2p_provider,
                &RebusEvidence::default(),
                weights,
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
            // Bounded semantic rescoring: at most the top 12 survivors, with
            // the configured blend (0.0 disables without removing survivors).
            let blend = weights.semantic.clamp(0.0, 1.0);
            for candidate in ranked.iter_mut().take(12) {
                if let Some(value) = similarity(&candidate.text, text) {
                    candidate.score = ((1.0 - blend) * candidate.score
                        + blend * value.clamp(0.0, 1.0))
                    .clamp(0.0, 1.0);
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
