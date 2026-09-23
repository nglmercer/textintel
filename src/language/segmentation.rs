use unicode_segmentation::UnicodeSegmentation;

use crate::core::error::ProviderError;
use crate::core::providers::LanguageDetectionProvider;
use crate::core::types::{LanguageCandidate, MessageSegment};
use crate::language::detector::detect_languages;
use crate::lexical::tokenizer::is_emoji;
use crate::visual::scripts::script_name;

/// Whether `text` can match an `http(s)://` prefix once ASCII-lowercased:
/// `to_ascii_lowercase` never mints ASCII from non-ASCII, so only `h`/`H`
/// first bytes qualify. Spares every other piece the lowercase copy.
fn maybe_http(text: &str) -> bool {
    text.len() >= 7 && (text.as_bytes()[0] | 32) == b'h'
}

fn classify(piece: &str) -> &'static str {
    if maybe_http(piece) {
        let lower = piece.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            return "url";
        }
    }
    if piece.contains('@') && piece.contains('.') {
        "email"
    } else if piece.starts_with('@') && piece.len() > 1 {
        "mention"
    } else if piece.starts_with('#') && piece.len() > 1 {
        "hashtag"
    } else if !piece.is_empty() && piece.chars().all(|ch| ch.is_numeric()) {
        "number"
    } else if piece.chars().any(is_emoji) {
        "emoji"
    } else if is_named_entity(piece) {
        "named_entity"
    } else if piece.chars().all(is_word_character) {
        "text"
    } else if piece
        .chars()
        .any(|ch| matches!(script_name(ch), Some("Symbol")))
    {
        "symbol"
    } else {
        "unknown"
    }
}

fn is_combining_mark(ch: char) -> bool {
    let code = ch as u32;
    (0x0300..=0x036f).contains(&code)
        || (0x1ab0..=0x1aff).contains(&code)
        || (0x1dc0..=0x1dff).contains(&code)
        || (0x20d0..=0x20ff).contains(&code)
        || (0xfe20..=0xfe2f).contains(&code)
}

pub(crate) fn is_word_character(ch: char) -> bool {
    ch.is_alphabetic()
        || is_combining_mark(ch)
        || matches!(ch, '\'' | '_' | '\u{200c}' | '\u{200d}')
}

fn is_no_space_script(piece: &str) -> bool {
    piece.chars().next().is_some_and(|ch| {
        matches!(
            script_name(ch),
            Some("Han")
                | Some("Hiragana")
                | Some("Katakana")
                | Some("Thai")
                | Some("Lao")
                | Some("Khmer")
                | Some("Myanmar")
        )
    })
}

fn is_named_entity(piece: &str) -> bool {
    let mut chars = piece.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_lowercase() && chars.clone().any(char::is_uppercase)
        || first.is_uppercase()
            && chars.clone().any(char::is_lowercase)
            && chars.any(|ch| ch.is_uppercase())
}

fn push_piece(pieces: &mut Vec<(String, usize, usize)>, text: &str, start: usize, end: usize) {
    if !text.is_empty() {
        pieces.push((text.to_string(), start, end));
    }
}

fn process_chunk(
    chunk: &str,
    chunk_start: usize,
    pieces: &mut Vec<(String, usize, usize)>,
    max_segments: usize,
) {
    if chunk.is_empty() || pieces.len() >= max_segments {
        return;
    }
    let chunk_end = chunk_start + chunk.len();
    // Keep network identifiers intact for spam and privacy-aware inspection.
    // The URL check lowercases only when the first byte permits a match.
    let is_url = maybe_http(chunk) && {
        let lower = chunk.to_ascii_lowercase();
        lower.starts_with("http://") || lower.starts_with("https://")
    };
    if is_url
        || (chunk.contains('@') && chunk.contains('.'))
        || (chunk.starts_with('@') && chunk.len() > 1)
        || (chunk.starts_with('#') && chunk.len() > 1)
    {
        push_piece(pieces, chunk, chunk_start, chunk_end);
        return;
    }
    if is_no_space_script(chunk) {
        process_no_space_chunk(chunk, chunk_start, pieces, max_segments);
        return;
    }
    let graphemes: Vec<(usize, &str)> = chunk.grapheme_indices(true).collect();
    let mut index = 0;
    while index < graphemes.len() && pieces.len() < max_segments {
        let (relative_start, grapheme) = graphemes[index];
        let start = chunk_start + relative_start;
        let alphabetic = grapheme.chars().all(is_word_character);
        let numeric = grapheme.chars().all(char::is_numeric);
        if alphabetic && !is_no_space_script(grapheme) {
            let first = index;
            index += 1;
            while index < graphemes.len()
                && graphemes[index].1.chars().all(is_word_character)
                && !is_no_space_script(graphemes[index].1)
            {
                index += 1;
            }
            let end = chunk_start
                + if index < graphemes.len() {
                    graphemes[index].0
                } else {
                    chunk.len()
                };
            let word: String = graphemes[first..index]
                .iter()
                .map(|(_, part)| *part)
                .collect();
            push_piece(pieces, &word, start, end);
        } else if alphabetic {
            push_piece(pieces, grapheme, start, start + grapheme.len());
            index += 1;
        } else if numeric {
            let first = index;
            index += 1;
            while index < graphemes.len() && graphemes[index].1.chars().all(char::is_numeric) {
                index += 1;
            }
            let end = chunk_start
                + if index < graphemes.len() {
                    graphemes[index].0
                } else {
                    chunk.len()
                };
            let number: String = graphemes[first..index]
                .iter()
                .map(|(_, part)| *part)
                .collect();
            push_piece(pieces, &number, start, end);
        } else {
            let end = start + grapheme.len();
            push_piece(pieces, grapheme, start, end);
            index += 1;
        }
    }
}

