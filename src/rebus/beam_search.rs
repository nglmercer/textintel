use crate::core::providers::{AbbreviationProvider, SymbolKnowledgeProvider};
use crate::core::types::Transformation;
use crate::rebus::tokenizer::{rebus_tokens_with_spans, token_readings_with_abbreviation_provider};
use crate::symbols::knowledge::DefaultSymbolKnowledge;

#[derive(Debug, Clone)]
pub struct BeamNode {
    pub text: String,
    pub score: f64,
    pub transforms: Vec<Transformation>,
    pub language: Option<String>,
    /// Every non-`und` language observed along this path, in first-seen
    /// order. Mixed-language derivations keep all of them; scoring rewards
    /// coverage instead of collapsing the mix.
    pub languages: Vec<String>,
}

impl BeamNode {
    fn empty() -> Self {
        Self {
            text: String::new(),
            score: 1.0,
            transforms: Vec::new(),
            language: None,
            languages: Vec::new(),
        }
    }
}

/// `base + surface` without `format!` machinery: identical bytes, one
/// allocation.
fn concat_texts(base: &str, surface: &str) -> String {
    let mut text = String::with_capacity(base.len() + surface.len());
    text.push_str(base);
    text.push_str(surface);
    text
}

/// [`concat_texts`] with hypothesized word boundaries: identical bytes to
/// the `format!` variants it replaces.
fn concat_spaced(base: &str, surface: &str, left: bool, right: bool) -> String {
    let mut text =
        String::with_capacity(base.len() + surface.len() + usize::from(left) + usize::from(right));
    text.push_str(base);
    if left {
        text.push(' ');
    }
    text.push_str(surface);
    if right {
        text.push(' ');
    }
    text
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
    let defaults = crate::core::config::RebusWeights::default();
    beam_decode_with_abbreviation_provider(
        text,
        beam_width,
        max_candidates,
        max_symbol_readings,
        provider,
        None,
        languages,
        max_branches,
        defaults.language_switch_penalty,
        defaults.in_word_digit_discount,
    )
}

