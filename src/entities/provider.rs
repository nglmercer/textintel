//! Rule-based entity provider: scan orchestration, bounds, and the
//! independent agreement/conflict evidence functions.

use std::sync::Arc;

use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::providers::{EntityProvider, LexiconProvider};
use crate::core::types::EntityMention;

use super::currency::scan_currency;
use super::datetime::scan_dates_times;
use super::email::scan_emails;
use super::mention::scan_mentions;
use super::normalize::casefold;
use super::numeric::scan_numbers;
use super::url::scan_urls;
use super::{DEFAULT_MAX_ENTITIES, DEFAULT_MAX_ENTITY_SPAN};

/// Deterministic rule-based entity extractor. Bounds are enforced on every
/// call: spans longer than `max_span_chars` are dropped and output is
/// truncated to `max_entities` in offset order.
///
/// A lone capitalized word is name-like only when it is NOT a common word:
/// with a lexicon attached ([`Self::with_lexicon`], the engine default),
/// single title-case words and acronyms found in the lexicon (any language)
/// are sentence capitalization, not names, and are skipped. Multi-word
/// spans keep their heuristic reading.
#[derive(Clone)]
pub struct RuleBasedEntityProvider {
    max_entities: usize,
    max_span_chars: usize,
    lexicon: Option<Arc<dyn LexiconProvider>>,
}

impl std::fmt::Debug for RuleBasedEntityProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuleBasedEntityProvider")
            .field("max_entities", &self.max_entities)
            .field("max_span_chars", &self.max_span_chars)
            .field("lexicon_attached", &self.lexicon.is_some())
            .finish()
    }
}

impl Default for RuleBasedEntityProvider {
    fn default() -> Self {
        Self {
            max_entities: DEFAULT_MAX_ENTITIES,
            max_span_chars: DEFAULT_MAX_ENTITY_SPAN,
            lexicon: None,
        }
    }
}

impl RuleBasedEntityProvider {
    pub fn new(max_entities: usize, max_span_chars: usize) -> Self {
        Self {
            max_entities: max_entities.max(1),
            max_span_chars: max_span_chars.max(1),
            lexicon: None,
        }
    }

    /// Attach a lexicon so lone capitalized common words are not mistaken
    /// for names (see the type documentation).
    pub fn with_lexicon(mut self, lexicon: Arc<dyn LexiconProvider>) -> Self {
        self.lexicon = Some(lexicon);
        self
    }

    pub fn max_entities(&self) -> usize {
        self.max_entities
    }

    pub fn max_span_chars(&self) -> usize {
        self.max_span_chars
    }

    /// True when `word` is a common word (hence sentence capitalization
    /// rather than a name). Without a lexicon nothing is common. The
    /// diacritic-stripped form counts too (`Dónde` matches `donde`), since
    /// resource word lists are not always accented.
    pub(crate) fn is_common_word(&self, word: &str) -> bool {
        let Some(lexicon) = &self.lexicon else {
            return false;
        };
        let folded = casefold(word);
        let stripped = crate::normalization::unicode::strip_diacritics(&folded);
        [&folded, &stripped]
            .iter()
            .any(|form| lexicon.contains(form, None) || lexicon.is_stop_word(form, None))
    }

    fn bounded(&self, mut mentions: Vec<EntityMention>) -> Vec<EntityMention> {
        mentions.retain(|mention| {
            mention.value.chars().count() <= self.max_span_chars
                && mention.value.chars().count() > 0
        });
        mentions.sort_by(|left, right| {
            left.start
                .cmp(&right.start)
                .then_with(|| left.end.cmp(&right.end))
                .then_with(|| left.entity_type.cmp(&right.entity_type))
        });
        mentions.truncate(self.max_entities);
        mentions
    }
}