fn process_no_space_chunk(
    chunk: &str,
    chunk_start: usize,
    pieces: &mut Vec<(String, usize, usize)>,
    max_segments: usize,
) {
    let graphemes: Vec<(usize, &str)> = chunk.grapheme_indices(true).collect();
    let mut index = 0;
    while index < graphemes.len() && pieces.len() < max_segments {
        let (relative_start, grapheme) = graphemes[index];
        let start = chunk_start + relative_start;
        let word = grapheme.chars().all(is_word_character);
        let number = grapheme.chars().all(char::is_numeric);
        if word || number {
            let first = index;
            index += 1;
            while index < graphemes.len() {
                let next = graphemes[index].1;
                let same_kind = if word {
                    next.chars().all(is_word_character)
                } else {
                    next.chars().all(char::is_numeric)
                };
                if !same_kind {
                    break;
                }
                index += 1;
            }
            let end = chunk_start
                + if index < graphemes.len() {
                    graphemes[index].0
                } else {
                    chunk.len()
                };
            let value: String = graphemes[first..index]
                .iter()
                .map(|(_, part)| *part)
                .collect();
            push_piece(pieces, &value, start, end);
        } else {
            push_piece(pieces, grapheme, start, start + grapheme.len());
            index += 1;
        }
    }
}

/// Segment into bounded, independently classifiable pieces. Offsets are UTF-8
/// byte offsets into the original message.
pub fn segment_message(text: &str, max_segments: usize) -> Vec<MessageSegment> {
    segment_message_with_detector(text, max_segments, detect_languages)
}

/// Segment using a caller-provided language detector. This keeps custom
/// resource packs visible at both message and segment level.
pub fn segment_message_with_provider(
    text: &str,
    max_segments: usize,
    provider: &dyn LanguageDetectionProvider,
) -> Result<Vec<MessageSegment>, ProviderError> {
    let mut provider_error = None;
    let segments =
        segment_message_with_detector(text, max_segments, |piece| match provider.detect(piece) {
            Ok(candidates) => candidates,
            Err(error) => {
                provider_error = Some(error);
                vec![LanguageCandidate::new("unknown", 1.0)]
            }
        });
    match provider_error {
        Some(error) => Err(error),
        None => Ok(segments),
    }
}

pub fn segment_message_with_detector<F>(
    text: &str,
    max_segments: usize,
    mut detect: F,
) -> Vec<MessageSegment>
where
    F: FnMut(&str) -> Vec<LanguageCandidate>,
{
    let mut pieces = Vec::new();
    let mut chunk_start = 0;
    for (index, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if chunk_start < index {
                process_chunk(
                    &text[chunk_start..index],
                    chunk_start,
                    &mut pieces,
                    max_segments,
                );
            }
            chunk_start = index + ch.len_utf8();
            if pieces.len() >= max_segments {
                break;
            }
        }
    }
    if chunk_start < text.len() && pieces.len() < max_segments {
        process_chunk(&text[chunk_start..], chunk_start, &mut pieces, max_segments);
    }

    pieces
        .into_iter()
        .take(max_segments)
        .map(|(piece, start, end)| {
            let segment_type = classify(&piece).to_string();
            let language_candidates = if segment_type == "text" {
                detect(&piece)
            } else {
                vec![LanguageCandidate::new("unknown", 1.0)]
            };
            MessageSegment {
                text: piece,
                start,
                end,
                language_candidates,
                segment_type,
            }
        })
        .collect()
}
