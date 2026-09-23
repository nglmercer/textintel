//! BERT-family tokenizers: WordPiece over `vocab.txt` and SentencePiece-Unigram
//! over a Hugging Face `tokenizer.json` (the `tokenizers` Unigram subset).

use std::collections::{BTreeMap, HashMap};

use crate::core::error::ProviderError;

use super::{TOKENIZER_FILE, VOCAB_FILE, invalid};

/// BERT WordPiece tokenizer over a `vocab.txt` vocabulary (one token per
/// line, id = line number). Requires `[PAD]`, `[UNK]`, `[CLS]`, `[SEP]`.
#[derive(Debug, Clone)]
pub(crate) struct WordPieceTokenizer {
    // Hashed lookups only (never iterated), so hashing changes no id.
    pub(crate) vocab: HashMap<String, u32>,
    pub(crate) unk_id: u32,
    pub(crate) cls_id: u32,
    pub(crate) sep_id: u32,
    pub(crate) lowercase: bool,
}

impl WordPieceTokenizer {
    pub(crate) fn from_vocab_txt(source: &str, lowercase: bool) -> Result<Self, ProviderError> {
        let mut vocab = HashMap::new();
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
        // Forwards run unpadded, but a BERT vocab without [PAD] is
        // malformed: keep rejecting it at load.
        lookup("[PAD]")?;
        Ok(Self {
            unk_id: lookup("[UNK]")?,
            cls_id: lookup("[CLS]")?,
            sep_id: lookup("[SEP]")?,
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
    /// Candidates shrink one reusable buffer from the longest match down,
    /// so the same longest-first attempts run with no per-attempt
    /// allocation.
    fn word_pieces(&self, word: &str) -> Vec<u32> {
        if word.len() > 100 {
            return vec![self.unk_id];
        }
        let chars: Vec<char> = word.chars().collect();
        let mut pieces = Vec::new();
        let mut candidate = String::new();
        let mut start = 0;
        while start < chars.len() {
            candidate.clear();
            if start > 0 {
                candidate.push_str("##");
            }
            candidate.extend(chars[start..].iter().copied());
            let mut found = None;
            for end in (start + 1..=chars.len()).rev() {
                if let Some(id) = self.vocab.get(candidate.as_str()) {
                    found = Some(*id);
                    start = end;
                    break;
                }
                candidate.pop();
            }
            match found {
                Some(id) => pieces.push(id),
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

/// SentencePiece underline: word-boundary marker (`U+2581`).
const SPIECE_UNDERLINE: char = '\u{2581}';
/// Viterbi score for the single-character fallback (byte pieces or `[UNK]`),
/// lower than any real vocabulary score so it only covers truly unmatchable
/// characters.
const FALLBACK_SCORE: f32 = -1e9;

/// SentencePiece-Unigram tokenizer over a Hugging Face `tokenizer.json`
/// vocabulary (multilingual-e5 style).
///
/// Parses the `tokenizers` Unigram subset: `model.vocab` as `[piece, score]`
/// pairs (id = position), `model.unk_id`, `model.byte_fallback`, and the
/// `<s>` / `</s>` / `<pad>` ids from `added_tokens` (falling back to pieces
/// with the same text). Encoding follows SentencePiece framing (whitespace
/// collapsed, dummy `▁` prefix, spaces mapped to `▁`) with Viterbi best-path
/// segmentation; unmatchable characters use `<0xHH>` byte pieces when
/// `byte_fallback` is set, otherwise the unknown id. No lowercasing.
#[derive(Debug, Clone)]
pub(crate) struct UnigramTokenizer {
    // Hashed lookups only (never iterated), so hashing changes no id.
    pieces: HashMap<String, (u32, f32)>,
    max_piece_chars: usize,
    unk_id: u32,
    bos_id: u32,
    eos_id: u32,
    byte_fallback: bool,
    /// `<0xHH>` piece id per byte value, resolved once at load so the
    /// per-character fallback never formats or looks up. Boxed: the
    /// table would otherwise bloat every `EncoderTokenizer` by 2KB.
    byte_ids: Box<[Option<u32>; 256]>,
}

impl UnigramTokenizer {
    pub(crate) fn from_tokenizer_json(source: &str) -> Result<Self, ProviderError> {
        let value: serde_json::Value = serde_json::from_str(source)
            .map_err(|error| invalid(format!("{TOKENIZER_FILE}: {error}")))?;
        let model = value
            .get("model")
            .ok_or_else(|| invalid(format!("{TOKENIZER_FILE}: missing `model`")))?;
        let kind = model.get("type").and_then(serde_json::Value::as_str);
        if kind != Some("Unigram") {
            return Err(invalid(format!(
                "{TOKENIZER_FILE}: unsupported tokenizer model type {kind:?}: \
                 expected the Unigram subset (multilingual-e5 style)"
            )));
        }
        let vocab = model
            .get("vocab")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| invalid(format!("{TOKENIZER_FILE}: missing `model.vocab`")))?;
        if vocab.is_empty() {
            return Err(invalid(format!("{TOKENIZER_FILE}: vocabulary is empty")));
        }
        let mut pieces = HashMap::new();
        let mut max_piece_chars = 0usize;
        for (index, entry) in vocab.iter().enumerate() {
            let pair = entry.as_array().ok_or_else(|| {
                invalid(format!(
                    "{TOKENIZER_FILE}: vocab entry {index} is not a `[piece, score]` pair"
                ))
            })?;
            let piece = pair
                .first()
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    invalid(format!(
                        "{TOKENIZER_FILE}: vocab entry {index} has no piece text"
                    ))
                })?;
            let score = pair
                .get(1)
                .and_then(serde_json::Value::as_f64)
                .ok_or_else(|| {
                    invalid(format!(
                        "{TOKENIZER_FILE}: vocab entry {index} has no score"
                    ))
                })? as f32;
            if !score.is_finite() {
                return Err(invalid(format!(
                    "{TOKENIZER_FILE}: vocab entry {index} has a non-finite score"
                )));
            }
            let id = u32::try_from(index)
                .map_err(|_| invalid(format!("{TOKENIZER_FILE}: vocabulary exceeds u32 ids")))?;
            max_piece_chars = max_piece_chars.max(piece.chars().count());
            pieces.insert(piece.to_string(), (id, score));
        }
        let vocab_len = pieces.len() as u32;
        let unk_id = model
            .get("unk_id")
            .and_then(serde_json::Value::as_u64)
            .and_then(|id| u32::try_from(id).ok())
            .filter(|id| *id < vocab_len)
            .ok_or_else(|| {
                invalid(format!(
                    "{TOKENIZER_FILE}: missing or invalid `model.unk_id`"
                ))
            })?;
        let byte_fallback = model
            .get("byte_fallback")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        let added: BTreeMap<String, u32> = value
            .get("added_tokens")
            .and_then(serde_json::Value::as_array)
            .map(|tokens| {
                tokens
                    .iter()
                    .filter_map(|token| {
                        let content = token.get("content")?.as_str()?;
                        let id = token
                            .get("id")?
                            .as_u64()
                            .and_then(|id| u32::try_from(id).ok())?;
                        (id < vocab_len).then(|| (content.to_string(), id))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let bos_id = special_id(&added, &pieces, "<s>")?;
        let eos_id = special_id(&added, &pieces, "</s>")?;
        // Forwards run unpadded, but keep rejecting pad-less checkpoints.
        special_id(&added, &pieces, "<pad>")?;
        let mut byte_ids = Box::new([None; 256]);
        if byte_fallback {
            for (byte, slot) in byte_ids.iter_mut().enumerate() {
                let key = format!("<0x{byte:02X}>");
                *slot = pieces.get(&key).map(|(id, _)| *id);
            }
        }
        Ok(Self {
            pieces,
            max_piece_chars: max_piece_chars.max(1),
            unk_id,
            bos_id,
            eos_id,
            byte_fallback,
            byte_ids,
        })
    }

    pub(crate) fn vocab_len(&self) -> usize {
        self.pieces.len()
    }

    /// Ids for one unmatchable character: `<0xHH>` byte pieces when enabled
    /// (and present), otherwise the unknown id.
    fn fallback_ids(&self, ch: char) -> Vec<u32> {
        if self.byte_fallback {
            let mut encoded = [0u8; 4];
            let bytes = ch.encode_utf8(&mut encoded);
            let mut ids = Vec::with_capacity(bytes.len());
            for byte in bytes.as_bytes() {
                match self.byte_ids[*byte as usize] {
                    Some(id) => ids.push(id),
                    None => return vec![self.unk_id],
                }
            }
            return ids;
        }
        vec![self.unk_id]
    }

    /// Viterbi best-path segmentation of SentencePiece-framed text.
    /// Candidates extend one reusable buffer per start (same attempts, same
    /// order, one allocation instead of one per end), and the fallback path
    /// compares its score before building ids — both rewrite the same
    /// best/back entries as before.
    fn segment(&self, text: &str) -> Vec<u32> {
        let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut framed = String::with_capacity(collapsed.len() + 1);
        framed.push(SPIECE_UNDERLINE);
        for ch in collapsed.chars() {
            framed.push(if ch == ' ' { SPIECE_UNDERLINE } else { ch });
        }
        let chars: Vec<char> = framed.chars().collect();
        let len = chars.len();
        let mut best = vec![f32::NEG_INFINITY; len + 1];
        let mut back: Vec<Option<(usize, Vec<u32>)>> = vec![None; len + 1];
        best[0] = 0.0;
        let mut piece = String::new();
        for start in 0..len {
            if best[start].is_infinite() {
                continue;
            }
            let limit = (start + self.max_piece_chars).min(len);
            piece.clear();
            for end in start + 1..=limit {
                piece.push(chars[end - 1]);
                if let Some((id, score)) = self.pieces.get(piece.as_str()) {
                    let candidate = best[start] + score;
                    if candidate > best[end] {
                        best[end] = candidate;
                        back[end] = Some((start, vec![*id]));
                    }
                }
            }
            let candidate = best[start] + FALLBACK_SCORE;
            if candidate > best[start + 1] {
                best[start + 1] = candidate;
                back[start + 1] = Some((start, self.fallback_ids(chars[start])));
            }
        }
        let mut steps = Vec::new();
        let mut cursor = len;
        while cursor > 0 {
            match back[cursor].take() {
                Some((prev, piece_ids)) => {
                    steps.push(piece_ids);
                    cursor = prev;
                }
                None => {
                    steps.push(vec![self.unk_id]);
                    break;
                }
            }
        }
        steps.reverse();
        steps.into_iter().flatten().collect()
    }

    /// Encode to `<s> pieces </s>`, truncating pieces to `max_len - 2`.
    /// Returns `(ids, truncated)`.
    pub(crate) fn encode(&self, text: &str, max_len: usize) -> (Vec<u32>, bool) {
        let capacity = max_len.saturating_sub(2).max(1);
        let mut pieces = self.segment(text);
        let truncated = pieces.len() > capacity;
        pieces.truncate(capacity);
        let mut ids = Vec::with_capacity(pieces.len() + 2);
        ids.push(self.bos_id);
        ids.extend(pieces);
        ids.push(self.eos_id);
        (ids, truncated)
    }
}

/// Resolve a required special token from `added_tokens`, falling back to a
/// vocabulary piece with the same text.
fn special_id(
    added: &BTreeMap<String, u32>,
    pieces: &HashMap<String, (u32, f32)>,
    name: &str,
) -> Result<u32, ProviderError> {
    added
        .get(name)
        .copied()
        .or_else(|| pieces.get(name).map(|(id, _)| *id))
        .ok_or_else(|| invalid(format!("{TOKENIZER_FILE}: missing required token `{name}`")))
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

    fn unigram_fixture() -> &'static str {
        r#"{
            "model": {
                "type": "Unigram",
                "unk_id": 3,
                "byte_fallback": false,
                "vocab": [
                    ["<s>", 0.0], ["<pad>", 0.0], ["</s>", 0.0], ["<unk>", 0.0],
                    ["▁", -1.0], ["▁hello", 5.0], ["▁world", 4.0],
                    ["he", 1.0], ["llo", 1.0], ["s", 0.5]
                ]
            },
            "added_tokens": [
                {"id": 0, "content": "<s>", "special": true},
                {"id": 1, "content": "<pad>", "special": true},
                {"id": 2, "content": "</s>", "special": true}
            ]
        }"#
    }

    #[test]
    fn unigram_prefers_best_path_and_wraps_bos_eos() {
        let tokenizer = UnigramTokenizer::from_tokenizer_json(unigram_fixture()).unwrap();
        assert_eq!(tokenizer.vocab_len(), 10);
        let (ids, truncated) = tokenizer.encode("hello world", 32);
        assert!(!truncated);
        // <s> ▁hello ▁world </s>: whole-word pieces beat he+llo on score.
        assert_eq!(ids, vec![0, 5, 6, 2], "got {ids:?}");
        let (ids, _) = tokenizer.encode("hellos", 32);
        assert_eq!(ids, vec![0, 5, 9, 2], "got {ids:?}");
    }

    #[test]
    fn unigram_falls_back_to_unk_and_truncates() {
        let tokenizer = UnigramTokenizer::from_tokenizer_json(unigram_fixture()).unwrap();
        let (ids, _) = tokenizer.encode("xyzzy", 32);
        assert!(ids.contains(&3), "unknown word maps to <unk>, got {ids:?}");
        let (_, truncated) = tokenizer.encode("hello ".repeat(100).as_str(), 8);
        assert!(truncated);
        let (empty, truncated) = tokenizer.encode("", 32);
        assert!(!truncated);
        assert_eq!(empty.first(), Some(&0));
        assert_eq!(empty.last(), Some(&2));
    }

    #[test]
    fn unigram_rejects_wrong_model_types_and_bad_shapes() {
        assert!(UnigramTokenizer::from_tokenizer_json("{}").is_err());
        assert!(
            UnigramTokenizer::from_tokenizer_json(
                r#"{"model": {"type": "BPE", "vocab": {}, "unk_id": 0}}"#
            )
            .is_err()
        );
        assert!(
            UnigramTokenizer::from_tokenizer_json(
                r#"{"model": {"type": "Unigram", "unk_id": 99, "vocab": [["a", 1.0]]}}"#
            )
            .is_err()
        );
        assert!(
            UnigramTokenizer::from_tokenizer_json(
                r#"{"model": {"type": "Unigram", "vocab": [["a", 1.0]]}}"#
            )
            .is_err()
        );
    }

    #[test]
    fn unigram_byte_fallback_covers_unmatchable_characters() {
        let source = r#"{
            "model": {
                "type": "Unigram",
                "unk_id": 3,
                "byte_fallback": true,
                "vocab": [
                    ["<s>", 0.0], ["<pad>", 0.0], ["</s>", 0.0], ["<unk>", 0.0],
                    ["▁", -1.0], ["<0xC3>", 0.0], ["<0xA9>", 0.0]
                ]
            },
            "added_tokens": [
                {"id": 0, "content": "<s>", "special": true},
                {"id": 1, "content": "<pad>", "special": true},
                {"id": 2, "content": "</s>", "special": true}
            ]
        }"#;
        let tokenizer = UnigramTokenizer::from_tokenizer_json(source).unwrap();
        // "é" is U+00E9 -> UTF-8 C3 A9, covered by byte pieces, not <unk>.
        let (ids, _) = tokenizer.encode("é", 32);
        assert_eq!(ids, vec![0, 4, 5, 6, 2], "got {ids:?}");
    }
}
