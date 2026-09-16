use crate::core::providers::{G2PProvider, LexiconProvider};
use crate::normalization::repetition::collapse_repetition;
use crate::resources::DefaultLexiconProvider;

fn known(text: &str, languages: Option<&[String]>, provider: &dyn LexiconProvider) -> bool {
    provider.contains(text, languages)
}

/// Scripts that never delimit words with spaces (Han ideographs and kana).
/// Hangul is deliberately excluded: Korean writes spaces between words, so
/// whitespace segmentation stays meaningful evidence there.
fn is_spaceless_cjk(character: char) -> bool {
    matches!(
        character,
        '\u{3400}'..='\u{4dbf}'
            | '\u{4e00}'..='\u{9fff}'
            | '\u{3040}'..='\u{309f}'
            | '\u{30a0}'..='\u{30ff}'
            | '\u{ff66}'..='\u{ff9f}'
    )
}

fn can_split_known(
    text: &str,
    depth: usize,
    max_depth: usize,
    languages: Option<&[String]>,
    provider: &dyn LexiconProvider,
) -> bool {
    if depth > max_depth || text.is_empty() {
        return false;
    }
    if known(text, languages, provider) {
        return true;
    }
    (2..text.len().saturating_sub(1)).any(|index| {
        text.is_char_boundary(index)
            && known(&text[..index], languages, provider)
            && can_split_known(&text[index..], depth + 1, max_depth, languages, provider)
    })
}

pub fn lexical_plausibility_with_limit(text: &str, max_recursion: usize) -> f64 {
    lexical_plausibility_with_provider(text, max_recursion, None, &DefaultLexiconProvider)
}

pub fn lexical_plausibility_with_provider(
    text: &str,
    max_recursion: usize,
    languages: Option<&[String]>,
    provider: &dyn LexiconProvider,
) -> f64 {
    let folded = collapse_repetition(&text.to_lowercase(), 1);
    let compact: String = folded.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.is_empty() {
        return 0.0;
    }
    if known(&compact, languages, provider)
        || can_split_known(&compact, 0, max_recursion, languages, provider)
    {
        return 1.0;
    }
    let parts = folded.split_whitespace().collect::<Vec<_>>();
    let hits = parts
        .iter()
        .filter(|part| known(part, languages, provider))
        .count();
    // Whitespace segmentation is not evidence for spaceless scripts: a
    // spaced CJK candidate (`我 爱 你`) is orthographically wrong, so per-part
    // lexicon hits must not let it outscore the unspaced form (`我爱你`).
    // Skipping the parts branch restores the tie that word-boundary costs
    // (spaces cost extra transformations) break toward the correct form.
    let spaceless = compact.chars().all(is_spaceless_cjk);
    if !parts.is_empty() && hits > 0 && !spaceless {
        return 0.4 + 0.6 * hits as f64 / parts.len() as f64;
    }
    if provider.starts_with(&compact, languages) {
        return 0.55;
    }
    0.15
}

pub fn score_candidate(surface: &str, prior: f64) -> (f64, f64, f64, f64) {
    score_candidate_with_limit(surface, prior, 4)
}

pub fn lexical_plausibility(text: &str) -> f64 {
    lexical_plausibility_with_limit(text, 4)
}

pub fn score_candidate_with_limit(
    surface: &str,
    prior: f64,
    max_recursion: usize,
) -> (f64, f64, f64, f64) {
    score_candidate_with_provider(surface, prior, max_recursion, None, &DefaultLexiconProvider)
}

pub fn score_candidate_with_provider(
    surface: &str,
    prior: f64,
    max_recursion: usize,
    languages: Option<&[String]>,
    provider: &dyn LexiconProvider,
) -> (f64, f64, f64, f64) {
    score_candidate_with_g2p(
        surface,
        surface,
        prior,
        max_recursion,
        languages,
        provider,
        &crate::phonetic::g2p::RuleBasedG2PProvider,
    )
}

pub fn score_candidate_with_g2p(
    surface: &str,
    source: &str,
    prior: f64,
    max_recursion: usize,
    languages: Option<&[String]>,
    provider: &dyn LexiconProvider,
    g2p_provider: &dyn G2PProvider,
) -> (f64, f64, f64, f64) {
    score_candidate_with_evidence(
        surface,
        source,
        prior,
        max_recursion,
        languages,
        provider,
        g2p_provider,
        &RebusEvidence::default(),
    )
}

/// Optional evidence beyond the surface string. Every field has a neutral
/// default so legacy callers are unaffected.
#[derive(Debug, Clone, Default)]
pub struct RebusEvidence {
    /// Primary language reported by the beam decoder for this candidate.
    pub candidate_language: Option<String>,
    /// Every language observed along the beam path (enables mixed-language
    /// scoring; empty falls back to `candidate_language`).
    pub candidate_languages: Vec<String>,
    /// Whole-text semantic similarity between surface and source, when an
    /// embedding backend is available.
    pub semantic_similarity: Option<f64>,
    /// Transformation labels applied along the beam path (e.g. "leetspeak").
    pub transformation_types: Vec<String>,
}

