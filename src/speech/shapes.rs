//! Shared source grammars for numeric relations, scientific numbers and durations.
//!
//! Front ends select explicit compatibility profiles and retain their own
//! activation, protected-region, boundary, warning and rendering policies.
//! Every end offset addresses the original UTF-8 input.
use super::core::{
    MAX_NUMBER_DIGITS, ascii_digits, digit, number_is_zero, parse_number, scan_numeric_end,
    scan_numeric_end_for_locale, skip_space,
};
use super::{NumberValue, SpeechLocale, SpeechOptions};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShapePolicy {
    /// Versioned API: attached forms and established small budgets.
    Versioned,
    /// Typed API: Unicode digits, operator notation and explicit units.
    Typed,
}

pub(crate) enum RelationValue {
    Fraction {
        numerator: NumberValue,
        denominator: NumberValue,
    },
    MixedFraction {
        whole: NumberValue,
        numerator: NumberValue,
        denominator: NumberValue,
    },
    Range {
        from: NumberValue,
        to: NumberValue,
    },
}
pub(crate) struct RelationShape {
    pub end: usize,
    pub value: RelationValue,
}

fn relation_number(raw: &str, options: &SpeechOptions) -> Option<NumberValue> {
    let value = parse_number(raw.trim(), options).ok()?;
    (value.integer.len() <= MAX_NUMBER_DIGITS
        && value
            .fraction
            .as_ref()
            .is_none_or(|f| f.len() <= MAX_NUMBER_DIGITS))
    .then_some(value)
}

fn legacy_unsigned(raw: &str, range: bool) -> Option<NumberValue> {
    if raw.is_empty()
        || !raw.bytes().all(|c| c.is_ascii_digit())
        || (range && (raw.len() > 15 || (raw.len() > 1 && raw.starts_with('0'))))
    {
        return None;
    }
    // Fractions historically accept u64-sized counts including padding;
    // ranges use the stricter 15-digit, unpadded plain-count policy.
    let count = raw.parse::<u64>().ok()?;
    Some(NumberValue {
        negative: false,
        integer: count.to_string(),
        fraction: None,
    })
}

/// Shared slash-fraction, mixed-fraction and range grammar. Caller-owned
/// boundaries, unit attachment and presentation are not inferred here.
pub(crate) fn relation(
    text: &str,
    start: usize,
    options: &SpeechOptions,
    policy: ShapePolicy,
) -> Option<RelationShape> {
    text.get(start..)?;
    let mut end = start;
    for (offset, ch) in text[start..].char_indices() {
        let allowed = if policy == ShapePolicy::Versioned {
            ch.is_ascii_digit() || matches!(ch, '/' | '–')
        } else {
            digit(ch).is_some()
                || matches!(
                    ch,
                    '+' | '-'
                        | '−'
                        | '–'
                        | '—'
                        | '.'
                        | ','
                        | '/'
                        | '٫'
                        | '٬'
                        | ' '
                        | '\u{a0}'
                        | '\u{202f}'
                )
        };
        if !allowed {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    if policy == ShapePolicy::Typed {
        while end > start
            && text[..end]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '.' | ','))
        {
            end -= text[..end].chars().next_back()?.len_utf8();
        }
    }
    let raw = text.get(start..end)?;
    if raw.is_empty() {
        return None;
    }
    if raw.matches('/').count() == 1 {
        let (left, right) = raw.split_once('/')?;
        if policy == ShapePolicy::Versioned {
            let numerator = legacy_unsigned(left, false)?;
            let denominator = legacy_unsigned(right, false)?;
            if number_is_zero(&denominator) {
                return None;
            }
            return Some(RelationShape {
                end,
                value: RelationValue::Fraction {
                    numerator,
                    denominator,
                },
            });
        }
        let denominator = relation_number(right, options)?;
        if number_is_zero(&denominator) {
            return None;
        }
        if let Some(numerator) = relation_number(left, options) {
            return Some(RelationShape {
                end,
                value: RelationValue::Fraction {
                    numerator,
                    denominator,
                },
            });
        }
        let split = left.trim_end().rfind(char::is_whitespace)?;
        let whole = relation_number(&left[..split], options)?;
        let numerator = relation_number(&left[split..], options)?;
        if whole.fraction.is_some()
            || numerator.fraction.is_some()
            || denominator.fraction.is_some()
            || numerator.negative
            || denominator.negative
        {
            return None;
        }
        return Some(RelationShape {
            end,
            value: RelationValue::MixedFraction {
                whole,
                numerator,
                denominator,
            },
        });
    }
    if policy == ShapePolicy::Versioned {
        let (left, right) = raw.split_once('–')?;
        let from = legacy_unsigned(left, true)?;
        let to = legacy_unsigned(right, true)?;
        if from.integer.parse::<u64>().ok()? >= to.integer.parse::<u64>().ok()? {
            return None;
        }
        return Some(RelationShape {
            end,
            value: RelationValue::Range { from, to },
        });
    }
    for (offset, ch) in raw.char_indices().skip(1) {
        if !matches!(ch, '-' | '–' | '—') {
            continue;
        }
        let Some(from) = relation_number(&raw[..offset], options) else {
            continue;
        };
        let Some(to) = relation_number(&raw[offset + ch.len_utf8()..], options) else {
            continue;
        };
        return Some(RelationShape {
            end,
            value: RelationValue::Range { from, to },
        });
    }
    None
}

