//! Standalone number entity scanning (digit runs not glued to letters).

use crate::core::types::EntityMention;

use super::currency::normalize_amount;
use super::normalize::{claim, is_claimed, push_mention};

pub(crate) fn scan_numbers(
    text: &str,
    claimed: &mut [bool],
    mentions: &mut Vec<EntityMention>,
    language: Option<&str>,
) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut index = 0;
    while index < chars.len() {
        let (offset, ch) = chars[index];
        if !ch.is_ascii_digit() || claimed[offset] {
            index += 1;
            continue;
        }
        let mut end = offset + ch.len_utf8();
        let mut cursor = index + 1;
        let mut digits = 1;
        while cursor < chars.len() {
            let (_, next) = chars[cursor];
            if next.is_ascii_digit() {
                digits += 1;
                end += next.len_utf8();
                cursor += 1;
            } else if matches!(next, '.' | ',')
                && cursor + 1 < chars.len()
                && chars[cursor + 1].1.is_ascii_digit()
            {
                end += next.len_utf8();
                cursor += 1;
            } else {
                break;
            }
        }
        // Standalone digit runs only: skip digits glued to letters (`abc123`
        // is a token, not a number entity).
        let glued_left = offset > 0
            && text[..offset]
                .chars()
                .next_back()
                .is_some_and(|prev| prev.is_alphanumeric());
        let glued_right = text[end..]
            .chars()
            .next()
            .is_some_and(|next| next.is_alphanumeric() && !next.is_ascii_digit());
        if digits > 0 && !glued_left && !glued_right && !is_claimed(claimed, offset, end) {
            claim(claimed, offset, end);
            push_mention(
                mentions,
                "number",
                normalize_amount(&text[offset..end]),
                offset,
                end,
                0.75,
                language,
            );
        }
        index = cursor.max(index + 1);
    }
}
