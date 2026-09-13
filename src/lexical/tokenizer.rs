/// Tokenize without losing emoji, Unicode words, URLs, or punctuation.
/// Segmentation is shared so byte offsets and token boundaries agree across
/// the fingerprint and rebus channels.
pub fn tokenize(text: &str) -> Vec<String> {
    crate::language::segmentation::segment_message(text, usize::MAX)
        .into_iter()
        .map(|segment| segment.text)
        .collect()
}

pub fn simple_lemmas(tokens: &[String]) -> Vec<String> {
    let suffixes = [
        "ing", "ed", "es", "s", "mente", "cion", "ción", "ando", "iendo",
    ];
    tokens
        .iter()
        .map(|token| {
            let mut value: String = token.chars().flat_map(char::to_lowercase).collect();
            if value.chars().all(char::is_alphabetic) && value.chars().count() > 5 {
                for suffix in suffixes {
                    if value.ends_with(suffix)
                        && value.chars().count() - suffix.chars().count() >= 3
                    {
                        value.truncate(value.len() - suffix.len());
                        break;
                    }
                }
            }
            value
        })
        .collect()
}

const STOP_WORDS: &[&str] = &[
    "a", "about", "and", "as", "at", "con", "de", "del", "do", "el", "en", "for", "in", "is", "it",
    "la", "las", "le", "les", "los", "of", "on", "or", "para", "por", "que", "the", "to", "un",
    "una", "with", "y",
];

pub fn stop_words(tokens: &[String]) -> Vec<String> {
    tokens
        .iter()
        .map(|token| {
            token
                .chars()
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|token| STOP_WORDS.contains(&token.as_str()))
        .collect()
}

pub fn is_emoji(ch: char) -> bool {
    let code = ch as u32;
    (0x1f000..=0x1faff).contains(&code)
        || (0x2600..=0x27bf).contains(&code)
        || matches!(ch, '❤' | '⭐')
}
