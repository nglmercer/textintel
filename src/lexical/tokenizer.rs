use crate::core::providers::LexiconProvider;
use crate::resources::DefaultLexiconProvider;
use unicode_properties::UnicodeEmoji;

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
    simple_lemmas_with_provider(tokens, None, &DefaultLexiconProvider)
}

pub fn simple_lemmas_with_provider(
    tokens: &[String],
    languages: Option<&[String]>,
    provider: &dyn LexiconProvider,
) -> Vec<String> {
    // Suffix char lengths computed once: the strip rule below used to
    // recount the token (and the suffix) per candidate suffix.
    let suffixes = [
        "ing", "ed", "es", "s", "mente", "cion", "ción", "ando", "iendo",
    ]
    .map(|suffix| (suffix, suffix.chars().count()));
    tokens
        .iter()
        .map(|token| {
            let mut value: String = token.chars().flat_map(char::to_lowercase).collect();
            if let Some(lemma) = provider.lemma(&value, languages) {
                return lemma;
            }
            if value.chars().all(char::is_alphabetic) {
                let value_len = value.chars().count();
                if value_len > 5 {
                    for (suffix, suffix_len) in suffixes {
                        if value.ends_with(suffix) && value_len - suffix_len >= 3 {
                            value.truncate(value.len() - suffix.len());
                            break;
                        }
                    }
                }
            }
            value
        })
        .collect()
}

pub fn stop_words(tokens: &[String]) -> Vec<String> {
    stop_words_with_provider(tokens, None, &DefaultLexiconProvider)
}

pub fn stop_words_with_provider(
    tokens: &[String],
    languages: Option<&[String]>,
    provider: &dyn LexiconProvider,
) -> Vec<String> {
    tokens
        .iter()
        .map(|token| {
            token
                .chars()
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|token| provider.is_stop_word(token, languages))
        .collect()
}

pub fn is_emoji(ch: char) -> bool {
    ch.is_emoji_char_or_emoji_component()
}
