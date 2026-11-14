//! Bounded sentence segmentation for speech preparation.
//! Third-party copyright and license: see normalization/LICENSE.
//! Normalize the complete input before segmenting. Whitespace at chunk
//! boundaries is canonicalized; other characters retain their order.
//! A grapheme larger than the limit is split to enforce the hard bound.
use crate::speech::normalization::segments;
/// Default hard maximum in Unicode scalar values.
pub const DEFAULT_MAX_CHUNK_CHARS: usize = 200;
pub fn split_sentences(text: &str) -> Vec<String> {
    split_sentences_with_limit(text, DEFAULT_MAX_CHUNK_CHARS)
}

/// Split `text` into chunks of at most `max_chars` Unicode characters.
///
/// Strategy: prefer protected sentence boundaries, then whitespace, then
/// a hard split between grapheme clusters (by character count only when
/// a single cluster exceeds the limit). No returned chunk ever exceeds
/// `max_chars` characters (measured with [`str::chars`], not UTF-8
/// bytes).
///
/// Sentence boundaries come from
/// [`crate::speech::normalization::segments`]: a `.` only ends a sentence
/// when followed by whitespace (or the end) and not after a known
/// abbreviation or initial, so decimals (`3.14`), IP addresses, versions
/// and URLs are never cut. `? ! ; …` follow the same rule; `。 ？ ！ ；`
/// and newlines always end a sentence.
pub fn split_sentences_with_limit(text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(1);
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    if text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }

    // 1. Split at protected sentence boundaries.
    let sentences: Vec<&str> = segments::sentence_spans(text)
        .into_iter()
        .map(|span| text[span].trim())
        .filter(|sentence| !sentence.is_empty())
        .collect();

    // 2. Greedily pack sentences, splitting oversized ones.
    let mut chunks = Vec::new();
    let mut pending = String::new();
    let mut pending_len = 0usize;
    for sentence in sentences {
        for piece in split_oversized(sentence, max_chars) {
            let piece_len = piece.chars().count();
            if pending_len > 0 && pending_len + 1 + piece_len > max_chars {
                chunks.push(std::mem::take(&mut pending));
                pending_len = 0;
            }
            if pending_len > 0 {
                pending.push(' ');
                pending_len += 1;
            }
            pending.push_str(&piece);
            pending_len += piece_len;
        }
    }
    if !pending.trim().is_empty() {
        chunks.push(pending.trim().to_string());
    }
    if chunks.is_empty() {
        chunks.push(hard_truncate(text, max_chars));
    }
    chunks
}

