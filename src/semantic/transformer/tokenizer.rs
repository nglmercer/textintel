//! BERT WordPiece tokenizer over a `vocab.txt` vocabulary.

use std::collections::BTreeMap;

use crate::core::error::ProviderError;

use super::{invalid, VOCAB_FILE};

/// BERT WordPiece tokenizer over a `vocab.txt` vocabulary (one token per
/// line, id = line number). Requires `[PAD]`, `[UNK]`, `[CLS]`, `[SEP]`.
#[derive(Debug, Clone)]
pub(crate) struct WordPieceTokenizer {
    pub(crate) vocab: BTreeMap<String, u32>,
    pub(crate) unk_id: u32,
    pub(crate) cls_id: u32,
    pub(crate) sep_id: u32,
    pub(crate) pad_id: u32,
    pub(crate) lowercase: bool,
}

impl WordPieceTokenizer {
    pub(crate) fn from_vocab_txt(source: &str, lowercase: bool) -> Result<Self, ProviderError> {
        let mut vocab = BTreeMap::new();
        for (index, line) in source.lines().enumerate() {
            let token = line.trim_end_matches(['\n', '\r']);
            if token.is_empty() {
                continue;
            }
            let id = u32::try_from(index)
                .map_err(|_| invalid(format!("{VOCAB_FILE}: vocabulary exceeds u32 ids")))?;
            vocab.insert(token.to_string(), id);
        }
        if vocab.is_empty() {
            return Err(invalid(format!("{VOCAB_FILE}: vocabulary is empty")));
        }
        let lookup = |special: &str| {
            vocab
                .get(special)
                .copied()
                .ok_or_else(|| invalid(format!("{VOCAB_FILE}: missing required token `{special}`")))
        };
        Ok(Self {
            unk_id: lookup("[UNK]")?,
            cls_id: lookup("[CLS]")?,
            sep_id: lookup("[SEP]")?,
            pad_id: lookup("[PAD]")?,
            vocab,
            lowercase,
        })
    }

    /// Basic tokenization: whitespace split, punctuation/CJK isolation,
    /// optional uncased folding with accent stripping.
    fn basic_tokens(&self, text: &str) -> Vec<String> {
        let mut spaced = String::with_capacity(text.len() + 8);
        for ch in text.chars() {
            if ch.is_whitespace() {
                spaced.push(' ');
            } else if is_split_char(ch) {
                spaced.push(' ');
                spaced.push(ch);
                spaced.push(' ');
            } else {
                spaced.push(ch);
            }
        }
        spaced
            .split_whitespace()
            .map(|word| {
                if self.lowercase {
                    strip_accents(&word.to_lowercase())
                } else {
                    word.to_string()
                }
            })
            .collect()
    }

    /// Greedy longest-match WordPiece segmentation of one basic token.
    fn word_pieces(&self, word: &str) -> Vec<u32> {
        if word.len() > 100 {
            return vec![self.unk_id];
        }
        let chars: Vec<char> = word.chars().collect();
        let mut pieces = Vec::new();
        let mut start = 0;
        while start < chars.len() {
            let mut end = chars.len();
            let mut found = None;
            while end > start {
                let candidate: String = chars[start..end].iter().collect();
                let key = if start == 0 {
                    candidate
                } else {
                    format!("##{candidate}")
                };
                if let Some(id) = self.vocab.get(&key) {
                    found = Some(*id);
                    break;
                }
                end -= 1;
            }
            match found {
                Some(id) => {
                    pieces.push(id);
                    start = end;
                }
                None => return vec![self.unk_id],
            }
        }
        pieces
    }

    /// Encode to `[CLS] pieces [SEP]`, truncating word pieces to `max_len - 2`.
    /// Returns `(ids, truncated)`.
    pub(crate) fn encode(&self, text: &str, max_len: usize) -> (Vec<u32>, bool) {
        let capacity = max_len.saturating_sub(2).max(1);
        let mut pieces = Vec::new();
        let mut truncated = false;
        'words: for word in self.basic_tokens(text) {
            for piece in self.word_pieces(&word) {
                if pieces.len() >= capacity {
                    truncated = true;
                    break 'words;
                }
                pieces.push(piece);
            }
        }
        let mut ids = Vec::with_capacity(pieces.len() + 2);
        ids.push(self.cls_id);
        ids.extend(pieces);
        ids.push(self.sep_id);
        (ids, truncated)
    }
}

fn is_split_char(ch: char) -> bool {
    if ch.is_ascii_punctuation() {
        return true;
    }
    // Loose CJK/letter-spacing for Han, Hiragana, Katakana, Hangul blocks.
    matches!(ch,
        '\u{2E80}'..='\u{2EFF}' | '\u{3000}'..='\u{303F}' | '\u{3040}'..='\u{309F}'
        | '\u{30A0}'..='\u{30FF}' | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}'
        | '\u{AC00}'..='\u{D7AF}' | '\u{F900}'..='\u{FAFF}' | '\u{FF00}'..='\u{FFEF}')
}

fn strip_accents(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    text.nfd().filter(|ch| !is_combining_mark(*ch)).collect()
}

fn is_combining_mark(ch: char) -> bool {
    matches!(ch, '\u{300}'..='\u{36F}' | '\u{1AB0}'..='\u{1AFF}' | '\u{1DC0}'..='\u{1DFF}'
        | '\u{20D0}'..='\u{20FF}' | '\u{FE20}'..='\u{FE2F}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wordpiece_handles_unknown_and_truncation() {
        let source = "[PAD]\n[UNK]\n[CLS]\n[SEP]\n[MASK]\nhello\n##s\nworld\n";
        let tokenizer = WordPieceTokenizer::from_vocab_txt(source, false).unwrap();
        let (ids, truncated) = tokenizer.encode("hello worlds", 32);
        assert!(!truncated);
        // "worlds" -> "world" + "##s".
        assert_eq!(ids.len(), 5, "CLS hello world ##s SEP, got {ids:?}");
        let (unk, _) = tokenizer.encode("xyzzy", 32);
        assert!(unk.contains(&tokenizer.unk_id));
        let (_, long) = tokenizer.encode("hello ".repeat(100).as_str(), 8);
        assert!(long);
    }
}
