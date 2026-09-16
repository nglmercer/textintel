//! Social-handle mention scanning (`@handle` spans).

use crate::core::types::EntityMention;

use super::normalize::{casefold, claim, is_claimed, push_mention};

fn mention_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.')
}

pub(crate) fn scan_mentions(
    text: &str,
    claimed: &mut [bool],
    mentions: &mut Vec<EntityMention>,
    language: Option<&str>,
) {
    for (offset, ch) in text.char_indices() {
        if ch != '@' || claimed[offset] {
            continue;
        }
        let prev_ok = offset == 0
            || text[..offset].chars().next_back().is_some_and(|prev| {
                prev.is_whitespace() || matches!(prev, '(' | '[' | '{' | '"' | '\'')
            });
        if !prev_ok {
            continue;
        }
        let mut end = offset + 1;
        let mut count = 0;
        for next in text[offset + 1..].chars() {
            if !mention_char(next) || count >= 30 {
                break;
            }
            end += next.len_utf8();
            count += 1;
        }
        if count == 0 {
            continue;
        }
        // Avoid the domain half of a skipped email (`user@host` where the
        // email scan failed validation): a mention needs a boundary before
        // `@`, which the check above already enforces.
        if is_claimed(claimed, offset, end) {
            continue;
        }
        claim(claimed, offset, end);
        push_mention(
            mentions,
            "mention",
            casefold(&text[offset..end]),
            offset,
            end,
            0.7,
            language,
        );
    }
}
