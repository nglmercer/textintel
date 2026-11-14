use unicode_normalization::UnicodeNormalization;

pub fn nfc(text: &str) -> String {
    text.nfc().collect()
}

pub fn nfkc(text: &str) -> String {
    text.nfkc().collect()
}

use crate::core::unicode::is_generic_diacritic;

/// Strip generic combining diacritics (NFD, then drop combining marks).
/// Accents distinguish presentation, not word identity for similarity
/// purposes (`café` ≈ `cafe`, `niño` ≈ `nino`): every accent-only pair in
/// the evaluation data is labelled similar, and no dissimilar pair
/// collides under this fold. Precomposed stroke letters without a
/// decomposition (`ł`, `ø`, `đ`) and script-specific marks are preserved.
/// Apply AFTER [`casefold_text`]: casefolding can emit a combining mark
/// (`İ` → `i` + dot) that belongs to the fold, not the text.
pub fn strip_diacritics(text: &str) -> String {
    // Fast path: NFD is the identity on ASCII and ASCII holds no
    // combining marks, so nothing is filtered.
    if text.is_ascii() {
        return text.to_string();
    }
    text.nfd().filter(|ch| !is_generic_diacritic(*ch)).collect()
}

/// Rust has no standard-library Unicode case-fold API. NFKC followed by
/// lowercase plus the small set of multi-scalar folds below gives a stable
/// Unicode-aware matching view; raw/NFC/NFKC views remain available too.
pub fn casefold_text(text: &str) -> String {
    // Fast path: NFKC is the identity on ASCII (no ASCII character has a
    // decomposition mapping) and the special folds below are all
    // non-ASCII, so ASCII casefolds to plain lowercase.
    if text.is_ascii() {
        return text.to_ascii_lowercase();
    }
    nfkc(text)
        .chars()
        .flat_map(|character| match character {
            'ß' | 'ẞ' => "ss".chars().collect::<Vec<_>>(),
            'ς' => vec!['σ'],
            'ſ' => vec!['s'],
            'İ' => vec!['i', '\u{307}'],
            character => character.to_lowercase().collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_diacritics_folds_accents_only() {
        assert_eq!(strip_diacritics("música"), "musica");
        assert_eq!(strip_diacritics("niño"), "nino");
        assert_eq!(strip_diacritics("über"), "uber");
        assert_eq!(strip_diacritics("dzień"), "dzien");
        // Casefolding `İ` emits a combining dot; stripping completes the fold.
        assert_eq!(strip_diacritics(&casefold_text("İ")), "i");
        // Stroke letters have no decomposition and survive.
        assert_eq!(strip_diacritics("łódź"), "łodz");
        // Script-specific marks (Arabic harakat here) survive.
        assert_eq!(strip_diacritics("مَدرسة"), "مَدرسة");
    }
}
