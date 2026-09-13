/// Best-effort script classification for the Unicode ranges most useful to
/// text-spoof analysis.  Common punctuation, numbers, and symbols are kept
/// separate so they do not make a message look multilingual.
pub fn script_name(ch: char) -> Option<&'static str> {
    if ch.is_whitespace() || ch.is_ascii_punctuation() || ch.is_ascii_digit() {
        return None;
    }
    let code = ch as u32;
    if ch.is_numeric() && !(0x0660..=0x0669).contains(&code) {
        return Some("Common");
    }
    if (0x1f300..=0x1faff).contains(&code)
        || (0x2600..=0x27bf).contains(&code)
        || matches!(ch, '❤' | '⭐' | '☀' | '☁' | '☂')
    {
        return Some("Symbol");
    }
    match code {
        0x0000..=0x024f | 0x1e00..=0x1eff => Some("Latin"),
        0x0370..=0x03ff | 0x1f00..=0x1fff => Some("Greek"),
        0x0400..=0x052f | 0x2de0..=0x2dff | 0xa640..=0xa69f => Some("Cyrillic"),
        0x0590..=0x05ff => Some("Hebrew"),
        0x0600..=0x06ff | 0x0750..=0x077f | 0x08a0..=0x08ff => Some("Arabic"),
        0x0900..=0x097f => Some("Devanagari"),
        0x0e00..=0x0e7f => Some("Thai"),
        0x3040..=0x309f => Some("Hiragana"),
        0x30a0..=0x30ff => Some("Katakana"),
        0x3130..=0x318f | 0xac00..=0xd7af => Some("Hangul"),
        0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff => Some("Han"),
        _ if ch.is_alphabetic() => Some("Other"),
        _ => None,
    }
}

pub fn scripts_in(text: &str) -> Vec<String> {
    let mut scripts = std::collections::BTreeSet::new();
    for ch in text.chars() {
        if let Some(script) = script_name(ch) {
            if !matches!(script, "Common" | "Symbol") {
                scripts.insert(script.to_string());
            }
        }
    }
    scripts.into_iter().collect()
}