impl EntityProvider for RuleBasedEntityProvider {
    fn extract(&self, text: &str, language: Option<&str>) -> Vec<EntityMention> {
        if text.is_empty() {
            return Vec::new();
        }
        let mut claimed = vec![false; text.len()];
        let mut mentions = Vec::new();
        // Priority order: structured identifiers first so numbers inside
        // URLs, emails, currency, and dates are never double-counted.
        scan_urls(text, &mut claimed, &mut mentions, language);
        scan_emails(text, &mut claimed, &mut mentions, language);
        scan_mentions(text, &mut claimed, &mut mentions, language);
        scan_currency(text, &mut claimed, &mut mentions, language);
        scan_dates_times(text, &mut claimed, &mut mentions, language);
        scan_numbers(text, &mut claimed, &mut mentions, language);
        self.scan_names(text, &mut claimed, &mut mentions, language);
        self.bounded(mentions)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("rule_based_entities")
            .with_quality(CapabilityLevel::Basic)
            .with_fallback("heuristic spans; prefer a trained NER provider for production")
    }
}

/// Entity agreement in `[0.0, 1.0]`: the fraction of the larger side's
/// mentions matched by the other side. Exact `(type, value)` matches count
/// `1.0`; compatible matches count `0.5`: for person/organization names,
/// same-type containment or near-identical values, and for any type an
/// entity value appearing as an ordinary token span in the other raw text.
/// Identifiers (URLs, emails, mentions, numbers, currency, dates, times)
/// never fuzz-match. Empty on either side reads `0.0`.
pub fn entity_agreement(
    left: &[EntityMention],
    left_raw: &str,
    right: &[EntityMention],
    right_raw: &str,
) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let left_fold = casefold(left_raw);
    let right_fold = casefold(right_raw);
    let mut matched = 0.0;
    let mut used = vec![false; right.len()];
    for mention in left {
        let mut best: Option<(usize, f64)> = None;
        for (position, candidate) in right.iter().enumerate() {
            if used[position] || candidate.entity_type != mention.entity_type {
                continue;
            }
            if candidate.value == mention.value {
                best = Some((position, 1.0));
                break;
            }
            if best.is_none()
                && compatible_values(&mention.entity_type, &mention.value, &candidate.value)
            {
                best = Some((position, 0.5));
            }
        }
        if let Some((position, weight)) = best {
            used[position] = true;
            matched += weight;
            continue;
        }
        // Entity-vs-ordinary-token: the value surfaces in the other text
        // without being extracted there (casing or context defeated the
        // heuristic). Counts as weak compatibility, never conflict.
        if mention.value.chars().count() >= 3 && right_fold.contains(&mention.value) {
            matched += 0.5;
        }
    }
    // Symmetric token-side check for right-only values missed above.
    for (position, mention) in right.iter().enumerate() {
        if used[position] || mention.value.chars().count() < 3 {
            continue;
        }
        if left_fold.contains(&mention.value)
            && !left
                .iter()
                .any(|candidate| candidate.entity_type == mention.entity_type)
        {
            matched += 0.25;
        }
    }
    (matched / left.len().max(right.len()) as f64).clamp(0.0, 1.0)
}

/// Entity conflict in `[0.0, 1.0]`: the fraction of shared entity types whose
/// value sets are disjoint with no compatible pair. Types appearing on one
/// side only are not conflicts, and empty on either side reads `0.0`, so
/// missing evidence never penalizes.
pub fn entity_conflict(left: &[EntityMention], right: &[EntityMention]) -> f64 {
    use std::collections::{BTreeMap, BTreeSet};
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let mut left_by_type: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut right_by_type: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for mention in left {
        left_by_type
            .entry(mention.entity_type.as_str())
            .or_default()
            .push(mention.value.as_str());
    }
    for mention in right {
        right_by_type
            .entry(mention.entity_type.as_str())
            .or_default()
            .push(mention.value.as_str());
    }
    let shared: BTreeSet<&str> = left_by_type
        .keys()
        .collect::<BTreeSet<_>>()
        .intersection(&right_by_type.keys().collect::<BTreeSet<_>>())
        .copied()
        .copied()
        .collect();
    if shared.is_empty() {
        return 0.0;
    }
    let mut disjoint = 0usize;
    for entity_type in &shared {
        let left_values = &left_by_type[entity_type];
        let right_values = &right_by_type[entity_type];
        let mut linked = false;
        for left_value in left_values {
            for right_value in right_values {
                if left_value == right_value
                    || compatible_values(entity_type, left_value, right_value)
                {
                    linked = true;
                    break;
                }
            }
            if linked {
                break;
            }
        }
        if !linked {
            disjoint += 1;
        }
    }
    (disjoint as f64 / shared.len() as f64).clamp(0.0, 1.0)
}