/// Split a single sentence that may exceed `max_chars`: first at whitespace,
/// then by hard character count. Every piece is <= `max_chars` chars.
fn split_oversized(sentence: &str, max_chars: usize) -> Vec<String> {
    if sentence.chars().count() <= max_chars {
        return vec![sentence.to_string()];
    }
    // Try whitespace splitting.
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for word in sentence.split_whitespace() {
        let word_len = word.chars().count();
        if word_len > max_chars {
            // Single word exceeds the limit: flush and hard-split the word.
            if !current.is_empty() {
                pieces.push(std::mem::take(&mut current));
                current_len = 0;
            }
            pieces.extend(hard_split(word, max_chars));
        } else {
            if current_len > 0 && current_len + 1 + word_len > max_chars {
                pieces.push(std::mem::take(&mut current));
                current_len = 0;
            }
            if current_len > 0 {
                current.push(' ');
                current_len += 1;
            }
            current.push_str(word);
            current_len += word_len;
        }
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    if pieces.is_empty() {
        // No whitespace at all: hard-split by character count.
        pieces.extend(hard_split(sentence, max_chars));
    }
    pieces
}

/// Hard split that never cuts inside a grapheme cluster unless the cluster
/// alone exceeds `max_chars`.
fn hard_split(text: &str, max_chars: usize) -> Vec<String> {
    segments::hard_split(text, max_chars)
}

fn hard_truncate(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_bounded(chunks: &[String], max_chars: usize) {
        assert!(!chunks.is_empty());
        for chunk in chunks {
            let len = chunk.chars().count();
            assert!(
                len <= max_chars,
                "chunk of {len} chars exceeds limit {max_chars}: {chunk:?}"
            );
            assert!(!chunk.is_empty());
        }
    }

    #[test]
    fn short_text_is_single_chunk() {
        assert_eq!(split_sentences("Hello."), vec!["Hello."]);
    }

    #[test]
    fn normal_punctuation_splits() {
        let text = "Hello world. This is a test! How are you? Fine, thanks.";
        let chunks = split_sentences_with_limit(text, 30);
        assert_bounded(&chunks, 30);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn long_punctuation_free_ascii_is_bounded() {
        let text = "lorem ipsum dolor sit amet ".repeat(40);
        let chunks = split_sentences_with_limit(&text, 200);
        assert_bounded(&chunks, 200);
        assert!(chunks.len() > 1);
    }

    #[test]
    fn long_single_word_without_whitespace_is_bounded() {
        let text = "a".repeat(1000);
        let chunks = split_sentences_with_limit(&text, 200);
        assert_bounded(&chunks, 200);
        assert_eq!(chunks.len(), 5);
    }

    #[test]
    fn cjk_text_without_spaces_is_bounded() {
        let text = "日本語のテスト文章です。".repeat(60);
        let chunks = split_sentences_with_limit(&text, 200);
        assert_bounded(&chunks, 200);
    }

    #[test]
    fn emoji_text_is_bounded_by_chars_not_bytes() {
        // Each emoji is 4 UTF-8 bytes but 1 char; 300 emojis = 1200 bytes.
        let text = "🎙️".repeat(150);
        let chunks = split_sentences_with_limit(&text, 200);
        assert_bounded(&chunks, 200);
        // Must not split in the middle of nothing: rejoining covers input.
        let joined_len: usize = chunks.iter().map(|c| c.chars().count()).sum();
        assert_eq!(joined_len, text.chars().count());
    }

    #[test]
    fn mixed_unicode_long_sentence_is_bounded() {
        let text = "Héllo wörld, this is à ünïcodé sentence without any period ".repeat(20);
        let chunks = split_sentences_with_limit(&text, 200);
        assert_bounded(&chunks, 200);
    }

    #[test]
    fn chunks_never_exceed_hard_limit_on_small_limit() {
        let text = "one two three four five six seven eight nine ten. ".repeat(10);
        for limit in [1, 7, 50] {
            let chunks = split_sentences_with_limit(&text, limit);
            assert_bounded(&chunks, limit);
        }
    }

    /// Non-whitespace characters of `text`, in order.
    fn dense(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace()).collect()
    }

    fn assert_covers(chunks: &[String], text: &str) {
        assert_eq!(
            dense(&chunks.concat()),
            dense(text),
            "chunks lost or reordered text"
        );
    }

    #[test]
    fn protected_tokens_are_never_split() {
        for token in [
            "3.14",
            "10.0.0.1:8080",
            "https://example.org/a?b=3.14",
            "v1.2.3",
            "v1.2.3-beta.2",
            "1.21.11",
            "1.234,56",
            "user@example.org",
        ] {
            let filler = "palabra ".repeat(6);
            let text = format!("{filler}valor {token} final. {filler}otra frase aquí.");
            let chunks = split_sentences_with_limit(&text, 80);
            assert_bounded(&chunks, 80);
            assert_covers(&chunks, &text);
            assert!(
                chunks.iter().any(|c| c.contains(token)),
                "token {token} was cut: {chunks:?}"
            );
        }
    }

    #[test]
    fn decimal_is_not_a_sentence_boundary() {
        let text = "El valor es 3.14 y sigue la frase sin cortar nada más.";
        let chunks = split_sentences_with_limit(text, 40);
        assert_bounded(&chunks, 40);
        assert_covers(&chunks, text);
        assert!(chunks.iter().any(|c| c.contains("3.14")));
    }

    #[test]
    fn abbreviation_is_not_a_sentence_boundary() {
        let text = "Ayer vino el Dr. García. Hoy no vino.";
        let chunks = split_sentences_with_limit(text, 24);
        assert_bounded(&chunks, 24);
        assert_eq!(chunks, vec!["Ayer vino el Dr. García.", "Hoy no vino."]);
    }

    #[test]
    fn sentence_boundaries_still_split() {
        let text = "Primera frase. ¿Segunda? ¡Tercera! Cuarta; quinta\nsexta 。七";
        let chunks = split_sentences_with_limit(text, 16);
        assert_bounded(&chunks, 16);
        assert_covers(&chunks, text);
        assert!(chunks.len() >= 5);
    }

    #[test]
    fn zwj_emoji_and_accents_survive_hard_split() {
        let family = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
        let text = format!("{} Pingüino niño acción áéíóú", family.repeat(30));
        let chunks = split_sentences_with_limit(&text, 20);
        assert_bounded(&chunks, 20);
        assert_covers(&chunks, &text);
        let families: usize = chunks.iter().map(|c| c.matches(family).count()).sum();
        assert_eq!(families, 30, "a ZWJ family was split: {chunks:?}");
        assert!(chunks.iter().any(|c| c.contains("Pingüino")));
    }
}
use serde::Serialize;

/// Suggested inter-fragment silence in seconds for consumers assembling audio.
/// Consumers that manage continuity can configure their own value.
pub const DEFAULT_INTER_FRAGMENT_SILENCE_SECS: f64 = 0.2;

/// One independently completable piece of a long-form synthesis request.
///
/// Fragments always preserve the original order (`index` in `0..total`);
/// stitching concatenates fragment audio in index order with the policy's
/// inter-fragment silence, so duplicated, omitted, or reordered speech at
/// boundaries is a programming error, never an accepted outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fragment {
    /// Position in the original order (`0`-based).
    pub index: usize,
    /// Total fragment count of the request this fragment belongs to.
    pub total: usize,
    /// Prepared (normalized) text of this fragment. Never empty.
    pub text: String,
}

