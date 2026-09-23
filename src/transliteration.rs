//! Rule-based script transliteration (`privet ↔ привет`, …).
//!
//! [`RuleBasedTransliterationProvider`] converts Cyrillic, Arabic, (a small
//! table of) Simplified Chinese, Japanese kana, and Devanagari to Latin, plus
//! Latin back to Cyrillic, Arabic, and Han. It is a deterministic `Basic`
//! fallback: per-character maps plus a few digraphs and word entries, with
//! unknown characters passed through unchanged. Views are stored as
//! additional `transliteration:<script>` fingerprint views and participate in
//! decoded-overlap comparison; [`MessageFingerprint::raw`](crate::core::types::MessageFingerprint)
//! is never replaced.
//!
//! Known limits (documented, not hidden): Arabic short vowels are unwritten,
//! so `سلام` folds to `slam` (the reverse direction still links `salam` →
//! `سلام`); the Han table covers common characters only; Latin→Cyrillic is
//! Russian-biased; kana→Latin is Hepburn without foreign-word contractions
//! (`ファ` folds to `fua`); Devanagari→Latin deletes word-final schwa
//! (Hindi `कमल` → `kamal`) but keeps medial schwas, and folds long vowels
//! to short, Hunterian-style (`काल` and `कल` share `kal`); only the →Latin
//! direction exists for kana and Devanagari.

use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::providers::{Transliteration, TransliterationProvider};

/// Deterministic rule-based transliteration for Latn/Cyrl/Arab/Hans.
#[derive(Debug, Default, Clone, Copy)]
pub struct RuleBasedTransliterationProvider;

/// First-in-table digraph match without allocation. `table` keys must
/// be ASCII-only and the compared window must be ASCII (checked by the
/// caller): ASCII lowering is byte-exact (`b | 32`), so byte comparison
/// equals the legacy lowercase-the-suffix-then-`starts_with` check.
/// Returns the matched table index. Non-ASCII windows fall back to the
/// legacy path (lowering can mint ASCII from e.g. Kelvin sign).
pub(crate) fn ascii_table_match<T>(
    chars: &[char],
    index: usize,
    table: &[(&str, T)],
) -> Option<usize> {
    table.iter().position(|(pattern, _)| {
        let bytes = pattern.as_bytes();
        chars.len() - index >= bytes.len()
            && bytes
                .iter()
                .enumerate()
                .all(|(offset, expected)| (chars[index + offset] as u8 | 32) == *expected)
    })
}

/// Longest key in a digraph table: the ASCII-window check covers this
/// many upcoming chars, so every compared char is proven ASCII.
pub(crate) fn table_key_len<T>(table: &[(&str, T)]) -> usize {
    table.iter().map(|(key, _)| key.len()).max().unwrap_or(0)
}