/// Full variant with an explicit abbreviation source (`None` selects the
/// embedded versioned abbreviation packs), a per-switch language discount,
/// and an in-word digit discount, both from
/// [`RebusWeights`](crate::core::config::RebusWeights).
#[allow(clippy::too_many_arguments)]
pub fn beam_decode_with_abbreviation_provider(
    text: &str,
    beam_width: usize,
    max_candidates: usize,
    max_symbol_readings: usize,
    provider: &dyn SymbolKnowledgeProvider,
    abbreviations: Option<&dyn AbbreviationProvider>,
    languages: Option<&[String]>,
    max_branches: usize,
    language_switch_penalty: f64,
    in_word_digit_discount: f64,
) -> Vec<BeamNode> {
    let tokens = rebus_tokens_with_spans(text);
    if tokens.is_empty() {
        return vec![BeamNode {
            text: String::new(),
            score: 0.0,
            transforms: Vec::new(),
            language: None,
            languages: Vec::new(),
        }];
    }
    let width = beam_width.max(1);
    let branch_limit = max_branches.max(width);
    let switch_discount = (1.0 - language_switch_penalty.clamp(0.0, 0.99)).max(0.01);
    let digit_discount = in_word_digit_discount.clamp(0.01, 1.0);
    // Token classes for the in-word digit rule: a numeric token with a
    // letter neighbor is leet context (`Fr4` → `Fra`), not a number name.
    let is_digit_token: Vec<bool> = tokens
        .iter()
        .map(|(token, _, _)| !token.is_empty() && token.chars().all(|ch| ch.is_numeric()))
        .collect();
    let is_letter_token: Vec<bool> = tokens
        .iter()
        .map(|(token, _, _)| {
            !token.is_empty()
                && token
                    .chars()
                    .all(crate::language::segmentation::is_word_character)
        })
        .collect();
    let mut beam = vec![BeamNode::empty()];
    for (position, (token, start, end)) in tokens.iter().enumerate() {
        let readings = token_readings_with_abbreviation_provider(
            token,
            max_symbol_readings.max(1),
            provider,
            abbreviations,
            languages,
        );
        let last = position + 1 >= tokens.len();
        let in_word_digit = is_digit_token[position]
            && ((position > 0 && is_letter_token[position - 1])
                || (position + 1 < tokens.len() && is_letter_token[position + 1]));
        // Lowercased once per token: the identity checks below run per
        // beam node and realloced these strings every time.
        let token_lower = token.to_lowercase();
        // Per-reading values hoisted out of the per-node loop: the
        // lowercase, the in-word digit gate, the substantive flag, and
        // the trailing-space check depend only on the reading (plus
        // position), never on the beam node — same values, computed once.
        let precomputed: Vec<(String, bool, bool, bool)> = readings
            .iter()
            .map(|(surface, _, _, transform_type)| {
                let surface_lower = surface.to_lowercase();
                let digit_gated = in_word_digit
                    && transform_type == "number_reading"
                    && surface.chars().count() > 1
                    && surface.chars().all(|ch| ch.is_alphabetic());
                let substantive = surface_lower != token_lower;
                let trails_space = surface.chars().next_back().is_some_and(char::is_whitespace);
                (surface_lower, digit_gated, substantive, trails_space)
            })
            .collect();
        // Next-token whitespace lookahead is node-independent too.
        let next_is_space = tokens
            .get(position + 1)
            .is_some_and(|(next_token, _, _)| next_token.chars().all(char::is_whitespace));
        let mut next = Vec::new();
        for node in &beam {
            for (
                (surface, probability, language, transform_type),
                (surface_lower, digit_gated, substantive, surface_trails_space),
            ) in readings.iter().zip(precomputed.iter())
            {
                // In-word digits discount multi-letter number names (`4` →
                // `cuatro`); single-letter leet readings (`4` → `a`) and
                // standalone digits are untouched.
                let contextual = if *digit_gated {
                    probability * digit_discount
                } else {
                    *probability
                };
                let mut transforms = node.transforms.clone();
                if transform_type != "identity" && *surface_lower != token_lower {
                    let mut step =
                        Transformation::new(token.clone(), surface.clone(), transform_type.clone())
                            .with_span(*start, *end, token.clone())
                            .with_confidence(contextual)
                            .with_provider("rebus");
                    if language != "und" {
                        step = step.with_language(language.clone());
                    }
                    transforms.push(step);
                }
                let reading_language = if language != "und" {
                    Some(language.clone())
                } else {
                    None
                };
                // Track the language set; a genuinely new language on a
                // non-empty path takes a small discount. The default (0.02)
                // keeps mixed-language inputs valid while preferring
                // monolingual derivations slightly.
                let mut path_languages = node.languages.clone();
                let mut switch = 1.0;
                if let Some(reading) = &reading_language
                    && !path_languages
                        .iter()
                        .any(|known| known.eq_ignore_ascii_case(reading))
                {
                    if !path_languages.is_empty() {
                        switch = switch_discount;
                    }
                    path_languages.push(reading.clone());
                }
                let language = reading_language.or_else(|| node.language.clone());
                let base_score = node.score * contextual.max(0.05) * switch;
                // Word-boundary variants exist only for substantive
                // (non-identity, surface-changing) readings; other nodes
                // move the transforms instead of cloning them a second
                // time. Same nodes, one fewer `Vec` clone on the common
                // identity path.
                let boundary_gate = transform_type != "identity" && *substantive;
                if boundary_gate {
                    next.push(BeamNode {
                        text: concat_texts(&node.text, surface),
                        score: base_score,
                        transforms: transforms.clone(),
                        language: language.clone(),
                        languages: path_languages.clone(),
                    });
                    // Word-boundary variants: symbol readings usually stand for
                    // whole words, so also hypothesize explicit boundaries. The
                    // small penalty keeps the compact form preferred unless the
                    // spaced words score better downstream. Bounded to substantive
                    // (non-identity, surface-changing) readings; beam truncation
                    // caps the rest. No boundary is hypothesized next to literal
                    // input whitespace — that would only double spaces.
                    let mut boundary = transforms;
                    boundary
                        .push(Transformation::new("", " ", "word_boundary").with_provider("rebus"));
                    let ends_with_space = node
                        .text
                        .chars()
                        .next_back()
                        .is_some_and(char::is_whitespace);
                    let left = !node.text.is_empty() && !ends_with_space;
                    let right = !last && !*surface_trails_space && !next_is_space;
                    if left {
                        next.push(BeamNode {
                            text: concat_spaced(&node.text, surface, true, false),
                            score: base_score * 0.97,
                            transforms: boundary.clone(),
                            language: language.clone(),
                            languages: path_languages.clone(),
                        });
                    }
                    if right {
                        next.push(BeamNode {
                            text: concat_spaced(&node.text, surface, false, true),
                            score: base_score * 0.97,
                            transforms: boundary.clone(),
                            language: language.clone(),
                            languages: path_languages.clone(),
                        });
                    }
                    if left && right {
                        next.push(BeamNode {
                            text: concat_spaced(&node.text, surface, true, true),
                            score: base_score * 0.94,
                            transforms: boundary,
                            language,
                            languages: path_languages,
                        });
                    }
                } else {
                    next.push(BeamNode {
                        text: concat_texts(&node.text, surface),
                        score: base_score,
                        transforms,
                        language,
                        languages: path_languages,
                    });
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
