//! Currency entity scanning (symbol-prefixed amounts and `100 USD` forms).

use crate::core::numeric::scan_unsigned_decimal;
use crate::core::types::EntityMention;
use crate::core::unicode::{ascii_decimal_digit as digit, is_word_letter};

use super::normalize::{claim, is_claimed, push_mention};

const CURRENCY_SYMBOLS: &[(char, &str)] = &[
    ('$', "USD"),
    ('€', "EUR"),
    ('£', "GBP"),
    ('₹', "INR"),
    ('₽', "RUB"),
    ('₩', "KRW"),
    ('¥', "JPY"),
    ('¢', "CENT"),
    ('₴', "UAH"),
    ('₺', "TRY"),
    ('₪', "ILS"),
    ('₫', "VND"),
    ('₱', "PHP"),
    ('฿', "THB"),
];

const CURRENCY_CODES: &[&str] = &[
    "USD", "EUR", "GBP", "JPY", "CNY", "INR", "RUB", "KRW", "CHF", "CAD", "AUD", "BRL", "MXN",
    "ARS", "CLP", "COP", "PEN", "SEK", "NOK", "DKK", "PLN", "CZK", "HUF", "RON", "TRY", "ZAR",
    "NGN", "KES", "GHS", "PHP", "IDR", "MYR", "THB", "VND", "TWD", "HKD", "SGD", "NZD", "ILS",
    "AED", "SAR", "QAR",
];

pub(crate) fn normalize_amount(raw: &str) -> String {
    raw.chars()
        .filter_map(|ch| {
            digit(ch).or(match ch {
                '.' | ',' => Some(ch),
                '٫' => Some('.'),
                '٬' => Some(','),
                _ => None,
            })
        })
        .collect()
}

pub(crate) fn scan_currency(
    text: &str,
    claimed: &mut [bool],
    mentions: &mut Vec<EntityMention>,
    language: Option<&str>,
) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut index = 0;
    while index < chars.len() {
        let (offset, ch) = chars[index];
        if claimed[offset] {
            index += 1;
            continue;
        }
        let code = CURRENCY_SYMBOLS.iter().find(|(symbol, _)| *symbol == ch);
        if let Some((_, code)) = code {
            let mut end = offset + ch.len_utf8();
            let mut cursor = index + 1;
            while cursor < chars.len() && chars[cursor].1 == ' ' {
                end += 1;
                cursor += 1;
            }
            if let Some(amount_end) = scan_unsigned_decimal(text, end) {
                end = amount_end;
                while cursor < chars.len() && chars[cursor].0 < end {
                    cursor += 1;
                }
                if !is_claimed(claimed, offset, end) {
                    claim(claimed, offset, end);
                    let amount = normalize_amount(&text[offset..end]);
                    push_mention(
                        mentions,
                        "currency",
                        format!("{code} {amount}"),
                        offset,
                        end,
                        0.85,
                        language,
                    );
                }
                index = cursor;
                continue;
            }
        }
        index += 1;
    }
    // Amount-first form (`100 USD`): decimal digits plus ASCII currency codes.
    let mut word_start: Option<usize> = None;
    let mut words: Vec<(usize, usize)> = Vec::new();
    for (offset, ch) in text.char_indices() {
        let numeric_separator = matches!(ch, '.' | ',' | '٫' | '٬')
            && text[..offset]
                .chars()
                .next_back()
                .is_some_and(|c| digit(c).is_some())
            && text[offset + ch.len_utf8()..]
                .chars()
                .next()
                .is_some_and(|c| digit(c).is_some());
        if ch.is_alphanumeric() || is_word_letter(ch) || numeric_separator {
            if word_start.is_none() {
                word_start = Some(offset);
            }
        } else if let Some(start) = word_start.take() {
            words.push((start, offset));
        }
    }
    if let Some(start) = word_start {
        words.push((start, text.len()));
    }
    for window in words.windows(2) {
        let (amount_start, amount_end) = window[0];
        let (code_start, code_end) = window[1];
        if is_claimed(claimed, amount_start, code_end) {
            continue;
        }
        let amount_raw = &text[amount_start..amount_end];
        let code_raw = &text[code_start..code_end];
        if scan_unsigned_decimal(amount_raw, 0) != Some(amount_raw.len()) {
            continue;
        }
        if code_raw.len() == 3
            && code_raw.chars().all(|ch| ch.is_ascii_uppercase())
            && CURRENCY_CODES.contains(&code_raw)
        {
            // The two words must be adjacent modulo one space.
            if text[amount_end..code_start] == *" " {
                claim(claimed, amount_start, code_end);
                push_mention(
                    mentions,
                    "currency",
                    format!("{code_raw} {}", normalize_amount(amount_raw)),
                    amount_start,
                    code_end,
                    0.8,
                    language,
                );
            }
        }
    }
}