pub(crate) struct ScientificShape {
    pub end: usize,
    pub mantissa: NumberValue,
    pub exponent: i32,
}

pub(crate) fn digits_end(text: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in text[start..].char_indices() {
        if digit(ch).is_none() {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn mantissa(raw: &str, options: &SpeechOptions, policy: ShapePolicy) -> Option<NumberValue> {
    let mut legacy_options;
    let legacy_negative = raw.starts_with(['-', '−']);
    let mut parse_raw = raw;
    let options = if policy == ShapePolicy::Versioned {
        // A scientific mantissa's single dot OR comma is decimal, regardless
        // of the prose locale. Padded explicit decimal integers stay out.
        let body = raw.strip_prefix(['-', '−']).unwrap_or(raw);
        if legacy_negative && body.starts_with(['+', '-', '−']) {
            return None;
        }
        parse_raw = body;
        let groups: Vec<_> = body.split(['.', ',']).collect();
        // Historically the no-dot/comma branch delegates directly to the
        // exact parser, accepting '+' and native digits. Preserve that too.
        if groups.len() > 2
            || (groups.len() == 2
                && (!groups
                    .iter()
                    .all(|group| !group.is_empty() && group.bytes().all(|ch| ch.is_ascii_digit()))
                    || (groups[0].len() > 1 && groups[0].starts_with('0'))))
        {
            return None;
        }
        legacy_options = SpeechOptions::new(SpeechLocale::En);
        legacy_options.decimal_separator = Some(if body.contains(',') { ',' } else { '.' });
        &legacy_options
    } else {
        options
    };
    let mut value = parse_number(parse_raw, options).ok()?;
    if policy == ShapePolicy::Versioned {
        value.negative = legacy_negative;
    }
    let (integer_limit, fraction_limit) = match policy {
        ShapePolicy::Versioned => (15, 12),
        ShapePolicy::Typed => (MAX_NUMBER_DIGITS, MAX_NUMBER_DIGITS),
    };
    (value.integer.len() <= integer_limit
        && value
            .fraction
            .as_ref()
            .is_none_or(|f| f.len() <= fraction_limit))
    .then_some(value)
}

pub(crate) fn scientific(
    text: &str,
    start: usize,
    options: &SpeechOptions,
    policy: ShapePolicy,
) -> Option<ScientificShape> {
    text.get(start..)?;
    let mantissa_end = if policy == ShapePolicy::Typed {
        scan_numeric_end_for_locale(text, start, options.locale)
    } else {
        scan_numeric_end(text, start)
    };
    if mantissa_end == start {
        return None;
    }
    let mantissa = mantissa(&text[start..mantissa_end], options, policy)?;
    let exponent_start = if text[mantissa_end..].starts_with(['e', 'E']) {
        mantissa_end + 1
    } else {
        if policy == ShapePolicy::Versioned {
            return None;
        }
        let mut cursor = skip_space(text, mantissa_end);
        let ch = text[cursor..].chars().next()?;
        if !matches!(ch, '×' | '·' | 'x' | 'X' | '*') {
            return None;
        }
        cursor = skip_space(text, cursor + ch.len_utf8());
        let tens_end = digits_end(text, cursor);
        if ascii_digits(&text[cursor..tens_end])?.as_str() != "10" {
            return None;
        }
        cursor = skip_space(text, tens_end);
        if !text[cursor..].starts_with('^') {
            return None;
        }
        skip_space(text, cursor + 1)
    };
    let mut digits_start = exponent_start;
    if text[digits_start..].starts_with(['+', '-', '−']) {
        if policy == ShapePolicy::Versioned && text[digits_start..].starts_with('+') {
            return None;
        }
        digits_start += text[digits_start..].chars().next()?.len_utf8();
    }
    let end = digits_end(text, digits_start);
    if end == digits_start {
        return None;
    }
    let raw = &text[digits_start..end];
    if policy == ShapePolicy::Versioned && !raw.bytes().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let digits = ascii_digits(raw)?;
    let maximum = if policy == ShapePolicy::Versioned {
        2
    } else {
        4
    };
    if digits.len() > maximum {
        return None;
    }
    let mut exponent = digits.parse::<i32>().ok()?;
    if text[exponent_start..].starts_with(['-', '−']) {
        exponent = -exponent;
    }
    Some(ScientificShape {
        end,
        mantissa,
        exponent,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurationComponent {
    pub unit: DurationUnit,
    pub value: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurationUnit {
    Hours,
    Minutes,
    Seconds,
}
pub(crate) struct DurationShape {
    pub end: usize,
    pub components: Vec<DurationComponent>,
}

pub(crate) fn duration(text: &str, start: usize, policy: ShapePolicy) -> Option<DurationShape> {
    text.get(start..)?;
    let mut cursor = start;
    let mut accepted_end = start;
    let mut components = Vec::new();
    let mut last_order = 0;
    loop {
        let end = digits_end(text, cursor);
        if end == cursor {
            break;
        }
        let source_digits = &text[cursor..end];
        if policy == ShapePolicy::Versioned && !source_digits.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let raw = ascii_digits(source_digits)?;
        if policy == ShapePolicy::Typed && raw.len() > 12 {
            return None;
        }
        let value = raw.parse().ok()?;
        let unit_start = if policy == ShapePolicy::Typed {
            skip_space(text, end)
        } else {
            end
        };
        let units = [
            ("min", DurationUnit::Minutes, 2),
            ("sec", DurationUnit::Seconds, 3),
            ("hr", DurationUnit::Hours, 1),
            ("h", DurationUnit::Hours, 1),
            ("s", DurationUnit::Seconds, 3),
        ];
        let unit = units.into_iter().find(|(token, _, _)| {
            !matches!(
                (policy, *token),
                (ShapePolicy::Typed, "sec") | (ShapePolicy::Versioned, "hr")
            ) && text[unit_start..].starts_with(token)
        });
        let (token, unit, order) = match unit {
            Some(unit) => unit,
            // The established attached form 1h30 has an implicit minute.
            None if policy == ShapePolicy::Versioned && last_order == 1 && end == text.len() => {
                ("", DurationUnit::Minutes, 2)
            }
            None => break,
        };
        if order <= last_order
            || (policy == ShapePolicy::Versioned
                && last_order == 0
                && order == 2
                && unit_start + token.len() != text.len())
        {
            return None;
        }
        last_order = order;
        components.push(DurationComponent { unit, value });
        cursor = unit_start + token.len();
        accepted_end = cursor;
        let next = if policy == ShapePolicy::Typed {
            skip_space(text, cursor)
        } else {
            cursor
        };
        if text[next..]
            .chars()
            .next()
            .is_some_and(|ch| digit(ch).is_some())
        {
            cursor = next;
        } else {
            break;
        }
    }
    (!components.is_empty()).then_some(DurationShape {
        end: accepted_end,
        components,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn whole_scientific(raw: &str, policy: ShapePolicy) -> Option<ScientificShape> {
        let mut options = SpeechOptions::new(SpeechLocale::En);
        options.decimal_separator = Some('.');
        scientific(raw, 0, &options, policy).filter(|shape| shape.end == raw.len())
    }
    fn whole_duration(raw: &str, policy: ShapePolicy) -> Option<DurationShape> {
        duration(raw, 0, policy).filter(|shape| shape.end == raw.len())
    }

    fn whole_relation(raw: &str, policy: ShapePolicy) -> Option<RelationShape> {
        relation(raw, 0, &SpeechOptions::new(SpeechLocale::En), policy)
            .filter(|shape| shape.end == raw.len())
    }

    #[test]
    fn relation_profiles_preserve_count_limits_and_explicit_ordering_policy() {
        for source in [
            "01/02",
            "0/2",
            "18446744073709551615/2",
            "1/18446744073709551615",
            "3–4",
        ] {
            assert!(
                whole_relation(source, ShapePolicy::Versioned).is_some(),
                "{source}"
            );
        }
        for source in [
            "1/0",
            "1/000",
            "18446744073709551616/2",
            "1/18446744073709551616",
            "03–4",
            "4–3",
            "3–3",
            "3-4",
            "-3/2",
            "٣/٢",
            "3 1/2",
            "999999999999999–1000000000000000",
        ] {
            assert!(
                whole_relation(source, ShapePolicy::Versioned).is_none(),
                "{source}"
            );
        }
        for source in [
            "03–4",
            "4–3",
            "3–3",
            "3-4",
            "-3/2",
            "٣/٢",
            "3 1/2",
            "999999999999999–1000000000000000",
        ] {
            assert!(
                whole_relation(source, ShapePolicy::Typed).is_some(),
                "{source}"
            );
        }
        for source in [
            "3/0.00", "3/-0", "3 1/-2", "3 -1/2", "3.5 1/2", "3 1/2.5", "1/2/3",
        ] {
            assert!(
                whole_relation(source, ShapePolicy::Typed).is_none(),
                "{source}"
            );
        }
        assert!(
            whole_relation(
                &format!("{}/2", "9".repeat(MAX_NUMBER_DIGITS)),
                ShapePolicy::Typed
            )
            .is_some()
        );
        assert!(
            whole_relation(
                &format!("{}/2", "9".repeat(MAX_NUMBER_DIGITS + 1)),
                ShapePolicy::Typed
            )
            .is_none()
        );
        assert!(
            whole_relation(
                &format!("1/0.{}", "1".repeat(MAX_NUMBER_DIGITS + 1)),
                ShapePolicy::Typed
            )
            .is_none()
        );
    }

    #[test]
    fn relation_shapes_keep_original_offsets_signs_and_decimal_precision() {
        let options = SpeechOptions::new(SpeechLocale::En);
        let source = "前 −2.50–−1.25, next";
        let start = source.find('−').unwrap();
        let shape = relation(source, start, &options, ShapePolicy::Typed).unwrap();
        assert_eq!(&source[start..shape.end], "−2.50–−1.25");
        let RelationValue::Range { from, to } = shape.value else {
            panic!("range expected")
        };
        assert_eq!(
            from,
            NumberValue {
                negative: true,
                integer: "2".into(),
                fraction: Some("50".into())
            }
        );
        assert_eq!(
            to,
            NumberValue {
                negative: true,
                integer: "1".into(),
                fraction: Some("25".into())
            }
        );
        let shape = whole_relation("-3 1/2", ShapePolicy::Typed).unwrap();
        let RelationValue::MixedFraction {
            whole,
            numerator,
            denominator,
        } = shape.value
        else {
            panic!("mixed fraction expected")
        };
        assert!(whole.negative);
        assert_eq!(whole.integer, "3");
        assert_eq!(numerator.integer, "1");
        assert_eq!(denominator.integer, "2");
        assert!(relation(source, 1, &options, ShapePolicy::Typed).is_none());
    }

    #[test]
    fn scientific_profiles_preserve_distinct_sign_padding_and_digit_budgets() {
        for source in ["003e3", "+3e2", "٣e2", "−3,50e−03", "1.123456789012e99"] {
            assert!(
                whole_scientific(source, ShapePolicy::Versioned).is_some(),
                "{source}"
            );
        }
        for source in [
            "--3e2",
            "-+3e2",
            "−−3e2",
            "+3.5e2",
            "00.5e3",
            "1e+3",
            "1e100",
            "1e003",
            "1e٣",
            "1 ×10^3",
            "1.1234567890123e1",
            "1000000000000000e1",
        ] {
            assert!(
                whole_scientific(source, ShapePolicy::Versioned).is_none(),
                "{source}"
            );
        }
        for source in [
            "+3.5e+100",
            "1e003",
            "1e٣",
            "1 ×10^3",
            "1.1234567890123e1",
            "1000000000000000e1",
        ] {
            assert!(
                whole_scientific(source, ShapePolicy::Typed).is_some(),
                "{source}"
            );
        }
        for source in [
            format!("{}e1", "1".repeat(129)),
            format!("1.{}e1", "1".repeat(129)),
            "1e10000".into(),
        ] {
            assert!(
                whole_scientific(&source, ShapePolicy::Typed).is_none(),
                "{source}"
            );
        }
        assert!(whole_scientific(&format!("{}e1", "1".repeat(128)), ShapePolicy::Typed).is_some());
        let shape = whole_scientific("1,234e2", ShapePolicy::Versioned).unwrap();
        assert_eq!(shape.mantissa.integer, "1");
        assert_eq!(shape.mantissa.fraction.as_deref(), Some("234"));
        let shape = whole_scientific("1,234e2", ShapePolicy::Typed).unwrap();
        assert_eq!(shape.mantissa.integer, "1234");
        assert!(shape.mantissa.fraction.is_none());
    }

    #[test]
    fn duration_profiles_preserve_aliases_implicit_minutes_and_overflow_limits() {
        for source in [
            "1h",
            "1h30",
            "1h30min15sec",
            "1h30sec",
            "45sec",
            "90min",
            "18446744073709551615s",
            "000000000000000000001s",
        ] {
            assert!(
                whole_duration(source, ShapePolicy::Versioned).is_some(),
                "{source}"
            );
        }
        for source in [
            "1hr30min",
            "1 h 30 min",
            "١h٣٠min",
            "30min15s",
            "18446744073709551616s",
        ] {
            assert!(
                whole_duration(source, ShapePolicy::Versioned).is_none(),
                "{source}"
            );
        }
        for source in [
            "1hr30min",
            "1 h 30 min",
            "١h٣٠min",
            "30min15s",
            "999999999999s",
        ] {
            assert!(
                whole_duration(source, ShapePolicy::Typed).is_some(),
                "{source}"
            );
        }
        for source in [
            "1h30",
            "1h30sec",
            "45sec",
            "1000000000000s",
            "1h30min15min",
            "1s1h",
        ] {
            assert!(
                whole_duration(source, ShapePolicy::Typed).is_none(),
                "{source}"
            );
        }
        let shape = whole_duration("1h30", ShapePolicy::Versioned).unwrap();
        assert_eq!(
            shape.components,
            vec![
                DurationComponent {
                    unit: DurationUnit::Hours,
                    value: 1
                },
                DurationComponent {
                    unit: DurationUnit::Minutes,
                    value: 30
                },
            ]
        );
    }
}
