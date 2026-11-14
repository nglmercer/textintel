use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::G2PProvider as G2PProviderTrait;
use crate::core::types::PhoneticCandidate;
use crate::normalization::unicode::casefold_text;
use crate::visual::scripts::scripts_in;

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

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("null_g2p").with_quality(CapabilityLevel::Unavailable)
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
        // Accented vowels map to their base vowel: diacritics mark stress
        // or vowel quality, never a distinct phoneme for similarity
        // purposes (`música` must phonemize like `musica`). Consonant
        // diacritics keep their own rules (`ñ` → `ɲ` below).
        let phoneme = match ch {
            'a' | 'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' => "a",
            'e' | 'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' => "e",
            'i' | 'y' | 'í' | 'ì' | 'î' | 'ï' | 'ĩ' | 'ī' | 'į' | 'ý' | 'ÿ' if language == "es" => {
                "i"
            }
            'i' | 'y' | 'í' | 'ì' | 'î' | 'ï' | 'ĩ' | 'ī' | 'į' | 'ý' | 'ÿ' => "ɪ",
            'o' | 'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' | 'ő' | 'œ' => "o",
            'u' | 'ú' | 'ù' | 'û' | 'ü' | 'ũ' | 'ū' | 'ů' | 'ű' | 'ų' => "u",
            'b' | 'v' => "b",
            'c' if next.is_some_and(|value| matches!(value, 'e' | 'i')) && language == "es" => "s",
            'c' => "k",
            // Cedilla: /s/ in French/Portuguese/Catalan, /tʃ/ in Turkish.
            'ç' if language == "tr" => "tʃ",
            'ç' => "s",
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

/// Phoneme sequence for already-folded text, shared by the full and
/// lean paths so they can never disagree.
fn phonemes_for(folded: &str, language: &str) -> Vec<String> {
    let mut phonemes = Vec::new();
    for word in folded.split(|ch: char| !ch.is_alphabetic()) {
        if !word.is_empty() {
            phonemes.extend(emit_word(word, language));
        }
    }
    phonemes
}

/// The advertised rules are base-language rules, including regional callers
/// using that language. This selects the rule family, not a dialect model.
fn supported_rule_language(language: &str) -> Option<&'static str> {
    let base = super::language_family(language);
    if base.eq_ignore_ascii_case("es") {
        Some("es")
    } else if base.eq_ignore_ascii_case("en") {
        Some("en")
    } else {
        None
    }
}

/// Confidence describes the requested rule coverage as well as input script.
/// Unsupported languages retain the generic heuristic result for compatibility,
/// with confidence capped at the unsupported-script level.
fn confidence_for(folded: &str, phonemes: &[String], language: &str) -> f64 {
    if phonemes.is_empty() {
        return 0.0;
    }
    if supported_rule_language(language).is_none() {
        return 0.15;
    }
    let scripts = scripts_in(folded);
    let latin = scripts.iter().any(|script| script == "Latin");
    let other = scripts.iter().any(|script| script != "Latin");
    if latin && !other {
        0.65
    } else if latin {
        0.35
    } else {
        0.15
    }
}

impl G2PProviderTrait for RuleBasedG2PProvider {
    fn phonemize(&self, text: &str, language: &str) -> Result<PhoneticCandidate, ProviderError> {
        let folded = casefold_text(text);
        let rule_language = supported_rule_language(language).unwrap_or(language);
        let phonemes = phonemes_for(&folded, rule_language);
        let ipa = if phonemes.is_empty() {
            None
        } else {
            Some(phonemes.join(""))
        };
        let confidence = confidence_for(&folded, &phonemes, language);
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
            confidence,
        })
    }

    fn phonemes(&self, text: &str, language: &str) -> Result<(Vec<String>, f64), ProviderError> {
        // Same phonemes and confidence as `phonemize` (shared helpers),
        // skipping the IPA join, syllable scan, and per-phoneme feature
        // labels that scoring never reads.
        let folded = casefold_text(text);
        let rule_language = supported_rule_language(language).unwrap_or(language);
        let phonemes = phonemes_for(&folded, rule_language);
        let confidence = confidence_for(&folded, &phonemes, language);
        Ok((phonemes, confidence))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("rule_based_g2p")
            .with_languages(["es", "en"])
            .with_quality(CapabilityLevel::Basic)
            .with_fallback("English/Spanish Latin-script rules; other languages and scripts retain low-confidence heuristic output")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accented_vowels_phonemize_as_base_vowels() {
        assert_eq!(emit_word("música", "es"), emit_word("musica", "es"));
        assert_eq!(emit_word("über", "en"), emit_word("uber", "en"));
        assert_eq!(
            emit_word("façade", "fr"),
            vec!["f", "a", "s", "a", "d", "e"]
        );
        // Consonant diacritics keep their own rules.
        assert!(emit_word("niño", "es").contains(&"ɲ".to_string()));
    }
}
