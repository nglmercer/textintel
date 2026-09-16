//! Currency entity scanning (symbol-prefixed amounts and `100 USD` forms).

use crate::core::types::EntityMention;

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
        .filter(|ch| ch.is_ascii_digit() || matches!(ch, '.' | ','))
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
            let amount_start = cursor;
            let mut digits = 0;
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
            if digits > 0 && cursor > amount_start {
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
    // Amount-first form (`100 USD`): word scan over ASCII alphanumerics.
    let mut word_start: Option<usize> = None;
    let mut words: Vec<(usize, usize)> = Vec::new();
    for (offset, ch) in text.char_indices() {
        if ch.is_ascii_alphanumeric() {
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
        if !amount_raw.chars().any(|ch| ch.is_ascii_digit())
            || !amount_raw
                .chars()
                .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | ','))
        {
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
