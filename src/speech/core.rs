//! Shared exact-number and Unicode primitives for speech front ends.
use super::types::{NumberValue, SpeechLocale, SpeechOptions};
pub(crate) const MAX_NUMBER_DIGITS: usize = 128;
pub(crate) use crate::core::calendar::valid_date;

pub(crate) use crate::core::unicode::ascii_decimal_digit as digit;

pub(crate) fn ascii_digits(value: &str) -> Option<String> {
    value.chars().map(digit).collect()
}

pub(crate) fn skip_space(text: &str, start: usize) -> usize {
    start
        + text[start..]
            .char_indices()
            .take_while(|(_, ch)| ch.is_whitespace())
            .map(|(_, ch)| ch.len_utf8())
            .sum::<usize>()
}

pub(crate) fn scan_numeric_end(text: &str, start: usize) -> usize {
    scan_numeric_candidate(text, start, false)
}

/// Scan a locale-known quantity candidate. Plain prose keeps the conservative
/// wrapper above; interpretation and complete grouping remain in parse_number.
pub(crate) fn scan_numeric_end_for_locale(text: &str, start: usize, locale: SpeechLocale) -> usize {
    scan_numeric_candidate(text, start, uses_indian_grouping(locale))
}

fn uses_indian_grouping(locale: SpeechLocale) -> bool {
    matches!(
        locale,
        SpeechLocale::Hi
            | SpeechLocale::Bn
            | SpeechLocale::Ur
            | SpeechLocale::Pa
            | SpeechLocale::Mr
            | SpeechLocale::Te
            | SpeechLocale::Ta
    )
}

fn scan_numeric_candidate(text: &str, start: usize, indian: bool) -> usize {
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
                digit(ch).is_some()
                    || matches!(ch, ' ' | '\u{a0}' | '\u{202f}')
                    || (i == 0 && matches!(ch, '+' | '-' | '−'))
            });
            if !saw_digit || !(group == 3 || (indian && group == 2)) || !plain_group {
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

pub(crate) fn parse_number(
    raw: &str,
    options: &SpeechOptions,
) -> Result<NumberValue, &'static str> {
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
    if raw.contains('٬') && explicit == Some(',') {
        return Err("decimal separator conflicts with Arabic grouping mark");
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
        let indian = uses_indian_grouping(options.locale)
            && pieces.len() >= 2
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

/// Canonical semantic count after the caller has established a quantity context.
/// This never changes fractional digits, sign, or identifier reading policy.
pub(crate) fn canonical_count(value: &NumberValue) -> NumberValue {
    let mut canonical = value.clone();
    if value.integer.is_empty() || !value.integer.bytes().all(|b| b.is_ascii_digit()) {
        return canonical;
    }
    let integer = canonical.integer.trim_start_matches('0');
    canonical.integer = if integer.is_empty() {
        "0".into()
    } else {
        integer.into()
    };
    canonical
}

/// Zero magnitude for an already validated exact decimal value.
pub(crate) fn number_is_zero(value: &NumberValue) -> bool {
    value.integer.bytes().all(|b| b == b'0')
        && value
            .fraction
            .as_ref()
            .is_none_or(|f| f.bytes().all(|b| b == b'0'))
}

/// Scripts whose words need not be separated by spaces around numeric forms.
pub(crate) use crate::core::unicode::is_cjk;
pub(crate) fn is_word(character: char) -> bool {
    (character.is_alphanumeric() && !is_cjk(character))
        || character == '_'
        || crate::core::unicode::is_combining_mark(character)
}
pub(crate) fn left_boundary(text: &str, start: usize) -> bool {
    text.get(..start)
        .is_some_and(|head| head.chars().next_back().is_none_or(|ch| !is_word(ch)))
}
pub(crate) fn right_boundary(text: &str, end: usize) -> bool {
    text.get(end..)
        .is_some_and(|tail| tail.chars().next().is_none_or(|ch| !is_word(ch)))
}

/// Shift an exact decimal quantity without floating point or loss of fractional zeros.
/// Both components remain bounded to the common numeric renderer's digit limit.
pub(crate) fn shift_decimal(value: &NumberValue, power: i32) -> Option<NumberValue> {
    const MAX: usize = MAX_NUMBER_DIGITS;
    if value.integer.is_empty()
        || value.integer.len() > MAX
        || !value.integer.bytes().all(|b| b.is_ascii_digit())
        || value.fraction.as_ref().is_some_and(|f| {
            f.is_empty() || f.len() > MAX || !f.bytes().all(|b| b.is_ascii_digit())
        })
    {
        return None;
    }
    let mut digits = format!(
        "{}{}",
        value.integer,
        value.fraction.as_deref().unwrap_or("")
    );
    let point = i64::try_from(value.integer.len())
        .ok()?
        .checked_add(i64::from(power))?;
    let (integer, fraction) = if point <= 0 {
        let zeros = usize::try_from(-point).ok()?;
        if zeros.checked_add(digits.len())? > MAX {
            return None;
        }
        ("0".into(), Some(format!("{}{digits}", "0".repeat(zeros))))
    } else {
        let point = usize::try_from(point).ok()?;
        if point > MAX {
            return None;
        }
        if point < digits.len() {
            let fraction = digits.split_off(point);
            if fraction.len() > MAX {
                return None;
            }
            (digits, Some(fraction))
        } else {
            digits.extend(std::iter::repeat_n('0', point - digits.len()));
            (digits, None)
        }
    };
    Some(canonical_count(&NumberValue {
        negative: value.negative,
        integer,
        fraction,
    }))
}
