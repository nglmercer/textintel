//! Small, deterministic IPA tokenizer.
//!
//! A G2P provider may return IPA as a display form while exposing canonical
//! phoneme tokens separately. This parser gives providers that only have an
//! IPA string a stable token representation without treating UTF-8 bytes as
//! phonemes. Unknown symbols are retained as one token so information is not
//! silently discarded.

const MULTI_CHAR_TOKENS: &[&str] = &[
    "t͡ʃ", "d͡ʒ", "tʃ", "dʒ", "aɪ", "aʊ", "eɪ", "ɔɪ", "ks", "ɡ", "ɲ", "ʝ",
];

const SKIP_MARKS: &[char] = &[
    ' ', '\t', '\n', '.', ',', '|', '/', 'ˈ', 'ˌ', ':', 'ː', 'ʲ', 'ʷ',
];

/// Tokenize a UTF-8 IPA string using longest-match-first rules.
pub fn parse_ipa(ipa: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut remaining = ipa;
    while !remaining.is_empty() {
        if let Some(ch) = remaining.chars().next()
            && (SKIP_MARKS.contains(&ch) || is_combining_diacritic(ch))
        {
            remaining = &remaining[ch.len_utf8()..];
            continue;
        }
        if let Some(token) = MULTI_CHAR_TOKENS
            .iter()
            .find(|token| remaining.starts_with(**token))
        {
            output.push((*token).to_string());
            remaining = &remaining[token.len()..];
            continue;
        }
        let Some(ch) = remaining.chars().next() else {
            break;
        };
        output.push(ch.to_string());
        remaining = &remaining[ch.len_utf8()..];
    }
    output
}

fn is_combining_diacritic(ch: char) -> bool {
    matches!(ch as u32, 0x300..=0x36f | 0x2de..=0x2ff | 0x1ab0..=0x1aff)
}