impl RebusEvidence {
    /// Penalty capped by the weights: cheap normalizations cost less than
    /// symbol readings, so far-fetched derivations cannot outrank plain ones.
    pub fn transformation_penalty(&self) -> f64 {
        self.transformation_penalty_with_weights(&crate::core::config::RebusWeights::default())
    }

    pub fn transformation_penalty_with_weights(
        &self,
        weights: &crate::core::config::RebusWeights,
    ) -> f64 {
        let mut penalty = 0.0f64;
        for kind in &self.transformation_types {
            let cost = if kind == "identity" {
                weights.cost_identity
            } else if kind.contains("boundary") {
                weights.cost_boundary
            } else if kind.contains("leet") || kind.contains("case") || kind.contains("normal") {
                weights.cost_leet
            } else if kind.contains("symbol") || kind.contains("emoji") || kind.contains("read") {
                weights.cost_symbol
            } else {
                weights.cost_other
            };
            penalty += cost;
        }
        penalty.min(weights.transformation_penalty_cap)
    }

    /// Compatibility in `[0.0, 1.0]` between the candidate language and the
    /// requested languages. Unknown on either side is neutral, never evidence.
    /// Languages observed along the beam path behind this candidate.
    /// Empty means "unknown" and stays neutral; a non-empty set that is
    /// covered by the requested languages scores 1.0, partial overlap 0.8,
    /// and disjoint sets 0.4. Mixed-language derivations are valid by
    /// design: covering *more* requested languages never scores below a
    /// single-language mismatch.
    pub fn language_score(&self, languages: Option<&[String]>) -> f64 {
        let requested = languages.filter(|values| !values.is_empty());
        let mut observed: Vec<&str> = self
            .candidate_languages
            .iter()
            .map(String::as_str)
            .collect();
        if observed.is_empty() {
            if let Some(single) = self.candidate_language.as_deref() {
                observed.push(single);
            }
        }
        observed.retain(|language| {
            !language.eq_ignore_ascii_case("und") && !language.eq_ignore_ascii_case("unknown")
        });
        match (requested, observed.is_empty()) {
            (Some(requested), false) => {
                let covered = observed
                    .iter()
                    .filter(|candidate| {
                        requested
                            .iter()
                            .any(|language| language.eq_ignore_ascii_case(candidate))
                    })
                    .count();
                if covered == observed.len() {
                    1.0
                } else if covered > 0 {
                    0.8
                } else {
                    0.4
                }
            }
            _ => 0.7,
        }
    }
}

/// Evidence-weighted candidate scoring. The phonetic channel uses real G2P
/// similarity only: when no phonemes are available it is neutral (0.5),
/// never character similarity masquerading as phonetic evidence.
#[allow(clippy::too_many_arguments)]
pub fn score_candidate_with_evidence(
    surface: &str,
    source: &str,
    prior: f64,
    max_recursion: usize,
    languages: Option<&[String]>,
    provider: &dyn LexiconProvider,
    g2p_provider: &dyn G2PProvider,
    evidence: &RebusEvidence,
) -> (f64, f64, f64, f64) {
    score_candidate_with_evidence_and_weights(
        surface,
        source,
        prior,
        max_recursion,
        languages,
        provider,
        g2p_provider,
        evidence,
        &crate::core::config::RebusWeights::default(),
    )
}

/// [`score_candidate_with_evidence`] with an explicit weight vector from
/// [`EngineConfig::rebus_weights`](crate::core::config::EngineConfig).
/// Channel weights are renormalized, so tuned or trained vectors only need
/// correct relative magnitudes.
#[allow(clippy::too_many_arguments)]
pub fn score_candidate_with_evidence_and_weights(
    surface: &str,
    source: &str,
    prior: f64,
    max_recursion: usize,
    languages: Option<&[String]>,
    provider: &dyn LexiconProvider,
    g2p_provider: &dyn G2PProvider,
    evidence: &RebusEvidence,
    weights: &crate::core::config::RebusWeights,
) -> (f64, f64, f64, f64) {
    let lexical = lexical_plausibility_with_provider(surface, max_recursion, languages, provider);
    let frequency = provider.frequency(surface, languages).unwrap_or(0.0);
    let frequency_bonus =
        (frequency.ln_1p() / weights.frequency_scale).clamp(0.0, weights.frequency_cap);
    let lexical = (lexical + frequency_bonus).min(1.0);
    let phonetic = g2p_similarity(surface, source, languages, g2p_provider).unwrap_or(0.5);
    let context = context_score(surface, lexical, provider, languages);
    let language = evidence.language_score(languages);
    let symbol = prior.clamp(0.0, 1.0);
    let base = ((weights.lexical * lexical
        + weights.phonetic * phonetic
        + weights.context * context
        + weights.symbol * symbol
        + weights.language * language)
        / weights.channel_sum())
    .clamp(0.0, 1.0);
    let penalized = base * (1.0 - evidence.transformation_penalty_with_weights(weights));
    let total = match evidence.semantic_similarity {
        Some(similarity) => {
            (1.0 - weights.semantic) * penalized + weights.semantic * similarity.clamp(0.0, 1.0)
        }
        None => penalized,
    }
    .clamp(0.0, 1.0);
    (total, lexical, phonetic, context)
}

