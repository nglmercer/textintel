//! Standalone number entity scanning (digit runs not glued to letters).

use crate::core::numeric::scan_unsigned_decimal;
use crate::core::types::EntityMention;
use crate::core::unicode::{ascii_decimal_digit as digit, is_word_letter};

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
        if digit(ch).is_none() || claimed[offset] {
            index += 1;
            continue;
        }
        let end = scan_unsigned_decimal(text, offset).expect("decimal start checked above");
        let mut cursor = index + 1;
        while cursor < chars.len() && chars[cursor].0 < end {
            cursor += 1;
        }
        // Standalone digit runs only: skip digits glued to letters (`abc123`
        // is a token, not a number entity).
        let glued_left = offset > 0
            && text[..offset]
                .chars()
                .next_back()
                .is_some_and(|prev| prev.is_alphanumeric() || is_word_letter(prev));
        let glued_right = text[end..].chars().next().is_some_and(|next| {
            (next.is_alphanumeric() || is_word_letter(next)) && digit(next).is_none()
        });
        if !glued_left && !glued_right && !is_claimed(claimed, offset, end) {
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
