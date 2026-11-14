//! Lossless structural parsing for speech normalization.
//!
//! Parsing does not guess a locale's currency or date order. Exact source
//! spans survive; uncertain or malformed numeric constructs are explicitly
//! marked instead of being partially expanded into unrelated numbers.
use super::types::{
    DateOrder, NumberReading, NumberValue, SpeechDocument, SpeechOptions, SpeechSpan, SpeechValue,
};
use crate::TextIntelError;

/// Parse speech-oriented spans without changing or dropping input bytes.
pub fn parse_for_speech(
    text: &str,
    options: &SpeechOptions,
) -> Result<SpeechDocument, TextIntelError> {
    if options.max_input_chars == 0 || options.max_spans == 0 {
        return Err(TextIntelError::InvalidConfiguration(
            "speech limits must be positive".into(),
        ));
    }
    if !matches!(options.decimal_separator, None | Some('.' | ',' | '٫')) {
        return Err(TextIntelError::InvalidConfiguration(
            "speech decimal_separator must be '.', ',' or '٫'".into(),
        ));
    }
    let length = text.chars().count();
    if length > options.max_input_chars {
        return Err(TextIntelError::InputTooLong {
            length,
            maximum: options.max_input_chars,
        });
    }
    let mut spans = Vec::new();
    let mut cursor = 0;
    let mut plain_start = 0;
    while cursor < text.len() {
        if let Some((end, value)) = parse_at(text, cursor, options) {
            if plain_start < cursor {
                push_span(
                    &mut spans,
                    text,
                    plain_start,
                    cursor,
                    SpeechValue::Text,
                    options,
                )?;
            }
            push_span(&mut spans, text, cursor, end, value, options)?;
            cursor = end;
            plain_start = end;
        } else {
            cursor += text[cursor..].chars().next().map_or(0, char::len_utf8);
        }
    }
    if plain_start < text.len() {
        push_span(
            &mut spans,
            text,
            plain_start,
            text.len(),
            SpeechValue::Text,
            options,
        )?;
    }
    Ok(SpeechDocument {
        text: text.into(),
        locale: options.locale,
        spans,
    })
}

fn push_span(
    spans: &mut Vec<SpeechSpan>,
    text: &str,
    start: usize,
    end: usize,
    value: SpeechValue,
    options: &SpeechOptions,
) -> Result<(), TextIntelError> {
    if spans.len() >= options.max_spans {
        return Err(TextIntelError::TooManySegments {
            maximum: options.max_spans,
        });
    }
    spans.push(SpeechSpan {
        start,
        end,
        source: text[start..end].into(),
        value,
    });
    Ok(())
}

fn ambiguous(reason: &str) -> SpeechValue {
    SpeechValue::Ambiguous {
        reason: reason.into(),
    }
}
fn identifier(kind: &str) -> SpeechValue {
    SpeechValue::Identifier { kind: kind.into() }
}
fn is_cjk(character: char) -> bool {
    matches!(character as u32, 0x1100..=0x11ff | 0x3040..=0x30ff | 0x3130..=0x318f | 0x3400..=0x9fff | 0xac00..=0xd7af | 0xff66..=0xff9d | 0x20000..=0x323af)
}
fn is_word(character: char) -> bool {
    (character.is_alphanumeric() && !is_cjk(character)) || character == '_'
}
fn left_boundary(text: &str, start: usize) -> bool {
    text[..start]
        .chars()
        .next_back()
        .is_none_or(|ch| !is_word(ch))
}
fn right_boundary(text: &str, end: usize) -> bool {
    text[end..].chars().next().is_none_or(|ch| !is_word(ch))
}

