use unicode_segmentation::UnicodeSegmentation;

use crate::core::error::ProviderError;
use crate::core::providers::LanguageDetectionProvider;
use crate::core::types::{LanguageCandidate, MessageSegment};
use crate::language::detector::detect_languages;
use crate::lexical::tokenizer::is_emoji;
use crate::visual::scripts::script_name;

fn classify(piece: &str) -> &'static str {
    if piece.starts_with("http://") || piece.starts_with("https://") {
        "url"
    } else if piece.contains('@') && piece.contains('.') {
        "email"
    } else if piece.starts_with('@') && piece.len() > 1 {
        "mention"
    } else if piece.starts_with('#') && piece.len() > 1 {
        "hashtag"
    } else if !piece.is_empty() && piece.chars().all(|ch| ch.is_numeric()) {
        "number"
    } else if piece.chars().any(is_emoji) {
        "emoji"
    } else if piece
        .chars()
        .all(|ch| ch.is_alphabetic() || ch == '\'' || ch == '_')
    {
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
    if chunk.starts_with("http://")
        || chunk.starts_with("https://")
        || (chunk.contains('@') && chunk.contains('.'))
        || (chunk.starts_with('@') && chunk.len() > 1)
        || (chunk.starts_with('#') && chunk.len() > 1)
    {
        push_piece(pieces, chunk, chunk_start, chunk_end);
        return;
    }
    let graphemes: Vec<(usize, &str)> = chunk.grapheme_indices(true).collect();
    let mut index = 0;
    while index < graphemes.len() && pieces.len() < max_segments {
        let (relative_start, grapheme) = graphemes[index];
        let start = chunk_start + relative_start;
        let alphabetic = grapheme
            .chars()
            .all(|ch| ch.is_alphabetic() || ch == '\'' || ch == '_');
        let numeric = grapheme.chars().all(char::is_numeric);
        if alphabetic {
            let first = index;
            index += 1;
            while index < graphemes.len()
                && graphemes[index]
                    .1
                    .chars()
                    .all(|ch| ch.is_alphabetic() || ch == '\'' || ch == '_')
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