impl Fragment {
    /// Length in Unicode characters (not UTF-8 bytes).
    pub fn char_count(&self) -> usize {
        self.text.chars().count()
    }
}

/// Provider-specific long-text chunk policy.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentPolicy {
    /// Maximum fragment size in Unicode characters.
    pub max_chars: usize,
    /// Silence inserted between stitched fragments, in seconds.
    pub inter_fragment_silence_secs: f64,
}

impl SegmentPolicy {
    /// Character-bounded sentence policy. A zero limit is treated as one.
    /// The default limit is [`DEFAULT_MAX_CHUNK_CHARS`].
    pub fn bounded(max_chars: usize) -> Self {
        Self {
            max_chars: max_chars.max(1),
            inter_fragment_silence_secs: DEFAULT_INTER_FRAGMENT_SILENCE_SECS,
        }
    }
}

impl Default for SegmentPolicy {
    fn default() -> Self {
        Self::bounded(DEFAULT_MAX_CHUNK_CHARS)
    }
}

/// Split prepared text into ordered, independently completable [`Fragment`]s.
///
/// Contract:
/// - empty/blank input yields no fragments;
/// - no fragment is empty and none exceeds `policy.max_chars` characters;
/// - cuts prefer protected sentence boundaries, then whitespace, then a
///   hard split between graphemes when the bound permits (via
///   [`split_sentences_with_limit`]);
/// - `index`/`total` always reflect the original order.
pub fn segment_text(text: &str, policy: &SegmentPolicy) -> Vec<Fragment> {
    let chunks = split_sentences_with_limit(text, policy.max_chars);
    let total = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, text)| Fragment { index, total, text })
        .collect()
}

#[cfg(test)]
mod fragment_tests {
    use super::*;

    fn policy(max_chars: usize) -> SegmentPolicy {
        SegmentPolicy::bounded(max_chars)
    }

    #[test]
    fn empty_input_yields_no_fragments() {
        assert!(segment_text("", &policy(200)).is_empty());
        assert!(segment_text("   \n\t  ", &policy(200)).is_empty());
    }

    #[test]
    fn single_fragment_carries_index_and_total() {
        let fragments = segment_text("Hola mundo.", &policy(200));
        assert_eq!(fragments.len(), 1);
        assert_eq!(
            fragments[0],
            Fragment {
                index: 0,
                total: 1,
                text: "Hola mundo.".to_string(),
            }
        );
        assert_eq!(fragments[0].char_count(), "Hola mundo.".chars().count());
    }

    #[test]
    fn fragments_preserve_order_bounds_and_content() {
        let text = "Primera frase. Segunda frase con 3.14 decimales. \
                    Tercera frase con https://example.com/page?x=1 dentro. \
                    Cuarta y última frase para completar el texto largo.";
        let fragments = segment_text(text, &policy(60));
        assert!(fragments.len() > 1);
        // Order, contiguity, bounds, non-emptiness.
        for (i, fragment) in fragments.iter().enumerate() {
            assert_eq!(fragment.index, i);
            assert_eq!(fragment.total, fragments.len());
            assert!(!fragment.text.is_empty());
            assert!(
                fragment.char_count() <= 60,
                "fragment exceeds policy: {fragment:?}"
            );
        }
        // No content lost or duplicated: concatenation of the fragments
        // holds every word of the input exactly once.
        let joined = fragments
            .iter()
            .map(|fragment| fragment.text.clone())
            .collect::<Vec<_>>()
            .join(" ");
        for word in text.split_whitespace() {
            assert!(
                joined.split_whitespace().filter(|w| *w == word).count()
                    >= text.split_whitespace().filter(|w| *w == word).count(),
                "lost content near {word:?}"
            );
        }
    }

    #[test]
    fn protected_expressions_survive_segmentation() {
        // Decimals, dates, URLs and abbreviations must never be cut, even
        // when fragments are tiny.
        let text = "El valor es 3.14 euros. Fecha 26/10/2026. Ver https://example.com/a/b. Sr. García vino.";
        let fragments = segment_text(text, &policy(30));
        let joined = fragments
            .iter()
            .map(|fragment| fragment.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        for protected in ["3.14", "26/10/2026", "https://example.com/a/b", "Sr."] {
            assert!(
                joined.contains(protected),
                "protected expression cut: {protected:?} in {joined:?}"
            );
        }
    }

    #[test]
    fn grapheme_clusters_are_never_split() {
        // Family emoji (ZWJ sequence) plus combining marks: every char of
        // each cluster must land in the same fragment.
        let text = "👨‍👩‍👧‍👦 holá adiós amigo";
        let fragments = segment_text(text, &policy(8));
        for fragment in &fragments {
            assert!(fragment.char_count() <= 8);
        }
        let joined: String = fragments.iter().map(|f| f.text.clone()).collect();
        assert!(joined.contains("👨‍👩‍👧‍👦"));
    }

    #[test]
    fn default_policy_matches_bounded_chunk_limit() {
        assert_eq!(SegmentPolicy::default().max_chars, DEFAULT_MAX_CHUNK_CHARS);
    }
}
