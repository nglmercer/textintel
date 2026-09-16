use unicode_normalization::UnicodeNormalization;

pub fn nfc(text: &str) -> String {
    text.nfc().collect()
}

pub fn nfkc(text: &str) -> String {
    text.nfkc().collect()
}

/// A combining mark in the generic combining-diacritics blocks (covers
/// Latin, Greek, Cyrillic, Turkish, Polish, Vietnamese, and other
/// accent/stacking marks). Script-specific marks (Arabic harakat, Hebrew
/// points, Indic matras, ...) are deliberately NOT matched: inside those
/// scripts the marks can carry lexical weight, so stripping stays
/// conservative.
fn is_generic_combining_mark(character: char) -> bool {
    matches!(
        character,
        '\u{0300}'..='\u{036f}'
            | '\u{1ab0}'..='\u{1aff}'
            | '\u{1dc0}'..='\u{1dff}'
            | '\u{20d0}'..='\u{20ff}'
            | '\u{fe20}'..='\u{fe2f}'
    )
}

/// Strip generic combining diacritics (NFD, then drop combining marks).
/// Accents distinguish presentation, not word identity for similarity
/// purposes (`café` ≈ `cafe`, `niño` ≈ `nino`): every accent-only pair in
/// the evaluation data is labelled similar, and no dissimilar pair
/// collides under this fold. Precomposed stroke letters without a
/// decomposition (`ł`, `ø`, `đ`) and script-specific marks are preserved.
/// Apply AFTER [`casefold_text`]: casefolding can emit a combining mark
/// (`İ` → `i` + dot) that belongs to the fold, not the text.
pub fn strip_diacritics(text: &str) -> String {
    text.nfd()
        .filter(|ch| !is_generic_combining_mark(*ch))
        .collect()
}

/// Rust has no standard-library Unicode case-fold API. NFKC followed by
/// lowercase plus the small set of multi-scalar folds below gives a stable
/// Unicode-aware matching view; raw/NFC/NFKC views remain available too.
pub fn casefold_text(text: &str) -> String {
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
