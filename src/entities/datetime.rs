//! Date and time entity scanning (ISO, slashed, and `HH:MM` forms).

use crate::core::types::EntityMention;

use super::normalize::{claim, is_claimed, push_mention};

fn valid_date(year: u32, month: u32, day: u32) -> bool {
    (1..=12).contains(&month) && (1..=31).contains(&day) && (1000..=2999).contains(&year)
}

fn valid_time(hour: u32, minute: u32, second: Option<u32>) -> bool {
    hour <= 23 && minute <= 59 && second.map_or(true, |value| value <= 59)
}

pub(crate) fn scan_dates_times(
    text: &str,
    claimed: &mut [bool],
    mentions: &mut Vec<EntityMention>,
    language: Option<&str>,
) {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
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
            if valid_time(hour, minute, second) && !is_claimed(claimed, index, end) {
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
                        && (2..=4).contains(&third.len())
                        && first
                            .parse::<u32>()
                            .is_ok_and(|value| (1..=31).contains(&value))
                        && second
                            .parse::<u32>()
                            .is_ok_and(|value| (1..=31).contains(&value));
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