/// Decimal digit families used by the supported locales (plus fullwidth and
/// mathematical decimal digits). Unlike `char::to_digit`, this handles Nd
/// scripts without converting the original text or losing byte alignment.
fn digit(ch: char) -> Option<char> {
    let value = ch as u32;
    const ZEROES: &[u32] = &[
        0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
        0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0xff10, 0x1d7ce, 0x1d7d8, 0x1d7e2,
        0x1d7ec, 0x1d7f6,
    ];
    ZEROES.iter().find_map(|zero| {
        (value >= *zero && value < *zero + 10).then(|| char::from(b'0' + (value - *zero) as u8))
    })
}
fn ascii_digits(value: &str) -> Option<String> {
    value.chars().map(digit).collect()
}
fn token_end(text: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in text[start..].char_indices() {
        if ch.is_whitespace()
            || matches!(
                ch,
                '<' | '>'
                    | '"'
                    | '\''
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | ','
                    | ';'
                    | '!'
                    | '?'
                    | '。'
                    | '、'
                    | '，'
                    | '；'
                    | '！'
                    | '？'
            )
        {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    while end > start
        && text[..end]
            .chars()
            .next_back()
            .is_some_and(|ch| matches!(ch, '.' | ',' | ';' | ':' | '!' | '?'))
    {
        end -= text[..end].chars().next_back().map_or(0, char::len_utf8);
    }
    end
}

fn parse_at(text: &str, start: usize, options: &SpeechOptions) -> Option<(usize, SpeechValue)> {
    if !left_boundary(text, start) {
        return None;
    }
    let rest = &text[start..];
    let first = rest.chars().next()?;
    // Han, kana and Hangul do not require word-separating spaces. Do not
    // consume an entire CJK sentence as an alphanumeric identifier.
    if is_cjk(first) {
        return None;
    }
    let end = token_end(text, start);
    let token = &text[start..end];
    if ["https://", "http://", "www."].iter().any(|prefix| {
        rest.get(..prefix.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
    }) {
        let url_end = url_end(text, start);
        return Some((url_end, identifier("url")));
    }
    if token.contains('@') && email_like(token) {
        return Some((end, identifier("email")));
    }
    if matches!(first, '@' | '#') && token.len() > first.len_utf8() {
        return Some((
            end,
            identifier(if first == '@' { "mention" } else { "hashtag" }),
        ));
    }
    if first.is_alphabetic() {
        if let Some((number_start, code)) = currency_prefix(text, start) {
            return currency_number(text, start, number_start, code, options);
        }
        // Protect a full mixed identifier before looking for interior digits.
        if token.chars().any(|ch| digit(ch).is_some()) {
            return Some((
                end,
                identifier(if version_like(token) {
                    "version"
                } else {
                    "alphanumeric"
                }),
            ));
        }
        return None;
    }
    if let Some((number_start, code)) = currency_prefix(text, start) {
        return currency_number(text, start, number_start, code, options);
    }
    if digit(first).is_none() && !matches!(first, '+' | '-' | '−' | '(' | '.' | ',') {
        return None;
    }
    // Signed and parenthesized phone numbers are recognizable without
    // mistaking ordinary grouped quantities for telephone numbers.
    if matches!(first, '+' | '(')
        && let Some(end) = telephone_end(text, start)
        && !(first == '(' && date_shaped(text[start + 1..end].trim_end_matches(')')))
    {
        return Some((end, identifier("telephone")));
    }
    let numeric_end = scan_numeric_end(text, start);
    if numeric_end == start {
        return None;
    }
    let raw = &text[start..numeric_end];
    if !right_boundary(text, numeric_end) {
        if raw.contains(':') {
            let (time_end, value) = parse_time(text, start, numeric_end);
            if time_end > numeric_end {
                return Some((time_end, value));
            }
            return Some((
                end.max(numeric_end).max(token_end(text, numeric_end)),
                ambiguous("time has an unsupported suffix"),
            ));
        }
        if let Some((end, code)) = currency_suffix(text, numeric_end) {
            return Some((
                end,
                match parse_number(raw, options) {
                    Ok(value) => SpeechValue::Currency {
                        code: code.into(),
                        value,
                    },
                    Err(reason) => ambiguous(reason),
                },
            ));
        }
        // An attached unit is handled below; other mixed tokens remain whole.
        if let Some((end, unit)) = unit_suffix(text, numeric_end) {
            return Some((
                end,
                match parse_number(raw, options) {
                    Ok(value) => SpeechValue::Measure {
                        unit: unit.into(),
                        value,
                    },
                    Err(reason) => ambiguous(reason),
                },
            ));
        }
        return Some((
            end.max(numeric_end).max(token_end(text, numeric_end)),
            identifier("alphanumeric"),
        ));
    }
    if digit(first).is_some()
        && raw.contains('-')
        && !date_shaped(raw)
        && let Some(end) = telephone_end(text, start)
    {
        return Some((end, identifier("telephone")));
    }
    if let Some(value) = parse_date(raw, options.date_order) {
        return Some((numeric_end, value));
    }
    if raw.contains(':') {
        return Some(parse_time(text, start, numeric_end));
    }
    if version_like(raw) {
        return Some((numeric_end, identifier("version")));
    }
    if raw.contains('/') || raw.chars().skip(1).any(|ch| matches!(ch, '+' | '-' | '−')) {
        return Some((
            numeric_end,
            ambiguous("unsupported fraction, range, or malformed date"),
        ));
    }
    let range_start = skip_space(text, numeric_end);
    if let Some(ch @ ('-' | '–' | '—' | '…')) = text[range_start..].chars().next() {
        let second_start = skip_space(text, range_start + ch.len_utf8());
        if text[second_start..]
            .chars()
            .next()
            .is_some_and(|ch| digit(ch).is_some())
        {
            let end = scan_numeric_end(text, second_start);
            return Some((
                end,
                ambiguous("numeric range needs an explicit reading policy"),
            ));
        }
    }
    let number = match parse_number(raw, options) {
        Ok(value) => value,
        Err(reason) => return Some((numeric_end, ambiguous(reason))),
    };
    let suffix_start = skip_space(text, numeric_end);
    if let Some((suffix_end, code)) = currency_suffix(text, suffix_start) {
        return Some((
            suffix_end,
            SpeechValue::Currency {
                code: code.into(),
                value: number,
            },
        ));
    }
    if let Some(ch @ ('%' | '٪')) = text[suffix_start..].chars().next() {
        let end = suffix_start + ch.len_utf8();
        if right_boundary(text, end) {
            return Some((end, SpeechValue::Percent { value: number }));
        }
    }
    if let Some((suffix_end, unit)) = unit_suffix(text, suffix_start) {
        return Some((
            suffix_end,
            SpeechValue::Measure {
                unit: unit.into(),
                value: number,
            },
        ));
    }
    let reading =
        if number.fraction.is_none() && number.integer.len() > 1 && number.integer.starts_with('0')
        {
            NumberReading::Digits
        } else {
            NumberReading::Cardinal
        };
    Some((
        numeric_end,
        SpeechValue::Number {
            value: number,
            reading,
        },
    ))
}

fn skip_space(text: &str, start: usize) -> usize {
    start
        + text[start..]
            .char_indices()
            .take_while(|(_, ch)| ch.is_whitespace())
            .map(|(_, ch)| ch.len_utf8())
            .sum::<usize>()
}
fn url_end(text: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in text[start..].char_indices() {
        if ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\'') {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    while end > start
        && text[..end]
            .chars()
            .next_back()
            .is_some_and(|ch| matches!(ch, '.' | ',' | ';' | '!' | '?' | ')' | ']'))
    {
        end -= text[..end].chars().next_back().map_or(0, char::len_utf8);
    }
    end
}
fn email_like(token: &str) -> bool {
    let mut pieces = token.split('@');
    let Some(local) = pieces.next() else {
        return false;
    };
    let Some(domain) = pieces.next() else {
        return false;
    };
    !local.is_empty()
        && pieces.next().is_none()
        && domain.contains('.')
        && local
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '%' | '+' | '-'))
        && domain
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-'))
        && domain.split('.').all(|part| !part.is_empty())
}
fn version_like(token: &str) -> bool {
    let value = token
        .strip_prefix('v')
        .or_else(|| token.strip_prefix('V'))
        .unwrap_or(token);
    let pieces: Vec<_> = value.split('.').collect();
    pieces.len() >= 3
        && pieces
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|ch| digit(ch).is_some()))
}
fn telephone_end(text: &str, start: usize) -> Option<usize> {
    let mut digits = 0;
    let mut end = start;
    let mut separators = 0;
    for (offset, ch) in text[start..].char_indices() {
        if digit(ch).is_some() {
            digits += 1;
            end = start + offset + ch.len_utf8();
        } else if matches!(ch, '-' | '(' | ')' | ' ' | '\u{a0}') || (ch == '+' && offset == 0) {
            separators += 1;
        } else {
            break;
        }
        if digits > 15 {
            return None;
        }
    }
    (digits >= 7 && separators > 0 && right_boundary(text, end)).then_some(end)
}
fn scan_numeric_end(text: &str, start: usize) -> usize {
    let mut end = start;
    let mut saw_digit = false;
    for (offset, ch) in text[start..].char_indices() {
        if digit(ch).is_some() {
            saw_digit = true;
        } else if ch == ' ' {
            let after = start + offset + 1;
            let group = text[after..]
                .chars()
                .take_while(|ch| digit(*ch).is_some())
                .count();
            let prefix = &text[start..start + offset];
            let plain_group = prefix.chars().enumerate().all(|(i, ch)| {
                digit(ch).is_some() || ch == ' ' || (i == 0 && matches!(ch, '+' | '-' | '−'))
            });
            if !saw_digit || group != 3 || !plain_group {
                break;
            }
        } else if !matches!(
            ch,
            '+' | '-' | '−' | '.' | ',' | '/' | ':' | '٫' | '٬' | '\u{a0}' | '\u{202f}'
        ) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    if !saw_digit {
        return start;
    }
    while end > start
        && !text[end..].starts_with(['e', 'E'])
        && text[..end]
            .chars()
            .next_back()
            .is_some_and(|ch| matches!(ch, '.' | ',' | '\u{a0}' | '\u{202f}'))
    {
        end -= text[..end].chars().next_back().map_or(0, char::len_utf8);
    }
    end
}

const CURRENCIES: &[&str] = &[
    "USD", "EUR", "GBP", "PEN", "JPY", "CNY", "INR", "BDT", "PKR", "IDR", "BRL", "RUB", "KRW",
    "TRY", "VND", "CAD", "AUD", "CHF", "MXN", "ARS", "CLP", "COP", "AED", "SAR", "EGP", "NGN",
    "ZAR", "THB", "MYR", "SGD", "NZD", "HKD", "TWD", "PHP", "ILS", "PLN",
];
fn currency_prefix(text: &str, start: usize) -> Option<(usize, &'static str)> {
    let rest = &text[start..];
    for (symbol, code) in [
        ("US$", "USD"),
        ("S/", "PEN"),
        ("€", "EUR"),
        ("£", "GBP"),
        ("₹", "INR"),
        ("₽", "RUB"),
        ("₩", "KRW"),
        ("₺", "TRY"),
        ("₫", "VND"),
        ("₱", "PHP"),
        ("₪", "ILS"),
        ("$", "?"),
        ("¥", "?"),
    ] {
        if rest.starts_with(symbol) {
            let number_start = skip_space(text, start + symbol.len());
            if text[number_start..]
                .chars()
                .next()
                .is_some_and(|ch| digit(ch).is_some() || matches!(ch, '+' | '-' | '−'))
            {
                return Some((number_start, code));
            }
        }
    }
    for code in CURRENCIES {
        if rest.starts_with(code) {
            let number_start = skip_space(text, start + code.len());
            if text[number_start..]
                .chars()
                .next()
                .is_some_and(|ch| digit(ch).is_some() || matches!(ch, '+' | '-' | '−'))
            {
                return Some((number_start, code));
            }
        }
    }
    None
}
fn currency_suffix(text: &str, start: usize) -> Option<(usize, &'static str)> {
    CURRENCIES.iter().find_map(|code| {
        let end = start + code.len();
        (text[start..].starts_with(code) && right_boundary(text, end)).then_some((end, *code))
    })
}
fn currency_number(
    text: &str,
    start: usize,
    number_start: usize,
    code: &str,
    options: &SpeechOptions,
) -> Option<(usize, SpeechValue)> {
    let end = scan_numeric_end(text, number_start);
    if end == number_start {
        return None;
    }
    let value = if code == "?" {
        ambiguous("currency symbol needs an explicit ISO currency code")
    } else if !right_boundary(text, end) {
        ambiguous("currency amount has an attached identifier")
    } else {
        match parse_number(&text[number_start..end], options) {
            Ok(value) => SpeechValue::Currency {
                code: code.into(),
                value,
            },
            Err(reason) => ambiguous(reason),
        }
    };
    let end = if !right_boundary(text, end) {
        token_end(text, start).max(end).max(token_end(text, end))
    } else {
        end
    };
    Some((end, value))
}
fn unit_suffix(text: &str, start: usize) -> Option<(usize, &'static str)> {
    // Longest-first prevents interpreting `m` as the prefix of `mg`/`m²`.
    const UNITS: &[&str] = &[
        "km/h", "m/s", "°C", "°F", "kHz", "MHz", "GHz", "kWh", "m²", "m³", "kg", "mg", "km", "cm",
        "mm", "ml", "mL", "ms", "Hz", "kW", "MW", "GB", "MB", "TB", "KB", "min", "g", "m", "L",
        "l", "s", "h", "W", "V", "A", "K",
    ];
    UNITS.iter().find_map(|unit| {
        let end = start + unit.len();
        (text[start..].starts_with(unit) && right_boundary(text, end)).then_some((end, *unit))
    })
}

fn valid_date(year: u32, month: u32, day: u32) -> bool {
    if !(1..=9999).contains(&year) {
        return false;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return false,
    };
    (1..=days).contains(&day)
}
fn date_shaped(raw: &str) -> bool {
    let parts: Vec<_> = raw.split('-').collect();
    parts.len() >= 3
        && (parts[0].chars().count() == 4
            || (parts[2].chars().count() == 4
                && parts[0].chars().count() <= 2
                && parts[1].chars().count() <= 2))
}

fn parse_date(raw: &str, order: DateOrder) -> Option<SpeechValue> {
    let separator = if raw.contains('/') {
        '/'
    } else if raw.chars().skip(1).any(|ch| ch == '-') {
        '-'
    } else {
        return None;
    };
    let parts: Vec<_> = raw.split(separator).collect();
    if parts.len() != 3 {
        return Some(ambiguous("unsupported fraction, range, or malformed date"));
    }
    let digits: Option<Vec<String>> = parts.iter().map(|p| ascii_digits(p)).collect();
    let Some(digits) = digits else {
        return Some(ambiguous("invalid date digits"));
    };
    let values: Option<Vec<u32>> = digits.iter().map(|p| p.parse::<u32>().ok()).collect();
    let Some(values) = values else {
        return Some(ambiguous("invalid date width"));
    };
    let (a, b, c) = (values[0], values[1], values[2]);
    let make = |year, month, day| SpeechValue::Date { year, month, day };
    if digits[0].len() == 4 {
        return Some(
            if digits[1].len() <= 2 && digits[2].len() <= 2 && valid_date(a, b, c) {
                make(a, b, c)
            } else {
                ambiguous("invalid calendar date")
            },
        );
    }
    if digits[2].len() != 4
        || !(1..=2).contains(&digits[0].len())
        || !(1..=2).contains(&digits[1].len())
    {
        return Some(ambiguous(
            "date needs a four-digit year and one- or two-digit day/month",
        ));
    }
    let dmy = valid_date(c, b, a);
    let mdy = valid_date(c, a, b);
    Some(match order {
        DateOrder::DayMonthYear if dmy => make(c, b, a),
        DateOrder::MonthDayYear if mdy => make(c, a, b),
        DateOrder::RejectAmbiguous if dmy && (!mdy || a == b) => make(c, b, a),
        DateOrder::RejectAmbiguous if mdy && !dmy => make(c, a, b),
        DateOrder::RejectAmbiguous if dmy && mdy => {
            ambiguous("date order is ambiguous; set date_order")
        }
        _ => ambiguous("invalid calendar date"),
    })
}
fn parse_time(text: &str, start: usize, end: usize) -> (usize, SpeechValue) {
    let parts: Vec<_> = text[start..end].split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return (end, ambiguous("invalid time"));
    }
    let Some(values) = parts
        .iter()
        .map(|p| ascii_digits(p)?.parse::<u32>().ok())
        .collect::<Option<Vec<_>>>()
    else {
        return (end, ambiguous("invalid time"));
    };
    if !(1..=2).contains(&parts[0].chars().count())
        || parts[1].chars().count() != 2
        || (parts.len() == 3 && parts[2].chars().count() != 2)
    {
        return (end, ambiguous("invalid time width"));
    }
    let (mut hour, minute) = (values[0], values[1]);
    let second = values.get(2).copied();
    let suffix = skip_space(text, end);
    let mut final_end = end;
    for marker in ["a.m.", "p.m.", "am", "pm", "AM", "PM"] {
        if text[suffix..].starts_with(marker) && right_boundary(text, suffix + marker.len()) {
            if !(1..=12).contains(&hour) {
                return (suffix + marker.len(), ambiguous("invalid twelve-hour time"));
            }
            let pm = marker.starts_with('p') || marker.starts_with('P');
            hour = hour % 12 + if pm { 12 } else { 0 };
            final_end = suffix + marker.len();
            break;
        }
    }
    if hour > 23 || minute > 59 || second.is_some_and(|s| s > 59) {
        return (final_end, ambiguous("invalid clock time"));
    }
    (
        final_end,
        SpeechValue::Time {
            hour,
            minute,
            second,
        },
    )
}