fn context_score(
    surface: &str,
    lexical: f64,
    provider: &dyn LexiconProvider,
    languages: Option<&[String]>,
) -> f64 {
    let compact = surface.chars().filter(|ch| !ch.is_whitespace()).count();
    let boundary = if compact == 0 {
        0.0
    } else if provider.contains(surface, languages) {
        1.0
    } else if provider.starts_with(surface, languages) {
        0.65
    } else {
        0.35
    };
    (0.55 * lexical + 0.45 * boundary).clamp(0.0, 1.0)
}

fn g2p_similarity(
    surface: &str,
    source: &str,
    languages: Option<&[String]>,
    provider: &dyn G2PProvider,
) -> Option<f64> {
    let language = languages
        .and_then(|values| values.iter().find(|value| value.as_str() != "unknown"))
        .map(String::as_str)
        .unwrap_or("und");
    let candidate = provider.phonemize(surface, language).ok()?;
    let source = provider.phonemize(source, language).ok()?;
    if candidate.phonemes.is_empty() || source.phonemes.is_empty() {
        return None;
    }
    Some(crate::phonetic::similarity::phonetic_similarity(
        &candidate, &source,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phonetic::g2p::NullG2PProvider;

    #[test]
    fn empty_g2p_is_neutral_not_character_similarity() {
        // NullG2P yields no phonemes: the phonetic channel must be exactly
        // neutral instead of character similarity in disguise.
        let (total, _, phonetic, _) = score_candidate_with_evidence(
            "fracasado",
            "fracasado",
            0.5,
            4,
            None,
            &DefaultLexiconProvider,
            &NullG2PProvider,
            &RebusEvidence::default(),
        );
        assert_eq!(phonetic, 0.5);
        assert!(total > 0.0 && total <= 1.0);
    }

    #[test]
    fn transformation_penalty_grows_with_derivation_cost() {
        let plain = RebusEvidence::default();
        assert_eq!(plain.transformation_penalty(), 0.0);
        let leet = RebusEvidence {
            transformation_types: vec!["leetspeak".to_string()],
            ..RebusEvidence::default()
        };
        let symbols = RebusEvidence {
            transformation_types: vec!["symbol".to_string(), "emoji".to_string()],
            ..RebusEvidence::default()
        };
        assert!(leet.transformation_penalty() > 0.0);
        assert!(symbols.transformation_penalty() > leet.transformation_penalty());
        assert!(symbols.transformation_penalty() <= 0.35);
    }

    #[test]
    fn language_score_rewards_matches_and_ignores_unknowns() {
        let requested = vec!["es".to_string()];
        let matched = RebusEvidence {
            candidate_language: Some("es".to_string()),
            ..RebusEvidence::default()
        };
        assert_eq!(matched.language_score(Some(&requested)), 1.0);
        let mismatched = RebusEvidence {
            candidate_language: Some("fr".to_string()),
            ..RebusEvidence::default()
        };
        assert!(mismatched.language_score(Some(&requested)) < 0.7);
        assert_eq!(
            RebusEvidence::default().language_score(Some(&requested)),
            0.7
        );
        assert_eq!(matched.language_score(None), 0.7);
    }

    #[test]
    fn semantic_evidence_blends_into_total() {
        let base = RebusEvidence::default();
        let (plain, _, _, _) = score_candidate_with_evidence(
            "fracasado",
            "Fra🏠do",
            0.5,
            4,
            None,
            &DefaultLexiconProvider,
            &crate::phonetic::g2p::RuleBasedG2PProvider,
            &base,
        );
        let semantic = RebusEvidence {
            semantic_similarity: Some(1.0),
            ..RebusEvidence::default()
        };
        let (boosted, _, _, _) = score_candidate_with_evidence(
            "fracasado",
            "Fra🏠do",
            0.5,
            4,
            None,
            &DefaultLexiconProvider,
            &crate::phonetic::g2p::RuleBasedG2PProvider,
            &semantic,
        );
        assert!(boosted >= plain);
    }
}
