use crate::core::error::ProviderError;
use crate::core::providers::G2PProvider as G2PProviderTrait;
use crate::core::types::PhoneticCandidate;
use crate::normalization::unicode::casefold_text;

pub use crate::core::providers::G2PProvider;

#[derive(Debug, Default, Clone, Copy)]
pub struct NullG2PProvider;

impl G2PProviderTrait for NullG2PProvider {
    fn phonemize(&self, text: &str, language: &str) -> Result<PhoneticCandidate, ProviderError> {
        Ok(PhoneticCandidate {
            source: text.to_string(),
            language: language.to_string(),
            dialect: None,
            ipa: None,
            phonemes: Vec::new(),
            stress: None,
            syllables: 0,
            articulatory_features: Vec::new(),
            confidence: 0.0,
        })
    }
}

/// Small, deterministic fallback G2P implementation.  It is intentionally
/// conservative and acts as an evidence channel; applications needing full
/// pronunciation coverage can inject espeak, Epitran, or a model provider.
#[derive(Debug, Default, Clone, Copy)]
pub struct RuleBasedG2PProvider;

fn emit_word(word: &str, language: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut output = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        let next = chars.get(index + 1).copied();
        let pair = match (ch, next) {
            ('c', Some('h')) => Some("tʃ"),
            ('l', Some('l')) if language == "es" => Some("ʝ"),
            ('r', Some('r')) => Some("r"),
            ('q', Some('u')) => Some("k"),
            ('g', Some('u'))
                if chars.get(index + 2) == Some(&'e') || chars.get(index + 2) == Some(&'i') =>
            {
                Some("g")
            }
            _ => None,
        };
        if let Some(value) = pair {
            output.push(value.to_string());
            index += 2;
            continue;
        }
        let phoneme = match ch {
            'a' => "a",
            'e' => "e",
            'i' | 'y' if language == "es" => "i",
            'i' | 'y' => "ɪ",
            'o' => "o",
            'u' => "u",
            'b' | 'v' => "b",
            'c' if next.is_some_and(|value| matches!(value, 'e' | 'i')) && language == "es" => "s",
            'c' => "k",
            'd' => "d",
            'f' => "f",
            'g' if next.is_some_and(|value| matches!(value, 'e' | 'i')) && language == "es" => "x",
            'g' => "g",
            'h' if language == "es" => "",
            'j' if language == "es" => "x",
            'j' => "dʒ",
            'k' => "k",
            'l' => "l",
            'm' => "m",
            'n' => "n",
            'ñ' => "ɲ",
            'p' => "p",
            'r' => "ɾ",
            's' | 'z' => "s",
            't' => "t",
            'w' => "w",
            'x' => "ks",
            _ if ch.is_alphabetic() => "",
            _ => "",
        };
        if !phoneme.is_empty() {
            output.push(phoneme.to_string());
        }
        index += 1;
    }
    output
}

impl G2PProviderTrait for RuleBasedG2PProvider {
    fn phonemize(&self, text: &str, language: &str) -> Result<PhoneticCandidate, ProviderError> {
        let folded = casefold_text(text);
        let mut phonemes = Vec::new();
        for word in folded.split(|ch: char| !ch.is_alphabetic()) {
            if !word.is_empty() {
                phonemes.extend(emit_word(word, language));
            }
        }
        let ipa = if phonemes.is_empty() {
            None
        } else {
            Some(phonemes.join(""))
        };
        Ok(PhoneticCandidate {
            source: text.to_string(),
            language: language.to_string(),
            dialect: None,
            ipa,
            syllables: phonemes
                .iter()
                .filter(|phoneme| phoneme.chars().any(|ch| "aeiouəɛɪɔʊ".contains(ch)))
                .count(),
            stress: None,
            articulatory_features: phonemes
                .iter()
                .map(|phoneme| crate::phonetic::features::feature_label(phoneme).to_string())
                .collect(),
            phonemes,
            confidence: if text.is_empty() { 0.0 } else { 0.65 },
        })
    }
}
