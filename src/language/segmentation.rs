use unicode_segmentation::UnicodeSegmentation;

use crate::core::types::{LanguageCandidate, MessageSegment};
use crate::language::detector::detect_languages;
use crate::lexical::tokenizer::is_emoji;

fn classify(piece: &str) -> &'static str {
    if piece.starts_with("http://") || piece.starts_with("https://") {
        "url"
    } else if piece.contains('@') && piece.contains('.') {
        "email"
    } else if piece.starts_with('@') && piece.len() > 1 {
        "mention"
    } else if piece.starts_with('#') && piece.len() > 1 {
        "hashtag"
    } else if piece.chars().all(|ch| ch.is_numeric()) {
        "number"
    } else if piece.chars().any(is_emoji) {
        "emoji"
    } else if piece.chars().all(|ch| ch.is_alphabetic() || ch == '\'' || ch == '_') {
        "text"
    } else if piece.chars().any(|ch| ch.is_symbol()) {
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

/// Segment into bounded, independently classifiable pieces.  Offsets are
/// byte offsets into the original UTF-8 string.
pub fn segment_message(text: &str, max_segments: usize) -> Vec<MessageSegment> {
    let mut pieces = Vec::new();
    for (chunk_start, chunk) in text.split_word_bound_indices() {
        if chunk.trim().is_empty() {
            continue;
        }
        let chunk_end = chunk_start + chunk.len();
        // URL/email/mention/hashtag chunks are already useful as one unit.
        if chunk.starts_with("http://")
            || chunk.starts_with("https://")
            || (chunk.contains('@') && chunk.contains('.'))
            || (chunk.starts_with('@') && chunk.len() > 1)
            || (chunk.starts_with('#') && chunk.len() > 1)
        {
            push_piece(&mut pieces, chunk, chunk_start, chunk_end);
            continue;
        }
        let graphemes: Vec<(usize, &str)> = chunk.grapheme_indices(true).collect();
        let mut index = 0;
        while index < graphemes.len() {
            let (relative_start, grapheme) = graphemes[index];
            let start = chunk_start + relative_start;
            if grapheme.chars().all(|ch| ch.is_alphabetic() || ch == '\'' || ch == '_') {
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
                let end = chunk_start + if index < graphemes.len() { graphemes[index].0 } else { chunk.len() };
                let word: String = graphemes[first..index].iter().map(|(_, part)| *part).collect();
                push_piece(&mut pieces, &word, start, end);
            } else if grapheme.chars().all(char::is_numeric) {
                let first = index;
                index += 1;
                while index < graphemes.len() && graphemes[index].1.chars().all(char::is_numeric) {
                    index += 1;
                }
                let end = chunk_start + if index < graphemes.len() { graphemes[index].0 } else { chunk.len() };
                let number: String = graphemes[first..index].iter().map(|(_, part)| *part).collect();
                push_piece(&mut pieces, &number, start, end);
            } else {
                let end = start + grapheme.len();
                push_piece(&mut pieces, grapheme, start, end);
                index += 1;
            }
            if pieces.len() >= max_segments {
                break;
            }
        }
        if pieces.len() >= max_segments {
            break;
        }
    }

    pieces
        .into_iter()
        .take(max_segments)
        .map(|(piece, start, end)| {
            let segment_type = classify(&piece).to_string();
            let language_candidates = if segment_type == "text" {
                let mut candidates = detect_languages(&piece);
                if matches!(piece.to_lowercase().as_str(), "bro" | "now" | "ok" | "lol") {
                    candidates = vec![LanguageCandidate::new("en", 0.7), LanguageCandidate::new("unknown", 0.3)];
                }
                candidates
            } else {
                vec![LanguageCandidate::new("unknown", 1.0)]
            };
            MessageSegment { text: piece, start, end, language_candidates, segment_type }
        })
        .collect()
}

