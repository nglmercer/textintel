//! URL entity scanning (`http(s)://` and `www.` spans).

use crate::core::types::EntityMention;

use super::normalize::{casefold, claim, is_claimed, push_mention, trim_trailing_punctuation};

pub(crate) fn scan_urls(
    text: &str,
    claimed: &mut [bool],
    mentions: &mut Vec<EntityMention>,
    language: Option<&str>,
) {
    for (offset, _) in text.char_indices() {
        if claimed[offset] {
            continue;
        }
        let rest = &text[offset..];
        let prefix_matches = |prefix: &str| {
            rest.get(..prefix.len())
                .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
        };
        let has_scheme = prefix_matches("http://") || prefix_matches("https://");
        let has_www = prefix_matches("www.") && rest.len() > 4;
        if !has_scheme && !has_www {
            continue;
        }
        let mut end = offset;
        for (index, ch) in rest.char_indices() {
            if ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\'' | '`' | '«' | '»') {
                break;
            }
            end = offset + index + ch.len_utf8();
        }
        if end <= offset {
            continue;
        }
        let mut raw = &text[offset..end];
        let trimmed = trim_trailing_punctuation(raw);
        if trimmed.is_empty() {
            continue;
        }
        end = offset + trimmed.len();
        raw = trimmed;
        if has_www && !raw.contains('.') {
            continue;
        }
        if is_claimed(claimed, offset, end) {
            continue;
        }
        claim(claimed, offset, end);
        push_mention(mentions, "url", casefold(raw), offset, end, 0.95, language);
    }
}
