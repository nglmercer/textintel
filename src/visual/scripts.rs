use unicode_properties::UnicodeEmoji;
use unicode_script::{Script, UnicodeScript};

/// Best-effort script classification for the Unicode ranges most useful to
/// text-spoof analysis. Unicode Script properties are used instead of a
/// hand-maintained range table. Common punctuation, numbers, and symbols are
/// kept separate so they do not make a message look multilingual.
pub fn script_name(ch: char) -> Option<&'static str> {
    if ch.is_whitespace() || ch.is_ascii_digit() {
        return None;
    }
    if is_symbol_character(ch) {
        return Some("Symbol");
    }
    if ch.is_ascii_punctuation() {
        return None;
    }
    if ch.is_numeric() {
        return Some("Common");
    }
    let script = ch.script();
    if script == Script::Common
        && (ch.is_emoji_char_or_emoji_component() || matches!(ch, '❤' | '⭐' | '☀' | '☁' | '☂'))
    {
        Some("Symbol")
    } else {
        Some(script.full_name())
    }
}

fn is_symbol_character(ch: char) -> bool {
    matches!(
        ch,
        '$' | '+'
            | '='
            | '%'
            | '€'
            | '£'
            | '¥'
            | '₽'
            | '₹'
            | '₩'
            | '₿'
            | '×'
            | '÷'
            | '±'
            | '∞'
            | '≈'
            | '≠'
            | '≤'
            | '≥'
            | '∑'
            | '√'
            | '∫'
    )
}

pub fn scripts_in(text: &str) -> Vec<String> {
    let mut scripts = std::collections::BTreeSet::new();
    for ch in text.chars() {
        if let Some(script) = script_name(ch)
            && !matches!(script, "Common" | "Inherited" | "Symbol")
        {
            scripts.insert(script.to_string());
        }
    }
    scripts.into_iter().collect()
}

pub fn script_extensions_for(ch: char) -> Vec<String> {
    ch.script_extension()
        .iter()
        .map(|script| script.full_name().to_string())
        .collect()
}
