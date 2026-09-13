use unicode_normalization::UnicodeNormalization;

pub fn nfc(text: &str) -> String {
    text.nfc().collect()
}

pub fn nfkc(text: &str) -> String {
    text.nfkc().collect()
}

/// Rust has no standard-library Unicode case-fold API.  Lowercasing after NFC
/// gives a deterministic, Unicode-aware approximation suitable for matching;
/// the original input and NFC/NFKC views remain available separately.
pub fn casefold_text(text: &str) -> String {
    nfc(text).chars().flat_map(char::to_lowercase).collect()
}