fn parse_number(raw: &str, options: &SpeechOptions) -> Result<NumberValue, &'static str> {
    let mut text = String::with_capacity(raw.len());
    for ch in raw.chars() {
        text.push(if let Some(d) = digit(ch) {
            d
        } else {
            match ch {
                '−' => '-',
                '٫' => '.',
                '٬' => ',',
                '\u{a0}' | '\u{202f}' => ' ',
                other => other,
            }
        });
    }
    let negative = text.starts_with('-');
    let text = text.strip_prefix(['+', '-']).unwrap_or(&text);
    if text.is_empty()
        || !text
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | ',' | ' '))
    {
        return Err("invalid number");
    }
    let dots = text.matches('.').count();
    let commas = text.matches(',').count();
    let explicit = options
        .decimal_separator
        .map(|s| if s == '٫' { '.' } else { s });
    if raw.contains('٫') && explicit == Some(',') {
        return Err("decimal separator conflicts with Arabic decimal mark");
    }
    let explicit =
        explicit.or_else(|| raw.chars().any(|ch| matches!(ch, '٫' | '٬')).then_some('.'));
    let decimal = if let Some(decimal) = explicit {
        Some(decimal)
    } else if dots > 0 && commas > 0 {
        Some(if text.rfind('.') > text.rfind(',') {
            '.'
        } else {
            ','
        })
    } else if dots + commas == 1 {
        let separator = if dots == 1 { '.' } else { ',' };
        let (whole, fraction) = text.split_once(separator).ok_or("invalid number")?;
        if !whole.is_empty() && fraction.len() == 3 {
            return Err("number separator is ambiguous; set decimal_separator");
        }
        Some(separator)
    } else {
        None
    };
    let (whole, fraction) = if let Some(decimal) = decimal {
        if text.matches(decimal).count() > 1 {
            return Err("repeated decimal separator");
        }
        match text.split_once(decimal) {
            Some((whole, fraction)) => {
                if fraction.is_empty() || !fraction.chars().all(|ch| ch.is_ascii_digit()) {
                    return Err("invalid fractional digits");
                }
                (whole, Some(fraction.to_string()))
            }
            None => (text, None),
        }
    } else {
        (text, None)
    };
    if whole.is_empty() {
        return fraction
            .map(|fraction| NumberValue {
                negative,
                integer: "0".into(),
                fraction: Some(fraction),
            })
            .ok_or("number needs integer digits");
    }
    let grouping: Vec<char> = whole.chars().filter(|ch| !ch.is_ascii_digit()).collect();
    if let Some(separator) = grouping.first() {
        if grouping.iter().any(|ch| ch != separator) {
            return Err("mixed grouping separators");
        }
        let pieces: Vec<_> = whole.split(*separator).collect();
        let western = pieces
            .first()
            .is_some_and(|p| !p.is_empty() && p.len() <= 3)
            && pieces.iter().skip(1).all(|p| p.len() == 3);
        let indian = matches!(
            options.locale,
            super::types::SpeechLocale::Hi
                | super::types::SpeechLocale::Bn
                | super::types::SpeechLocale::Ur
                | super::types::SpeechLocale::Pa
                | super::types::SpeechLocale::Mr
                | super::types::SpeechLocale::Te
                | super::types::SpeechLocale::Ta
        ) && pieces.len() >= 2
            && pieces
                .first()
                .is_some_and(|p| !p.is_empty() && p.len() <= 2)
            && pieces.last().is_some_and(|p| p.len() == 3)
            && pieces[1..pieces.len() - 1].iter().all(|p| p.len() == 2);
        if !western && !indian {
            return Err("invalid digit grouping");
        }
    }
    let integer: String = whole.chars().filter(char::is_ascii_digit).collect();
    if integer.is_empty() {
        return Err("number needs integer digits");
    }
    Ok(NumberValue {
        negative,
        integer,
        fraction,
    })
}
