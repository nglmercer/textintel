//! Single-position word-swap probes for contextual hard negatives.
//! Detects sentence pairs differing in exactly one word and scores the
//! swapped words (character + phonetic) for confusable separation.

use crate::core::types::MessageFingerprint;
use crate::lexical::character::combined_character_similarity;
use crate::normalization::unicode::casefold_text;
use crate::phonetic::similarity::phonetic_similarity;

use super::word_tokens;

/// The differing word position and pair when both sides carry the same
/// number (≥ 2) of word tokens differing in exactly one position (compared
/// case-insensitively); `None` otherwise. Single-word pairs are excluded:
/// their whole-string `character` channel already carries the same signal.
pub(crate) fn swapped_words<'a>(
    a: &'a MessageFingerprint,
    b: &'a MessageFingerprint,
) -> Option<(usize, &'a str, &'a str)> {
    let left = word_tokens(a);
    let right = word_tokens(b);
    if left.len() < 2 || right.len() < 2 {
        return None;
    }
    if left.len() == right.len() {
        let mut differing: Option<(usize, &str, &str)> = None;
        for (index, (left_word, right_word)) in left.iter().zip(right.iter()).enumerate() {
            if casefold_text(left_word) == casefold_text(right_word) {
                continue;
            }
            if differing.is_some() {
                return None;
            }
            differing = Some((index, *left_word, *right_word));
        }
        return differing;
    }
    // Uneven sides differing by exactly one token: align past a single
    // insertion/deletion, then require exactly one substitution
    // (`win a free prize now` / `win free praise now` skips `a` and swaps
    // `prize`/`praise`). Pure insertions (`quick brown fox` / `the quick
    // brown fox`) align with zero substitutions and stay `None`. Leftmost
    // skip wins so the alignment is deterministic; the reported position is
    // in the longer side's coordinates. Both sides still need ≥ 2 tokens:
    // single-word expansions (`wanna` / `want to`) are not swaps.
    if left.len().abs_diff(right.len()) != 1 {
        return None;
    }
    let (long, short, swapped) = if left.len() > right.len() {
        (left, right, false)
    } else {
        (right, left, true)
    };
    for skip in 0..long.len() {
        let rest: Vec<&&str> = long
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != skip)
            .map(|(_, word)| word)
            .collect();
        let mut differing: Option<(usize, &str, &str)> = None;
        let mut aligned = true;
        for (index, (long_word, short_word)) in rest.iter().zip(short.iter()).enumerate() {
            if casefold_text(long_word) == casefold_text(short_word) {
                continue;
            }
            if differing.is_some() {
                aligned = false;
                break;
            }
            // Position in the longer side's coordinates (shift past the
            // skip when the difference sits at or after it).
            let position = if index >= skip { index + 1 } else { index };
            differing = Some((position, **long_word, *short_word));
        }
        if aligned {
            if let Some((position, long_word, short_word)) = differing {
                return Some(if swapped {
                    (position, short_word, long_word)
                } else {
                    (position, long_word, short_word)
                });
            }
        }
    }
    None
}

/// Top substantive language per text segment, in segment order, when the
/// text segments align 1:1 with the word tokens (compared
/// case-insensitively); `None` otherwise. Powers cross-language swap
/// suppression: without clean alignment there is no suppression (graceful
/// abstention, never misattribution).
fn aligned_segment_languages(fingerprint: &MessageFingerprint) -> Option<Vec<Option<String>>> {
    let words = word_tokens(fingerprint);
    let text_segments: Vec<_> = fingerprint
        .segments
        .iter()
        .filter(|segment| segment.segment_type == "text")
        .collect();
    if text_segments.len() != words.len() {
        return None;
    }
    for (segment, word) in text_segments.iter().zip(words.iter()) {
        if casefold_text(&segment.text) != casefold_text(word) {
            return None;
        }
    }
    Some(
        text_segments
            .iter()
            .map(|segment| {
                segment
                    .language_candidates
                    .first()
                    .filter(|candidate| !candidate.language.eq_ignore_ascii_case("unknown"))
                    .map(|candidate| candidate.language.clone())
            })
            .collect(),
    )
}

/// 0.0 when the swapped words at `position` sit in known-different-language
/// segments — a language switch (`mi`/`my`), not a confusable — else 1.0.
/// Any ambiguity (no alignment, unknown top language, same language) keeps
/// the penalty: suppression fails closed toward catching confusables.
pub(crate) fn same_language_swap(
    a: &MessageFingerprint,
    b: &MessageFingerprint,
    position: usize,
) -> f64 {
    match (aligned_segment_languages(a), aligned_segment_languages(b)) {
        (Some(left), Some(right)) => match (left.get(position), right.get(position)) {
            (Some(Some(left)), Some(Some(right))) if !left.eq_ignore_ascii_case(right) => 0.0,
            _ => 1.0,
        },
        _ => 1.0,
    }
}

pub(crate) fn swapped_word_similarity(a: &MessageFingerprint, b: &MessageFingerprint) -> f64 {
    match swapped_words(a, b) {
        Some((_, left_word, right_word)) => {
            combined_character_similarity(left_word, right_word).clamp(0.0, 1.0)
        }
        None => 0.0,
    }
}

/// 1.0 when the swap gate fires and BOTH swapped words are lexicon
/// words (any language), else 0.0. Separates malapropisms (`money`/`honey`,
/// both real words, meanings differ) from typos (`you`/`xou`, one side
/// misspelled, meanings match): pair-level `lexicon_validity` dilutes this
/// signal over sentence length, while the swap position carries it exactly.
/// Mirrors `lexicon_coverage` (raw token or its alphanumeric fold), over the
/// embedded lexicon — the same default-provider precedent as
/// `swapped_phonetic_similarity` below, since the scorer trait carries no
/// provider handle.
pub(crate) fn swapped_validity(a: &MessageFingerprint, b: &MessageFingerprint) -> f64 {
    use crate::core::providers::LexiconProvider;
    let Some((_, left_word, right_word)) = swapped_words(a, b) else {
        return 0.0;
    };
    let provider = crate::resources::DefaultLexiconProvider;
    let valid = |word: &str| {
        provider.contains(word, None) || provider.contains(&alphanumeric_fold(word), None)
    };
    if valid(left_word) && valid(right_word) {
        1.0
    } else {
        0.0
    }
}

fn alphanumeric_fold(text: &str) -> String {
    casefold_text(text)
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}

/// Phonetic similarity of the swapped words (rule-based G2P under the
/// default `und` rules, so both sides phonemize comparably regardless of
/// detected language); 0.0 when the swap gate above does not fire.
pub(crate) fn swapped_phonetic_similarity(a: &MessageFingerprint, b: &MessageFingerprint) -> f64 {
    use crate::core::providers::G2PProvider;
    let Some((_, left_word, right_word)) = swapped_words(a, b) else {
        return 0.0;
    };
    let g2p = crate::phonetic::g2p::RuleBasedG2PProvider;
    let left = g2p.phonemize(left_word, "und");
    let right = g2p.phonemize(right_word, "und");
    match (left, right) {
        (Ok(left), Ok(right)) => phonetic_similarity(&left, &right).clamp(0.0, 1.0),
        _ => 0.0,
    }
}