/// Casefolded, whitespace-stripped comparison key for transliteration
/// views (see [`RuleBasedTransliterationProvider::views_for`]).
fn casefold_compact(text: &str) -> String {
    use crate::normalization::unicode::casefold_text;
    casefold_text(text)
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

impl RuleBasedTransliterationProvider {
    fn views_for(text: &str) -> Vec<Transliteration> {
        // The input key folds lazily: texts with no matching script (or
        // only unchanged outputs) never pay for either fold.
        let mut compact_input: Option<String> = None;
        let mut views = Vec::new();
        let mut push = |views: &mut Vec<Transliteration>,
                        output: String,
                        script: &str,
                        language: Option<&str>,
                        confidence: f64| {
            // Views must carry new information: outputs equal modulo
            // case/whitespace (e.g. pass-through CJK with inserted syllable
            // spaces) are dropped instead of stored. Cheap rejections run
            // before either fold.
            if output == text || output.is_empty() {
                return;
            }
            let input = compact_input.get_or_insert_with(|| casefold_compact(text));
            if casefold_compact(&output) != *input {
                views.push(Transliteration::new(
                    output,
                    script,
                    language.map(str::to_string),
                    confidence,
                ));
            }
        };
        if contains_script(text, Script::Cyrillic) {
            push(&mut views, cyrillic_to_latin(text), "Latn", Some("ru"), 0.6);
        }
        if contains_script(text, Script::Arabic) {
            push(&mut views, arabic_to_latin(text), "Latn", Some("ar"), 0.6);
        }
        if contains_script(text, Script::Han) {
            push(&mut views, han_to_latin(text), "Latn", Some("zh"), 0.5);
        }
        if contains_script(text, Script::Kana) {
            push(&mut views, kana_to_latin(text), "Latn", Some("ja"), 0.6);
        }
        if contains_script(text, Script::Devanagari) {
            push(
                &mut views,
                devanagari_to_latin(text),
                "Latn",
                Some("hi"),
                0.5,
            );
        }
        if latin_heavy(text) {
            push(&mut views, latin_to_cyrillic(text), "Cyrl", Some("ru"), 0.6);
            push(&mut views, latin_to_arabic(text), "Arab", Some("ar"), 0.55);
            push(&mut views, latin_to_han(text), "Hans", Some("zh"), 0.5);
        }
        // One view per target script: keep the first (highest confidence).
        let mut seen = std::collections::BTreeSet::new();
        views.retain(|view| seen.insert(view.target_script.clone()));
        views
    }
}

impl TransliterationProvider for RuleBasedTransliterationProvider {
    fn transliterate(&self, text: &str) -> Vec<Transliteration> {
        Self::views_for(text)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("rule_based_transliteration")
            .with_languages(["ru", "ar", "zh", "ja", "hi", "und"])
            .with_quality(CapabilityLevel::Basic)
            .with_fallback(
                "per-character rule tables; prefer a trained transliterator for production",
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    Cyrillic,
    Arabic,
    Han,
    /// Hiragana and katakana share one Hepburn table.
    Kana,
    Devanagari,
}

fn contains_script(text: &str, script: Script) -> bool {
    text.chars().any(|ch| match script {
        Script::Cyrillic => matches!(ch, '\u{400}'..='\u{52f}'),
        Script::Arabic => {
            matches!(ch, '\u{600}'..='\u{6ff}' | '\u{750}'..='\u{77f}' | '\u{8a0}'..='\u{8ff}')
        }
        Script::Han => matches!(ch, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}'),
        Script::Kana => matches!(ch, '\u{3040}'..='\u{309f}' | '\u{30a0}'..='\u{30ff}'),
        Script::Devanagari => matches!(ch, '\u{900}'..='\u{97f}'),
    })
}

fn latin_heavy(text: &str) -> bool {
    let mut latin = 0usize;
    let mut letters = 0usize;
    for ch in text.chars() {
        if ch.is_alphabetic() {
            letters += 1;
            if ch.is_ascii_alphabetic() {
                latin += 1;
            }
        }
    }
    latin >= 2 && latin * 2 >= letters
}

/// Vowel skeleton of an already-folded string: ASCII vowels plus `y`
/// removed. Semitic scripts do not write short vowels, so a consonantal
/// view (`hbyby`) only meets its vocalized form (`habibi`) with vowels
/// abstracted away (`hbb`); `y` folds too because it vocalizes as `i` in
/// romanizations (`hbyby` vs `habibi`). Non-Latin text passes through
/// untouched, so the fold only ever links a Latin view with Latin text.
pub(crate) fn strip_latin_vowels(folded: &str) -> String {
    folded
        .chars()
        .filter(|ch| !matches!(ch, 'a' | 'e' | 'i' | 'o' | 'u' | 'y'))
        .collect()
}

/// Whether a folded view is truly consonantal (no written vowels): only
/// such views meet text as skeletons. A view that already spells its
/// vowels (`apple`, Cyrillic-derived) is complete — folding it would
/// destroy real vowel information and merge distinct words (`apple` with
/// `apply`). `y` does not count: it vocalizes as `i` in romanizations.
pub(crate) fn view_is_consonantal(folded_view: &str) -> bool {
    !folded_view
        .chars()
        .any(|ch| matches!(ch, 'a' | 'e' | 'i' | 'o' | 'u'))
}

mod arabic;
mod cyrillic;
mod devanagari;
mod evidence;
mod han;
mod kana;

use arabic::{arabic_to_latin, latin_to_arabic};
use cyrillic::{cyrillic_to_latin, latin_to_cyrillic};
use devanagari::devanagari_to_latin;
use han::{han_to_latin, latin_to_han};
use kana::kana_to_latin;

pub(crate) use evidence::transliteration_evidence_with_raw;
pub use evidence::{
    TransliterationEvidence, effective_transliteration_evidence, transliteration_compatibility,
    transliteration_evidence, transliteration_similarity,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn latin_views(text: &str) -> Vec<String> {
        RuleBasedTransliterationProvider
            .transliterate(text)
            .into_iter()
            .filter(|view| view.target_script == "Latn")
            .map(|view| view.text)
            .collect()
    }

    #[test]
    fn vowel_skeletons_link_semitic_views() {
        assert_eq!(strip_latin_vowels("hbyby"), "hbb");
        assert_eq!(strip_latin_vowels("habibi"), "hbb");
        assert_eq!(strip_latin_vowels("lyl"), "ll");
        assert_eq!(strip_latin_vowels("layl"), "ll");
        assert_eq!(strip_latin_vowels("net"), "nt");
        assert_eq!(strip_latin_vowels("niet"), "nt");
        // Non-Latin text passes through; empties stay empty.
        assert_eq!(strip_latin_vowels("حبيبي"), "حبيبي");
        assert_eq!(strip_latin_vowels("ai"), "");
        // Only consonantal views fold: complete views keep their vowels.
        assert!(view_is_consonantal("hbyby"));
        assert!(view_is_consonantal("lyl"));
        assert!(!view_is_consonantal("apple"));
        assert!(!view_is_consonantal("net"));
    }

    #[test]
    fn views_emit_only_for_covered_scripts() {
        assert!(
            RuleBasedTransliterationProvider
                .transliterate("hello world")
                .iter()
                .any(|view| view.target_script == "Cyrl")
        );
        // Pure ASCII gets no Latin view (nothing changed).
        assert!(latin_views("hello world").is_empty());
        // Emoji-only input yields no views rather than junk.
        assert!(
            RuleBasedTransliterationProvider
                .transliterate("🏠🔥")
                .is_empty()
        );
    }

    #[test]
    fn unknown_characters_pass_through() {
        // `e` has no single Arabic letter; tables document approximations
        // and never drop input silently.
        assert!(!latin_to_arabic("hello").is_empty());
        assert_eq!(cyrillic_to_latin("привет!"), "privet!");
    }
}
