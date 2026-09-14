use unicode_normalization::UnicodeNormalization;

pub fn nfc(text: &str) -> String {
    text.nfc().collect()
}

pub fn nfkc(text: &str) -> String {
    text.nfkc().collect()
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
