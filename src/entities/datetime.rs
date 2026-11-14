//! Date and time entity scanning (ISO, slashed, and `HH:MM` forms).

use crate::core::types::EntityMention;
use crate::core::unicode::{ascii_decimal_digit as digit, is_cjk, is_word_letter};

use super::normalize::{claim, is_claimed, push_mention};

fn valid_date(year: u32, month: u32, day: u32) -> bool {
    (1000..=2999).contains(&year) && crate::core::calendar::valid_date(year, month, day)
}

fn valid_time(hour: u32, minute: u32, second: Option<u32>) -> bool {
    hour <= 23 && minute <= 59 && second.is_none_or(|value| value <= 59)
}

fn identifier_character(ch: char) -> bool {
    ((ch.is_alphanumeric() || is_word_letter(ch)) && !is_cjk(ch)) || ch == '_'
}

fn token_end(text: &str, end: usize) -> bool {
    text[end..]
        .chars()
        .next()
        .is_none_or(|ch| !identifier_character(ch) && !matches!(ch, '/' | ':' | '-'))
}

pub(crate) fn scan_dates_times(
    text: &str,
    claimed: &mut [bool],
    mentions: &mut Vec<EntityMention>,
    language: Option<&str>,
) {
    if text.chars().any(|ch| !ch.is_ascii() && digit(ch).is_some()) {
        // Run the same strict shape/calendar scanner on digit values, then
        // restore original UTF-8 offsets. Non-digit spelling is untouched.
        let mut folded = String::with_capacity(text.len());
        let mut offsets = Vec::with_capacity(text.len() + 1);
        for (offset, ch) in text.char_indices() {
            if let Some(ascii) = digit(ch) {
                folded.push(ascii);
                offsets.push(offset);
            } else {
                folded.push(ch);
                offsets.extend((0..ch.len_utf8()).map(|byte| offset + byte));
            }
        }
        offsets.push(text.len());
        let mut folded_claimed: Vec<bool> = offsets
            .windows(2)
            .map(|span| is_claimed(claimed, span[0], span[1]))
            .collect();
        let mut found = Vec::new();
        scan_ascii_dates_times(&folded, &mut folded_claimed, &mut found, language);
        for mut mention in found {
            mention.start = offsets[mention.start];
            mention.end = offsets[mention.end];
            claim(claimed, mention.start, mention.end);
            mentions.push(mention);
        }
        return;
    }
    scan_ascii_dates_times(text, claimed, mentions, language);
}

fn scan_ascii_dates_times(
    text: &str,
    claimed: &mut [bool],
    mentions: &mut Vec<EntityMention>,
    language: Option<&str>,
) {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        // Do not recognize a valid suffix inside a longer identifier or a
        // malformed date/time (e.g. 12024-01-01 or 123:45).
        if !text.is_char_boundary(index)
            || text[..index]
                .chars()
                .next_back()
                .is_some_and(|ch| identifier_character(ch) || matches!(ch, '/' | ':' | '-'))
        {
            index += 1;
            continue;
        }
        // ISO date `YYYY-MM-DD` (ASCII digits and dashes: byte offsets are
        // char boundaries).
        if index + 10 <= bytes.len()
            && bytes[index..index + 4].iter().all(|b| b.is_ascii_digit())
            && bytes[index + 4] == b'-'
            && bytes[index + 5..index + 7]
                .iter()
                .all(|b| b.is_ascii_digit())
            && bytes[index + 7] == b'-'
            && bytes[index + 8..index + 10]
                .iter()
                .all(|b| b.is_ascii_digit())
            && text.is_char_boundary(index)
            && text.is_char_boundary(index + 10)
            && token_end(text, index + 10)
        {
            let year: u32 = text[index..index + 4].parse().unwrap_or(0);
            let month: u32 = text[index + 5..index + 7].parse().unwrap_or(0);
            let day: u32 = text[index + 8..index + 10].parse().unwrap_or(0);
            if valid_date(year, month, day) && !is_claimed(claimed, index, index + 10) {
                claim(claimed, index, index + 10);
                push_mention(
                    mentions,
                    "date",
                    text[index..index + 10].to_string(),
                    index,
                    index + 10,
                    0.8,
                    language,
                );
                index += 10;
                continue;
            }
        }
        // Time `HH:MM` with optional `:SS`.
        if index + 5 <= bytes.len()
            && bytes[index..index + 2].iter().all(|b| b.is_ascii_digit())
            && bytes[index + 2] == b':'
            && bytes[index + 3..index + 5]
                .iter()
                .all(|b| b.is_ascii_digit())
            && text.is_char_boundary(index)
            && text.is_char_boundary(index + 5)
        {
            let hour: u32 = text[index..index + 2].parse().unwrap_or(99);
            let minute: u32 = text[index + 3..index + 5].parse().unwrap_or(99);
            let mut end = index + 5;
            let mut second = None;
            if index + 8 <= bytes.len()
                && bytes[index + 5] == b':'
                && bytes[index + 6..index + 8]
                    .iter()
                    .all(|b| b.is_ascii_digit())
                && text.is_char_boundary(index + 8)
            {
                second = Some(text[index + 6..index + 8].parse().unwrap_or(99));
                end = index + 8;
            }
            if valid_time(hour, minute, second)
                && token_end(text, end)
                && !is_claimed(claimed, index, end)
            {
                claim(claimed, index, end);
                push_mention(
                    mentions,
                    "time",
                    text[index..end].to_string(),
                    index,
                    end,
                    0.7,
                    language,
                );
                index = end;
                continue;
            }
        }
        // Slashed date `D{1,2}/D{1,2}/D{2,4}` (day/month order ambiguous, so
        // lower confidence; still a reliable date-shaped span).
        if bytes[index].is_ascii_digit() {
            let mut cursor = index;
            while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                cursor += 1;
            }
            let first = &text[index..cursor];
            if cursor < bytes.len() && bytes[cursor] == b'/' {
                let second_start = cursor + 1;
                let mut second_end = second_start;
                while second_end < bytes.len() && bytes[second_end].is_ascii_digit() {
                    second_end += 1;
                }
                if second_end < bytes.len() && bytes[second_end] == b'/' {
                    let third_start = second_end + 1;
                    let mut third_end = third_start;
                    while third_end < bytes.len() && bytes[third_end].is_ascii_digit() {
                        third_end += 1;
                    }
                    let second = &text[second_start..second_end];
                    let third = &text[third_start..third_end];
                    let plausible = (1..=2).contains(&first.len())
                        && (1..=2).contains(&second.len())
                        && matches!(third.len(), 2 | 4)
                        && token_end(text, third_end)
                        && match (
                            first.parse::<u32>(),
                            second.parse::<u32>(),
                            third.parse::<u32>(),
                        ) {
                            (Ok(first), Ok(second), Ok(year)) => {
                                let year = if third.len() == 2 { 2000 + year } else { year };
                                valid_date(year, second, first) || valid_date(year, first, second)
                            }
                            _ => false,
                        };
                    if plausible
                        && text.is_char_boundary(index)
                        && text.is_char_boundary(third_end)
                        && !is_claimed(claimed, index, third_end)
                    {
                        claim(claimed, index, third_end);
                        push_mention(
                            mentions,
                            "date",
                            text[index..third_end].to_string(),
                            index,
                            third_end,
                            0.6,
                            language,
                        );
                        index = third_end;
                        continue;
                    }
                }
            }
        }
        index += 1;
    }
}