fn compatible_values(entity_type: &str, left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    // Identifiers match exactly or not at all: a different URL path, email,
    // number, amount, or timestamp is a different referent even when the
    // strings look alike (`…/north` vs `…/south`). Only person and
    // organization names tolerate containment and near-matches (typos,
    // shortenings like `Smith` for `John Smith`).
    if !matches!(entity_type, "person" | "organization") {
        return false;
    }
    let (shorter, longer) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };
    if shorter.chars().count() >= 3 && longer.contains(shorter) {
        return true;
    }
    crate::lexical::character::combined_character_similarity(left, right) > 0.85
}

/// Type-level entity evidence lines (counts and type names only, never
/// mention values, so evidence stays safe to log).
pub fn entity_evidence_lines(
    left: &[EntityMention],
    left_raw: &str,
    right: &[EntityMention],
    right_raw: &str,
) -> Vec<String> {
    let agreement = entity_agreement(left, left_raw, right, right_raw);
    let conflict = entity_conflict(left, right);
    vec![
        format!("entity_agreement={agreement:.3}"),
        format!("entity_conflict={conflict:.3}"),
        format!("entity_count_a={}", left.len()),
        format!("entity_count_b={}", right.len()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(text: &str) -> Vec<EntityMention> {
        RuleBasedEntityProvider::default().extract(text, Some("en"))
    }

    #[test]
    fn structured_identifiers_win_over_numbers() {
        let mentions =
            extract("contact ada@example.com or visit https://example.com/42 by 2026-01-30");
        let types: Vec<&str> = mentions
            .iter()
            .map(|mention| mention.entity_type.as_str())
            .collect();
        assert!(types.contains(&"email"));
        assert!(types.contains(&"url"));
        assert!(types.contains(&"date"));
        // The `42` inside the URL and the date parts are claimed, not numbers.
        assert!(!mentions
            .iter()
            .any(|mention| mention.entity_type == "number" && mention.value == "42"));
        for mention in &mentions {
            assert!(mention.start < mention.end);
            assert_eq!(mention.provider, "rule_based_entities");
            assert_eq!(mention.language.as_deref(), Some("en"));
        }
    }

    #[test]
    fn currency_and_mentions_normalize() {
        let mentions = extract("send $1,200 to @Ada_Lovelace before 14:30");
        assert!(mentions
            .iter()
            .any(|mention| mention.entity_type == "currency"));
        assert!(mentions
            .iter()
            .any(|mention| mention.entity_type == "mention" && mention.value == "@ada_lovelace"));
        assert!(mentions.iter().any(|mention| mention.entity_type == "time"));
    }

    #[test]
    fn agreement_and_conflict_are_graded() {
        let left = extract("Ada Lovelace met Grace Hopper");
        let right_same = extract("Ada Lovelace met Grace Hopper");
        assert!(
            entity_agreement(
                &left,
                "Ada Lovelace met Grace Hopper",
                &right_same,
                "Ada Lovelace met Grace Hopper"
            ) > 0.9
        );
        assert_eq!(entity_conflict(&left, &right_same), 0.0);
        let right_other = extract("Alan Turing met John Neumann");
        assert_eq!(
            entity_agreement(
                &left,
                "Ada Lovelace met Grace Hopper",
                &right_other,
                "Alan Turing met John Neumann"
            ),
            0.0
        );
        assert_eq!(entity_conflict(&left, &right_other), 1.0);
        // Missing evidence never penalizes.
        assert_eq!(entity_agreement(&left, "x", &[], "y"), 0.0);
        assert_eq!(entity_conflict(&left, &[]), 0.0);
    }

    #[test]
    fn bounds_cap_count_and_span() {
        let provider = RuleBasedEntityProvider::new(2, 8);
        let mentions = provider.extract(
            "Ada Lovelace and Grace Hopper met Alan Turing at Example Corporation",
            None,
        );
        assert!(mentions.len() <= 2);
        assert!(mentions
            .iter()
            .all(|mention| mention.value.chars().count() <= 8));
    }
}
