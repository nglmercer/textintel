//! Shared quantity grammar over exact decimal values and locale dictionaries.
//!
//! The written surface is kept in the caller's span map. Unit aliases and
//! currency symbols are recognized here once, independent of verbalization.
use super::core::{
    canonical_count, digit, left_boundary, parse_number, right_boundary as boundary,
    scan_numeric_end_for_locale, shift_decimal, skip_space,
};
use super::lexicons::{currency_name, spoken_unit_pattern, unit_forms, unit_pattern};
use super::numbers::{arabic_construct_count, verbalize_with_gender, verbalize_with_options};
use super::types::{
    CurrencyNaming, NumberReading, NumberValue, ReadingStyle, SpeechInterpretation, SpeechLocale,
    SpeechOptions, SpeechValue,
};
use super::verbalize::{
    agreed_number, currency_gender, money, noun_quantity_pattern, plural, quantity,
};
use unicode_normalization::UnicodeNormalization;

/// Match a supported written unit and return its canonical CLDR key.
/// Matching normalizes fullwidth letters but leaves source offsets unchanged.
pub fn unit_alias(text: &str, start: usize) -> Option<(usize, &'static str)> {
    if !text.is_char_boundary(start) {
        return None;
    }
    // Longest match is necessary for `mg` vs `m` and `km/h` vs `km`.
    let mut best = None;
    for (alias, key) in UNIT_ALIASES {
        let mut normalized = String::new();
        for (offset, ch) in text[start..].char_indices().take(48) {
            for n in ch.to_string().nfkc() {
                normalized.extend(n.to_lowercase());
            }
            if normalized == *alias {
                let end = start + offset + ch.len_utf8();
                let source: String = text[start..end].nfkc().collect();
                let key = if sensitive_symbol(&normalized) {
                    exact_symbol(&source)
                } else {
                    Some(*key)
                };
                if let Some(key) = key
                    && boundary(text, end)
                    && best.is_none_or(|(old, _)| end > old)
                {
                    best = Some((end, key));
                }
                break;
            }
            if !alias.starts_with(&normalized) {
                break;
            }
        }
    }
    best
}

/// Match a localized unit noun from the shared CLDR dictionary.
/// Unlike the global SI aliases this uses the caller's declared language.
pub(super) fn localized_unit_alias(
    text: &str,
    start: usize,
    locale: SpeechLocale,
) -> Option<(usize, &'static str)> {
    if !text.is_char_boundary(start) {
        return None;
    }
    let forms = unit_forms(locale);
    let mut normalized = String::new();
    let mut best = None;
    for (offset, ch) in text[start..].char_indices().take(128) {
        let end = start + offset + ch.len_utf8();
        normalized.clear();
        normalized.extend(text[start..end].nfkc().flat_map(char::to_lowercase));
        if boundary(text, end)
            && let Some((_, key)) = forms.iter().find(|(form, _)| form == &normalized)
        {
            let source: String = text[start..end].nfkc().collect();
            if !sensitive_symbol(&normalized) {
                best = Some((end, *key));
            } else if let Some(key) = exact_symbol(&source) {
                best = Some((end, key));
            }
        }
        if !forms.iter().any(|(form, _)| form.starts_with(&normalized))
            && !text[end..]
                .chars()
                .next()
                .is_some_and(crate::core::unicode::is_combining_mark)
        {
            break;
        }
    }
    best
}
pub(super) fn unit_at<'a>(
    text: &str,
    start: usize,
    options: &'a SpeechOptions,
) -> Option<(usize, &'a str)> {
    if let Some(found) = options
        .unit_aliases
        .iter()
        .filter_map(|(alias, key)| {
            let end = start.checked_add(alias.len())?;
            (text.get(start..end) == Some(alias.as_str()) && boundary(text, end))
                .then_some((end, key.as_str()))
        })
        .max_by_key(|(end, _)| *end)
    {
        return Some(found);
    }
    let global = unit_alias(text, start);
    let local = localized_unit_alias(text, start, options.locale);
    match (global, local) {
        (Some(a), Some(b)) => Some(if a.0 >= b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}
/// Whether a canonical quantity key has a noun pattern in this language.
pub fn supports_unit(locale: SpeechLocale, key: &str) -> bool {
    if key.len() > 256 {
        return false;
    }
    if let Some(key) = key.strip_prefix("per:") {
        return supports_unit(locale, key);
    }
    if let Some((a, b)) = key.split_once("/per:") {
        return supports_unit(locale, a) && supports_unit(locale, b);
    }
    unit_pattern(locale, key, "other").is_some()
}

fn sensitive_symbol(folded: &str) -> bool {
    matches!(
        folded,
        "km/h"
            | "m/s"
            | "m²"
            | "m2"
            | "km²"
            | "km2"
            | "cm³"
            | "cm3"
            | "m³"
            | "m3"
            | "mbps"
            | "kbps"
            | "gbps"
            | "mw"
            | "kw"
            | "w"
            | "mhz"
            | "khz"
            | "ghz"
            | "hz"
            | "ml"
            | "mb"
            | "kb"
            | "gb"
            | "tb"
            | "mg"
            | "mm"
            | "m"
            | "g"
            | "s"
            | "h"
            | "a"
            | "v"
            | "k"
    )
}
fn exact_symbol(symbol: &str) -> Option<&'static str> {
    Some(match symbol {
        "km/h" => "speed-kilometer-per-hour",
        "m/s" => "speed-meter-per-second",
        "m²" | "m2" => "area-square-meter",
        "km²" | "km2" => "area-square-kilometer",
        "cm³" | "cm3" => "volume-cubic-centimeter",
        "m³" | "m3" => "volume-cubic-meter",
        "MBps" => "digital-megabyte/per:duration-second",
        "Mbps" | "mbps" => "digital-megabit/per:duration-second",
        "kBps" | "KBps" => "digital-kilobyte/per:duration-second",
        "Kbps" | "kbps" => "digital-kilobit/per:duration-second",
        "GBps" => "digital-gigabyte/per:duration-second",
        "Gbps" | "gbps" => "digital-gigabit/per:duration-second",
        "MW" => "power-megawatt",
        "kW" => "power-kilowatt",
        "W" => "power-watt",
        "MHz" => "frequency-megahertz",
        "kHz" => "frequency-kilohertz",
        "GHz" => "frequency-gigahertz",
        "Hz" => "frequency-hertz",
        "mL" | "ml" => "volume-milliliter",
        "MB" => "digital-megabyte",
        "Mb" => "digital-megabit",
        "kB" | "KB" => "digital-kilobyte",
        "kb" | "Kb" => "digital-kilobit",
        "GB" => "digital-gigabyte",
        "Gb" => "digital-gigabit",
        "TB" => "digital-terabyte",
        "mg" => "mass-milligram",
        "Mg" => "mass-tonne",
        "mm" => "length-millimeter",
        "m" => "length-meter",
        "g" => "mass-gram",
        "s" => "duration-second",
        "h" => "duration-hour",
        "A" => "electric-ampere",
        "V" => "electric-volt",
        "K" => "temperature-kelvin",
        _ => return None,
    })
}

fn is_number_start(text: &str, at: usize) -> bool {
    let mut chars = text[at..].chars();
    chars.next().is_some_and(|c| {
        digit(c).is_some()
            || matches!(c, '+' | '-' | '−')
            || (matches!(c, '.' | ',' | '٫') && chars.next().is_some_and(|c| digit(c).is_some()))
    })
}

/// Parse currency/percentage/measure expressions at one original byte offset.
/// Ambiguous currency symbols require an explicit caller currency convention.
pub(super) fn parse_at(
    text: &str,
    start: usize,
    options: &SpeechOptions,
) -> Option<(usize, SpeechValue)> {
    if !text.is_char_boundary(start) || !left_boundary(text, start) {
        return None;
    }
    if let Some(found) = custom_prefix_unit(text, start, options) {
        return Some(found);
    }
    if let Some((end, value)) = super::expressions::parse_at(text, start, options)
        && let Some((unit_end, unit)) = unit_at(text, skip_space(text, end), options)
    {
        return Some((
            unit_end,
            SpeechValue::MeasuredExpression {
                unit: unit.into(),
                value,
            },
        ));
    }
    if options.interpret_as == SpeechInterpretation::Measure
        && let Some(found) = parse_labelled(text, start, options)
    {
        return Some(found);
    }
    let mut number_start = start;
    let mut prefix_currency = None;
    let mut prefix_percent = false;
    for (alias, code) in CURRENCY_ALIASES {
        if let Some(end) = prefix_alias(text, start, alias) {
            let at = skip_space(text, end);
            if is_number_start(text, at) {
                prefix_currency = Some((*code, *alias));
                number_start = at;
                break;
            }
        }
    }
    if prefix_currency.is_none() && text[start..].starts_with(['%', '٪']) {
        prefix_percent = true;
        number_start = skip_space(text, start + text[start..].chars().next()?.len_utf8());
    }
    let mut negative_word = false;
    if options.locale == SpeechLocale::Ja && text[number_start..].starts_with("マイナス") {
        negative_word = true;
        number_start += "マイナス".len();
    }
    if !is_number_start(text, number_start) {
        return None;
    }
    let mut number_end = scan_numeric_end_for_locale(text, number_start, options.locale);
    if number_end > number_start
        && text[..number_end].ends_with(['/', '-'])
        && unit_at(text, number_end, options).is_some()
    {
        number_end -= 1;
    }
    if number_end <= number_start {
        return None;
    }
    let raw = &text[number_start..number_end];
    // The temporal/fraction grammars own these constructs in their entirety.
    if raw.contains(['/', ':']) || raw.chars().skip(1).any(|c| matches!(c, '+' | '-' | '−')) {
        return None;
    }
    let mut value = match parse_number(raw, options) {
        Ok(value) => value,
        Err(reason) => {
            if prefix_currency.is_some() || prefix_percent {
                return Some((
                    number_end,
                    SpeechValue::Ambiguous {
                        reason: reason.into(),
                    },
                ));
            }
            return None;
        }
    };
    value.negative |= negative_word;
    let mut suffix = skip_space(text, number_end);
    if let Some((end, power)) = scale_at(text, suffix, options.locale) {
        value = scale_value(&value, power)?;
        suffix = skip_space(text, end);
    }
    if options.locale == SpeechLocale::Zh
        && options.interpret_as == SpeechInterpretation::Currency
        && let Some(label @ ('毛' | '角' | '分')) = text[suffix..].chars().next()
    {
        let end = suffix + label.len_utf8();
        if boundary(text, end) {
            if prefix_currency.is_some_and(|(code, symbol)| {
                resolve_currency(code, symbol, options) != Some("CNY")
            }) {
                return Some((
                    end,
                    SpeechValue::Ambiguous {
                        reason: "currency prefix conflicts with the Chinese yuan subunit".into(),
                    },
                ));
            }
            let value = shift_decimal(&value, if label == '分' { -2 } else { -1 })?;
            return Some((
                end,
                SpeechValue::Currency {
                    code: "CNY".into(),
                    value,
                },
            ));
        }
    }
    if let Some((code, symbol)) = prefix_currency {
        let code = resolve_currency(code, symbol, options);
        let end = if suffix > number_end
            && scale_at(text, skip_space(text, number_end), options.locale).is_some()
        {
            suffix
        } else {
            number_end
        };
        if !boundary(text, end) {
            return None;
        }
        return Some((end, currency_value(code, value)));
    }
    if prefix_percent {
        if boundary(text, number_end) {
            return Some((number_end, SpeechValue::Percent { value }));
        }
        return None;
    }
    for (alias, code) in CURRENCY_ALIASES {
        if let Some(end) = prefix_alias(text, suffix, alias)
            && boundary(text, end)
        {
            if *code == "RUB"
                && value.fraction.is_none()
                && let Some((minor_end, minor)) = ruble_minor(text, end, options)
            {
                value.fraction = Some(format!("{minor:02}"));
                return Some((minor_end, currency_value(Some(code), value)));
            }
            return Some((
                end,
                currency_value(resolve_currency(code, alias, options), value),
            ));
        }
    }
    if text[suffix..].starts_with(['%', '٪']) {
        let end = suffix + text[suffix..].chars().next()?.len_utf8();
        if boundary(text, end) {
            return Some((end, SpeechValue::Percent { value }));
        }
    }
    // A slash before the unit means a reciprocal quantity; a slash after a
    // unit means a ratio. Keep both parts typed instead of discarding `/`.
    let reciprocal = text[suffix..].starts_with('/');
    let unit_start = if reciprocal {
        skip_space(text, suffix + 1)
    } else if text[suffix..].starts_with('-') {
        suffix + 1
    } else {
        suffix
    };
    let (end, unit) = unit_at(text, unit_start, options)?;
    let mut unit = if reciprocal {
        format!("per:{unit}")
    } else {
        unit.to_string()
    };
    let mut end = end;
    if text[end..].starts_with('/') {
        let denominator = skip_space(text, end + 1);
        if let Some((den_end, den)) = unit_at(text, denominator, options) {
            unit = format!("{unit}/per:{den}");
            end = den_end;
        } else {
            let unsupported_end = denominator
                + text[denominator..]
                    .char_indices()
                    .take_while(|(_, c)| c.is_alphanumeric() || matches!(c, '_' | '/' | '²' | '³'))
                    .map(|(_, c)| c.len_utf8())
                    .sum::<usize>();
            return Some((
                unsupported_end.max(end + 1),
                SpeechValue::Ambiguous {
                    reason: "unit denominator is unsupported or has ambiguous symbol case".into(),
                },
            ));
        }
    }
    Some((end, SpeechValue::Measure { unit, value }))
}

fn custom_prefix_unit(
    text: &str,
    start: usize,
    options: &SpeechOptions,
) -> Option<(usize, SpeechValue)> {
    let (number_start, key) = options
        .unit_aliases
        .iter()
        .filter_map(|(alias, key)| {
            let end = start.checked_add(alias.len())?;
            if text.get(start..end) != Some(alias.as_str()) {
                return None;
            }
            let number_start = skip_space(text, end);
            is_number_start(text, number_start).then_some((number_start, key))
        })
        .max_by_key(|(end, _)| *end)?;
    let end = scan_numeric_end_for_locale(text, number_start, options.locale);
    let raw = text.get(number_start..end)?;
    if !boundary(text, end) {
        return None;
    }
    let value = match parse_number(raw, options) {
        Ok(value) => SpeechValue::Measure {
            unit: key.clone(),
            value,
        },
        Err(reason) => SpeechValue::Ambiguous {
            reason: reason.into(),
        },
    };
    Some((end, value))
}

fn ruble_minor(text: &str, start: usize, options: &SpeechOptions) -> Option<(usize, u8)> {
    let start = skip_space(text, start);
    let end = scan_numeric_end_for_locale(text, start, options.locale);
    let value = parse_number(&text[start..end], options).ok()?;
    if value.negative || value.fraction.is_some() {
        return None;
    }
    let minor = value.integer.parse::<u8>().ok().filter(|n| *n <= 99)?;
    let suffix = skip_space(text, end);
    for label in ["копеек", "копейки", "копейка", "коп."] {
        let Some(end) = prefix_alias(text, suffix, label) else {
            continue;
        };
        if boundary(text, end) {
            return Some((end, minor));
        }
    }
    None
}

fn parse_labelled(
    text: &str,
    start: usize,
    options: &SpeechOptions,
) -> Option<(usize, SpeechValue)> {
    let label_end = |at: usize| {
        at + text[at..]
            .char_indices()
            .take_while(|(_, c)| c.is_alphabetic() || crate::core::unicode::is_combining_mark(*c))
            .map(|(_, c)| c.len_utf8())
            .sum::<usize>()
    };
    if text[start..].chars().next()?.is_alphabetic() {
        let end = label_end(start);
        if !text[end..].starts_with('-') {
            return None;
        }
        let number_start = end + 1;
        let number_end = scan_numeric_end_for_locale(text, number_start, options.locale);
        let value = parse_number(text.get(number_start..number_end)?, options).ok()?;
        if !boundary(text, number_end) {
            return None;
        }
        return Some((
            number_end,
            SpeechValue::LabelledNumber {
                label: text[start..end].into(),
                value,
                number_first: false,
                separator: "-".into(),
            },
        ));
    }
    let mut number_end = scan_numeric_end_for_locale(text, start, options.locale);
    if number_end <= start {
        return None;
    }
    let label_start = if text[..number_end].ends_with('-') {
        number_end -= 1;
        number_end + 1
    } else {
        skip_space(text, number_end)
    };
    let end = label_end(label_start);
    if end == label_start || !boundary(text, end) {
        return None;
    }
    let label = &text[label_start..end];
    if label.chars().count() > super::dictionary::MAX_ENTRY_CHARS {
        return None;
    }
    // Known physical units keep their semantic unit code. Ambiguous SI case
    // cannot be disguised as a literal label merely by supplying a hint.
    if unit_at(text, label_start, options).is_some() {
        return None;
    }
    // A scale or currency token belongs to the quantity parser below. A
    // literal-label hint must not consume only the first word of that amount.
    if scale_at(text, label_start, options.locale).is_some()
        || CURRENCY_ALIASES
            .iter()
            .any(|(alias, _)| label.eq_ignore_ascii_case(alias))
    {
        return None;
    }
    let folded: String = label.nfkc().flat_map(char::to_lowercase).collect();
    let explicit_letter_label = &text[number_end..label_start] == "-" && label.chars().count() == 1;
    if sensitive_symbol(&folded) && !explicit_letter_label {
        return None;
    }
    let value = parse_number(&text[start..number_end], options).ok()?;
    Some((
        end,
        SpeechValue::LabelledNumber {
            label: label.into(),
            value,
            number_first: true,
            separator: text[number_end..label_start].into(),
        },
    ))
}

fn prefix_alias(text: &str, start: usize, alias: &str) -> Option<usize> {
    let end = start.checked_add(alias.len())?;
    let candidate = text.get(start..end)?;
    (candidate.eq_ignore_ascii_case(alias)).then_some(end)
}
fn resolve_currency<'a>(
    code: &'a str,
    symbol: &str,
    options: &'a SpeechOptions,
) -> Option<&'a str> {
    if code != "?" {
        return Some(code);
    }
    if let Some(code) = options.currency_symbols.get(symbol) {
        return Some(code);
    }
    // Once a caller supplies symbol-specific policy, an unmapped symbol must
    // stay unresolved rather than inheriting a different symbol's currency.
    if options.currency_symbols.is_empty() {
        options.default_currency.as_deref()
    } else {
        None
    }
}

fn currency_value(code: Option<&str>, value: NumberValue) -> SpeechValue {
    match code {
        Some(code) => SpeechValue::Currency {
            code: code.into(),
            value,
        },
        None => SpeechValue::Ambiguous {
            reason: "currency symbol needs an explicit ISO currency code".into(),
        },
    }
}
fn scale_at(text: &str, start: usize, locale: SpeechLocale) -> Option<(usize, usize)> {
    let scales: &[(&str, usize)] = match locale {
        SpeechLocale::En => &[
            ("trillion", 12),
            ("billion", 9),
            ("million", 6),
            ("thousand", 3),
            ("bn", 9),
            ("b", 9),
        ],
        SpeechLocale::Es => &[
            ("billones", 12),
            ("billón", 12),
            ("millones", 6),
            ("millón", 6),
            ("mil", 3),
        ],
        SpeechLocale::Pt => &[
            ("bilhões", 9),
            ("bilhão", 9),
            ("milhões", 6),
            ("milhão", 6),
            ("mil", 3),
        ],
        SpeechLocale::De => &[
            ("milliarden", 9),
            ("milliarde", 9),
            ("millionen", 6),
            ("million", 6),
        ],
        SpeechLocale::Ru => &[("млрд", 9), ("млн", 6), ("тыс", 3)],
        SpeechLocale::Ko => &[("조", 12), ("억", 8), ("만", 4)],
        SpeechLocale::Zh => &[("亿", 8), ("万", 4)],
        SpeechLocale::Ja => &[("兆", 12), ("億", 8), ("万", 4)],
        _ => &[],
    };
    scales.iter().find_map(|(alias, power)| {
        let end = prefix_alias(text, start, alias)?;
        boundary(text, end).then_some((end, *power))
    })
}
fn scale_value(value: &NumberValue, power: usize) -> Option<NumberValue> {
    shift_decimal(value, i32::try_from(power).ok()?)
}

/// Render a quantity through shared numeric rules and locale noun patterns.
/// `source` is used only to retain explicit lexical choices, never as a lookup
/// key for complete input/output fixture pairs.
pub(super) fn render(value: &SpeechValue, source: &str, options: &SpeechOptions) -> Option<String> {
    match value {
        SpeechValue::MeasuredExpression { unit, value } => {
            render_measured_expression(unit, value, options)
        }
        SpeechValue::LabelledNumber {
            label,
            value,
            number_first,
            separator,
        } => {
            let words = verbalize_with_options(value, options, NumberReading::Cardinal)?;
            let separator = if options.reading_style == ReadingStyle::Natural
                || separator.is_empty()
                || separator.chars().all(char::is_whitespace)
            {
                " "
            } else {
                separator
            };
            Some(if *number_first {
                format!("{words}{separator}{label}")
            } else {
                format!("{label}{separator}{words}")
            })
        }
        SpeechValue::Measure { unit, value } => {
            render_written_measure(unit, value, source, options)
                .or_else(|| render_measure(unit, &canonical_count(value), options))
        }
        SpeechValue::Currency { code, value } => render_money(code, value, source, options),
        SpeechValue::Percent { value } => {
            let value = canonical_count(value);
            let words = verbalize_with_options(&value, options, NumberReading::Cardinal)?;
            if options.locale == SpeechLocale::Ar && options.reading_style != ReadingStyle::Precise
            {
                Some(format!("{words} في المائة"))
            } else {
                quantity(options.locale, "concentr-percent", &value)
            }
        }
        _ => None,
    }
}
fn render_measured_expression(
    unit: &str,
    value: &super::expressions::ExpressionValue,
    options: &SpeechOptions,
) -> Option<String> {
    use super::expressions::{self, ExpressionValue};
    // Keep public AST validation centralized before composing unit operands.
    let validated = expressions::render(value, options)?;
    let (one_pattern, gender) = unit_pattern(options.locale, unit, "one")?;
    if let ExpressionValue::Scientific { mantissa, exponent } = value {
        let mantissa = render_measure(unit, mantissa, options)?;
        let exponent = NumberValue {
            negative: *exponent < 0,
            integer: exponent.unsigned_abs().to_string(),
            fraction: None,
        };
        let exponent = verbalize_with_options(&exponent, options, NumberReading::Cardinal)?;
        return Some(expressions::scientific_template(
            options.locale,
            &mantissa,
            &exponent,
        ));
    }
    if matches!(
        value,
        ExpressionValue::Fraction { .. } | ExpressionValue::MixedFraction { .. }
    ) {
        if options.locale == SpeechLocale::En {
            return english_fraction_measure(unit, value, one_pattern, options);
        }
        if options.locale == SpeechLocale::Es
            && let Some(words) = spanish_fraction_measure(
                unit,
                value,
                one_pattern,
                gender == Some("feminine"),
                options,
            )
        {
            return Some(words);
        }
        // Formal fallback places the physical unit in the numerator operand;
        // appending a noun after the divisor would incorrectly bind it there.
        return formal_fraction_measure(unit, value, options);
    }
    // A duration already carries units and is not a scalar multiplier of an
    // arbitrary second unit. Preserve that unsupported composition.
    if matches!(value, ExpressionValue::Duration { .. }) {
        return None;
    }
    let pattern = unit_pattern(options.locale, unit, "other")?.0;
    Some(
        pattern
            .replace("{0}", &validated)
            .replace(['\u{a0}', '\u{202f}'], " "),
    )
}

fn formal_fraction_measure(
    unit: &str,
    value: &super::expressions::ExpressionValue,
    options: &SpeechOptions,
) -> Option<String> {
    use super::expressions;
    let (numerator, denominator) = expressions::rational_components(value)?;
    let numerator = render_measure(unit, &numerator, options)?;
    let denominator = verbalize_with_options(&denominator, options, NumberReading::Cardinal)?;
    Some(expressions::measured_division(
        options.locale,
        &numerator,
        &denominator,
    ))
}

fn spanish_fraction_measure(
    unit: &str,
    value: &super::expressions::ExpressionValue,
    one_pattern: &str,
    feminine: bool,
    options: &SpeechOptions,
) -> Option<String> {
    use super::expressions::{self, ExpressionValue};
    if options.reading_style != ReadingStyle::Natural {
        return None;
    }
    let (whole, numerator, denominator) = match value {
        ExpressionValue::Fraction {
            numerator,
            denominator,
        } => (None, numerator, denominator),
        ExpressionValue::MixedFraction {
            whole,
            numerator,
            denominator,
        } => (Some(whole), numerator, denominator),
        _ => return None,
    };
    if denominator.fraction.is_some()
        || denominator.negative
        || numerator.fraction.is_some()
        || !denominator
            .integer
            .parse::<u64>()
            .is_ok_and(|n| (2..=10).contains(&n))
    {
        return None;
    }
    let half =
        canonical_count(numerator).integer == "1" && canonical_count(denominator).integer == "2";
    if let Some(whole) = whole {
        if whole.integer.bytes().all(|c| c == b'0') {
            let mut numerator = numerator.clone();
            numerator.negative |= whole.negative;
            return spanish_fraction_measure(
                unit,
                &ExpressionValue::Fraction {
                    numerator,
                    denominator: denominator.clone(),
                },
                one_pattern,
                feminine,
                options,
            );
        }
        if half {
            // Medio is adjectival here: the whole count controls the noun's
            // number, and medio agrees with that noun's gender. For rates,
            // retain the denominator's scope: una milla y media por hora.
            // FundéuRAE: https://www.fundeu.es/consulta/el-punto-o-la-coma-en-la-escritura-de-las-horas-432/
            let mut positive = whole.clone();
            positive.negative = false;
            let quantity = render_measure(unit, &positive, options)?;
            let half = if feminine { "media" } else { "medio" };
            let quantity = if let Some((numerator, denominator)) = quantity.split_once(" por ") {
                format!("{numerator} y {half} por {denominator}")
            } else {
                format!("{quantity} y {half}")
            };
            return Some(if whole.negative {
                format!("menos {quantity}")
            } else {
                quantity
            });
        }
        // Keep the scope of a mixed negative explicit in the formal fallback.
        if whole.negative {
            return None;
        }
    } else if half {
        let words = expressions::render_with_gender(value, options, feminine)?;
        return Some(one_pattern.replace("{0}", &words));
    }
    // With de + unit, the fraction is a mathematical noun (tres medios de
    // hora), not an adjective agreeing with hora (media hora).
    // https://tulengua.iatext.ulpgc.es/es/convertir-numeros-a-texto-letras-fraccionarios.html
    let words = expressions::render_with_gender(value, options, false)?;
    let noun = one_pattern.strip_prefix("{0}")?.trim();
    Some(format!("{words} de {noun}"))
}

fn english_fraction_measure(
    unit: &str,
    value: &super::expressions::ExpressionValue,
    one_pattern: &str,
    options: &SpeechOptions,
) -> Option<String> {
    use super::expressions::{self, ExpressionValue};
    if options.reading_style != ReadingStyle::Natural {
        return formal_fraction_measure(unit, value, options);
    }
    match value {
        ExpressionValue::MixedFraction {
            whole,
            numerator,
            denominator,
        } => {
            if whole.integer.bytes().all(|b| b == b'0') {
                let mut numerator = numerator.clone();
                numerator.negative |= whole.negative;
                return english_fraction_measure(
                    unit,
                    &ExpressionValue::Fraction {
                        numerator,
                        denominator: denominator.clone(),
                    },
                    one_pattern,
                    options,
                );
            }
            let words = expressions::render(value, options)?;
            let pattern = unit_pattern(options.locale, unit, "other")?.0;
            return Some(pattern.replace("{0}", &words));
        }
        ExpressionValue::Fraction {
            numerator,
            denominator,
        } => {
            let named = numerator.fraction.is_none()
                && denominator.fraction.is_none()
                && !denominator.negative;
            if named {
                if denominator.integer == "1" {
                    return render_measure(unit, numerator, options);
                }
                let words = expressions::render(value, options)?;
                let noun = one_pattern.strip_prefix("{0}")?.trim();
                if numerator.integer == "1" && denominator.integer == "2" {
                    return Some(format!("{words} {noun}"));
                }
                let first = noun.split_whitespace().next()?.to_ascii_lowercase();
                let article = if first == "hour" || first.starts_with(['a', 'e', 'i', 'o', 'u']) {
                    "an"
                } else {
                    "a"
                };
                return Some(format!("{words} of {article} {noun}"));
            }
        }
        _ => return None,
    }
    formal_fraction_measure(unit, value, options)
}

pub(super) fn render_measure(
    unit: &str,
    value: &NumberValue,
    options: &SpeechOptions,
) -> Option<String> {
    if let Some(denominator) = unit.strip_prefix("per:") {
        return render_per(denominator, value, None, options);
    }
    if let Some((numerator, denominator)) = unit.split_once("/per:") {
        let numerator = render_measure(numerator, value, options)?;
        return render_per(denominator, value, Some(&numerator), options);
    }
    let unit = super::verbalize::unit_key(unit).unwrap_or(unit);
    if options.reading_style == ReadingStyle::Precise
        && let Some(result) = quantity(options.locale, unit, value)
    {
        return Some(result);
    }
    let category = plural(options.locale, value)?;
    let (pattern, gender) = if options.reading_style == ReadingStyle::Natural {
        spoken_unit_pattern(options.locale, unit, category)?
    } else {
        unit_pattern(options.locale, unit, category)?
    };
    if !pattern.contains("{0}") {
        return quantity(options.locale, unit, value);
    }
    let words = if gender == Some("feminine")
        && matches!(
            options.locale,
            SpeechLocale::Es | SpeechLocale::Pt | SpeechLocale::Ru
        ) {
        verbalize_with_gender(value, options, gender)?
    } else if options.reading_style == ReadingStyle::Precise
        || (matches!(
            options.locale,
            SpeechLocale::Es
                | SpeechLocale::Pt
                | SpeechLocale::Fr
                | SpeechLocale::De
                | SpeechLocale::Ru
                | SpeechLocale::Ar
        ) && value.fraction.is_none())
    {
        agreed_number(options.locale, value, gender)?
    } else {
        verbalize_with_options(value, options, NumberReading::Cardinal)?
    };
    let result = noun_quantity_pattern(options.locale, value, pattern, &words)
        .replace(['\u{a0}', '\u{202f}'], " ");
    Some(spoken_unit_spacing(result, &words, options))
}
fn spoken_unit_spacing(result: String, number: &str, options: &SpeechOptions) -> String {
    if options.reading_style != ReadingStyle::Natural {
        return result;
    }
    match options.locale {
        SpeechLocale::Zh | SpeechLocale::Ja => result.replace(' ', ""),
        SpeechLocale::Ko => {
            if let Some(suffix) = result.strip_prefix(number) {
                format!("{number} {}", suffix.trim_start())
            } else {
                result
            }
        }
        _ => result,
    }
}

fn render_per(
    denominator: &str,
    value: &NumberValue,
    numerator: Option<&str>,
    options: &SpeechOptions,
) -> Option<String> {
    let (pattern, _) = unit_pattern(options.locale, denominator, "one")?;
    let denominator = pattern.replace("{0}", "").trim().to_string();
    let words = match numerator {
        Some(s) => s.to_string(),
        None => verbalize_with_options(value, options, NumberReading::Cardinal)?,
    };
    let connective = match options.locale {
        SpeechLocale::En => "per",
        SpeechLocale::Es | SpeechLocale::Pt => "por",
        SpeechLocale::Fr => "par",
        SpeechLocale::De => "pro",
        SpeechLocale::Ru => "на",
        SpeechLocale::Hi => "प्रति",
        SpeechLocale::Ar => "لكل",
        SpeechLocale::Bn => "প্রতি",
        SpeechLocale::Ur => "فی",
        SpeechLocale::Id => "per",
        SpeechLocale::Pa => "ਪ੍ਰਤੀ",
        SpeechLocale::Mr => "प्रति",
        SpeechLocale::Te => "ప్రతి",
        SpeechLocale::Tr => "bölü",
        SpeechLocale::Ta => "ஒரு",
        SpeechLocale::Vi => "trên",
        SpeechLocale::Ko => "매",
        SpeechLocale::Zh => "每",
        SpeechLocale::Ja => "毎",
    };
    Some(format!("{words} {connective} {denominator}"))
}
fn written_scale(
    value: &NumberValue,
    source: &str,
    options: &SpeechOptions,
) -> Option<(String, usize)> {
    let digit_start = source.char_indices().find(|(_, c)| digit(*c).is_some())?.0;
    let start = source[..digit_start]
        .char_indices()
        .next_back()
        .filter(|(_, c)| matches!(c, '-' | '−' | '+'))
        .map_or(digit_start, |(at, _)| at);
    let number_end = scan_numeric_end_for_locale(source, start, options.locale);
    let raw = parse_number(&source[start..number_end], options).ok()?;
    let scale_start = skip_space(source, number_end);
    let (scale_end, power) = scale_at(source, scale_start, options.locale)?;
    if &scale_value(&raw, power)? != value {
        return None;
    }
    let mut words = verbalize_with_options(&raw, options, NumberReading::Cardinal)?;
    if raw.fraction.is_none() && options.locale == SpeechLocale::Ru && power == 3 {
        words = agreed_number(options.locale, &raw, Some("feminine"))?;
    } else if raw.fraction.is_none()
        && matches!(options.locale, SpeechLocale::Es | SpeechLocale::Pt)
        && power >= 6
    {
        words = agreed_number(options.locale, &raw, Some("masculine"))?;
    }
    let written = &source[scale_start..scale_end];
    let scale = match options.locale {
        SpeechLocale::En if matches!(written.to_ascii_lowercase().as_str(), "b" | "bn") => {
            "billion".to_string()
        }
        SpeechLocale::Ru => {
            let n = raw.integer.parse::<u64>().ok()?;
            let form = if raw.fraction.is_some() {
                1
            } else {
                match (n % 10, n % 100) {
                    (1, n) if n != 11 => 0,
                    (2..=4, n) if !(12..=14).contains(&n) => 1,
                    _ => 2,
                }
            };
            let forms = match power {
                3 => ["тысяча", "тысячи", "тысяч"],
                6 => ["миллион", "миллиона", "миллионов"],
                9 => ["миллиард", "миллиарда", "миллиардов"],
                _ => return None,
            };
            forms[form].to_string()
        }
        _ => written.to_string(),
    };
    let gap = if matches!(
        options.locale,
        SpeechLocale::Zh | SpeechLocale::Ja | SpeechLocale::Ko
    ) {
        ""
    } else {
        " "
    };
    Some((format!("{words}{gap}{scale}"), power))
}
fn render_written_measure(
    unit: &str,
    value: &NumberValue,
    source: &str,
    options: &SpeechOptions,
) -> Option<String> {
    let (number, _) = written_scale(value, source, options)?;
    let key = super::verbalize::unit_key(unit).unwrap_or(unit);
    let (pattern, _) = unit_pattern(options.locale, key, plural(options.locale, value)?)?;
    Some(
        noun_quantity_pattern(options.locale, value, pattern, &number)
            .replace(['\u{a0}', '\u{202f}'], " "),
    )
}
fn render_written_scale(
    code: &str,
    value: &NumberValue,
    source: &str,
    options: &SpeechOptions,
) -> Option<String> {
    let (number, power) = written_scale(value, source, options)?;
    let currency = if options.currency_naming == CurrencyNaming::Full {
        // A Russian scale governs a genitive plural currency noun even when
        // its own coefficient is fractional: два миллиона долларов США.
        full_currency_name(
            code,
            if options.locale == SpeechLocale::Ru {
                "many"
            } else {
                "other"
            },
            options,
        )?
    } else if options.reading_style == ReadingStyle::Precise {
        currency_name(options.locale, code, "other")?
    } else {
        short_currency_name(options.locale, code, false)
            .or_else(|| currency_name(options.locale, code, "other"))?
    };
    let connector = if matches!(options.locale, SpeechLocale::Es | SpeechLocale::Pt) && power >= 6 {
        " de "
    } else if matches!(
        options.locale,
        SpeechLocale::Zh | SpeechLocale::Ja | SpeechLocale::Ko
    ) {
        ""
    } else {
        " "
    };
    Some(format!("{number}{connector}{currency}"))
}

fn render_money(
    code: &str,
    value: &NumberValue,
    source: &str,
    options: &SpeechOptions,
) -> Option<String> {
    let mut amount = render_money_amount(code, value, source, options)?;
    // Retain an explicitly written country qualifier. Attach it once, after
    // the complete major/minor or scaled amount, rather than inside recursion.
    // Precise CLDR currency names already contain the written abbreviation.
    if options.locale == SpeechLocale::Ru
        && code == "USD"
        && (options.currency_naming == CurrencyNaming::Full
            || source.match_indices("США").any(|(start, written)| {
                left_boundary(source, start) && boundary(source, start + written.len())
            }))
    {
        if let Some(short) = amount.strip_suffix(" США") {
            amount = short.to_string();
        }
        amount.push_str(" сэ шэ а");
    }
    Some(amount)
}

fn render_money_amount(
    code: &str,
    value: &NumberValue,
    source: &str,
    options: &SpeechOptions,
) -> Option<String> {
    if code == "CNY"
        && options.locale == SpeechLocale::Zh
        && options.currency_naming == CurrencyNaming::Style
        && let Some(label @ ('毛' | '角' | '分')) = source.chars().next_back()
    {
        let raw = source[..source.len() - label.len_utf8()].trim();
        if let Ok(minor) = parse_number(raw, options)
            && shift_decimal(&minor, if label == '分' { -2 } else { -1 }).as_ref() == Some(value)
        {
            return Some(format!(
                "{}{label}",
                verbalize_with_options(&minor, options, NumberReading::Cardinal)?
            ));
        }
    }
    let canonical = canonical_count(value);
    let value = &canonical;
    if options.locale == SpeechLocale::Ar && options.currency_naming == CurrencyNaming::Full {
        // Display names do not establish Arabic count inflection. A full
        // name must also have a reviewed noun paradigm and qualifier form.
        arabic_full_currency_forms(code)?;
    }
    let arabic_feminine = options.locale == SpeechLocale::Ar && arabic_currency_is_feminine(code);
    // Digit-by-digit decimal readings do not agree with the currency noun:
    // فاصلة is a separator, and each side is read as an independent numeral.
    // Rational natural readings are likewise governed by their denominator,
    // not by the lexical gender of the currency being measured.
    if let Some(scaled) = render_written_scale(code, value, source, options) {
        return Some(scaled);
    }
    if options.reading_style == ReadingStyle::Precise
        && options.locale != SpeechLocale::Ar
        && options.currency_naming == CurrencyNaming::Style
    {
        return money(options.locale, code, value);
    }
    let locale = options.locale;
    let one = value.integer == "1"
        && value
            .fraction
            .as_ref()
            .is_none_or(|f| f.bytes().all(|b| b == b'0'));
    let mut plural_value = value.clone();
    if plural_value
        .fraction
        .as_ref()
        .is_some_and(|f| f.bytes().all(|b| b == b'0'))
    {
        plural_value.fraction = None;
    }
    let category = plural(locale, &plural_value)?;
    let name = if options.currency_naming == CurrencyNaming::Full {
        full_currency_name(code, category, options)?
    } else {
        match (locale, code, category) {
            (SpeechLocale::Ru, "RUB", "one") => "рубль",
            (SpeechLocale::Ru, "RUB", "few") => "рубля",
            (SpeechLocale::Ru, "RUB", _) => "рублей",
            (SpeechLocale::Ru, "USD", "one") => "доллар",
            (SpeechLocale::Ru, "USD", "few") => "доллара",
            (SpeechLocale::Ru, "USD", _) => "долларов",
            (SpeechLocale::Zh, "CNY", _) if source.trim_end().ends_with('元') => "元",
            _ => short_currency_name(locale, code, one)
                .or_else(|| currency_name(locale, code, category))?,
        }
    };
    let fraction = value
        .fraction
        .as_deref()
        .filter(|f| f.bytes().any(|b| b != b'0'));
    // Yen/won have no ordinary circulating minor denomination. Never invent
    // a fractional subunit for them.
    if matches!(code, "JPY" | "KRW") && fraction.is_some() {
        return None;
    }
    if let Some(fraction) = fraction
        && fraction.len() <= 2
        && let Some(minor_name) = minor_name(locale, code)
    {
        let mut major = value.clone();
        major.fraction = None;
        major.negative = false;
        let minor = format!("{fraction:0<2}").parse::<u32>().ok()?;
        let minor_value = NumberValue {
            negative: false,
            integer: minor.to_string(),
            fraction: None,
        };
        let minor_words = if matches!(
            locale,
            SpeechLocale::Es
                | SpeechLocale::Pt
                | SpeechLocale::De
                | SpeechLocale::Fr
                | SpeechLocale::Ru
        ) {
            let gender = if locale == SpeechLocale::Ru && code == "RUB" {
                "feminine"
            } else {
                "masculine"
            };
            agreed_number(locale, &minor_value, Some(gender))?
        } else {
            verbalize_with_options(&minor_value, options, NumberReading::Cardinal)?
        };
        let minor_name = if locale == SpeechLocale::Ru && matches!(code, "RUB" | "USD") {
            let forms = if code == "RUB" {
                ["копейка", "копейки", "копеек"]
            } else {
                ["цент", "цента", "центов"]
            };
            match (minor % 10, minor % 100) {
                (1, n) if n != 11 => forms[0],
                (2..=4, n) if !(12..=14).contains(&n) => forms[1],
                _ => forms[2],
            }
        } else if minor == 1 || (options.locale == SpeechLocale::Ar && !(3..=10).contains(&minor)) {
            minor_name.0
        } else {
            minor_name.1
        };
        let cents = if locale == SpeechLocale::Ar {
            arabic_noun_count(
                ["سنت", "سنتان", "سنتات", "سنتا"],
                &minor_value,
                options,
                false,
            )?
        } else {
            format!("{minor_words} {minor_name}")
        };
        let joined = if major.integer == "0" && options.currency_naming == CurrencyNaming::Style {
            cents
        } else {
            let mut major_text = render_money_amount(code, &major, source, options)?;
            let connector = match locale {
                SpeechLocale::Pt => " e ",
                SpeechLocale::En
                | SpeechLocale::Es
                | SpeechLocale::De
                | SpeechLocale::Ru
                | SpeechLocale::Vi => " ",
                SpeechLocale::Ar => " و",
                _ => " ",
            };
            major_text.push_str(connector);
            major_text.push_str(&cents);
            major_text
        };
        return Some(if value.negative {
            format!("{} {joined}", sign(locale))
        } else {
            joined
        });
    }
    if locale == SpeechLocale::Ar && fraction.is_some() {
        // A decimal tail is read independently of the counted currency noun.
        // Use the reviewed singular currency phrase rather than an invariant
        // CLDR label, which in particular cannot express the feminine taka
        // and naira terminology or the irregular shekel name.
        let singular = if options.currency_naming == CurrencyNaming::Full {
            arabic_full_currency_forms(code)?[0].clone()
        } else if let Some(forms) = arabic_currency_forms(code) {
            forms[0].to_string()
        } else {
            name.to_string()
        };
        let words = verbalize_with_options(value, options, NumberReading::Cardinal)?;
        return Some(format!("{words} {singular}"));
    }
    if locale == SpeechLocale::Ar && plural_value.fraction.is_none() {
        if options.currency_naming == CurrencyNaming::Full {
            let forms = arabic_full_currency_forms(code)?;
            return arabic_noun_count(
                [&forms[0], &forms[1], &forms[2], &forms[3]],
                &plural_value,
                options,
                arabic_feminine,
            );
        }
        if arabic_feminine {
            return arabic_currency_major(code, &plural_value, options);
        }
        if let Some(result) = arabic_currency_major(code, &plural_value, options) {
            return Some(result);
        }
        if matches!(plural_value.integer.as_str(), "1" | "2") {
            return None;
        }
    }
    let mut words = if plural_value.fraction.is_none()
        && matches!(
            locale,
            SpeechLocale::Es
                | SpeechLocale::Pt
                | SpeechLocale::Fr
                | SpeechLocale::De
                | SpeechLocale::Ru
        ) {
        agreed_number(locale, &plural_value, currency_gender(locale, code))?
    } else {
        verbalize_with_options(&plural_value, options, NumberReading::Cardinal)?
    };
    if locale == SpeechLocale::Es {
        if words.ends_with(" uno") {
            words.truncate(words.len() - 4);
            words.push_str(" un");
        } else if words == "uno" {
            words = "un".into();
        }
    }
    let tight = matches!(
        locale,
        SpeechLocale::Zh | SpeechLocale::Ja | SpeechLocale::Ko
    );
    let _ = source;
    if tight {
        Some(format!("{words}{name}"))
    } else {
        Some(noun_quantity_pattern(
            locale,
            &plural_value,
            &format!("{{0}} {name}"),
            &words,
        ))
    }
}

fn full_currency_name(code: &str, category: &str, options: &SpeechOptions) -> Option<&'static str> {
    let name = currency_name(options.locale, code, category)?;
    // The enclosing money renderer pronounces this qualifier once after the
    // complete amount, including recursive minor-unit composition.
    if options.locale == SpeechLocale::Ru && code == "USD" {
        name.strip_suffix(" США")
    } else {
        Some(name)
    }
}

fn arabic_full_currency_forms(code: &str) -> Option<[String; 4]> {
    if code == "HKD" {
        // هونغ كونغ is an invariable proper-place qualifier, not a nisba
        // adjective competing for agreement with دولار. CLDR supplies the
        // lexical name; the reviewed dollar noun paradigm supplies its forms.
        return Some(arabic_currency_forms("USD")?.map(|form| format!("{form} هونغ كونغ")));
    }
    if code == "ZAR" {
        // جنوب أفريقيا is likewise a fixed proper-country name. UNTERM's
        // South Africa record gives masculine راند and plural راندات.
        // https://unterm.un.org/unterm2/view/unhq/e3b4a626-fd77-4083-b30f-d6c1dca3a910
        return Some(arabic_currency_forms("ZAR")?.map(|form| format!("{form} جنوب أفريقيا")));
    }
    if matches!(code, "EUR" | "KRW" | "BDT" | "NGN" | "THB" | "ILS") {
        // These paradigms have irregular plurals, invariant country phrases,
        // multi-adjective names, or authoritative spellings that cannot be
        // decomposed into the productive single-nisba rule below.
        return Some(arabic_currency_forms(code)?.map(str::to_string));
    }
    let name = currency_name(SpeechLocale::Ar, code, "other")?;
    let (noun, qualifier) = name.split_once(' ')?;
    // Bound the productive nisba adjective rule to a single reviewed
    // adjective. Complex country phrases and unknown noun paradigms remain
    // unsupported rather than receiving invented agreement.
    if qualifier.contains(char::is_whitespace) {
        return None;
    }
    let feminine = arabic_currency_is_feminine(code);
    let qualifier = if feminine {
        qualifier
            .strip_suffix("ية")
            .or_else(|| qualifier.strip_suffix('ي'))?
    } else {
        qualifier.strip_suffix('ي')?
    };
    let base = [
        "USD", "GBP", "KWD", "SAR", "AED", "RUB", "CHF", "INR", "TRY", "PEN", "JPY", "CNY", "MYR",
        "VND", "MXN", "PLN",
    ]
    .into_iter()
    .filter_map(arabic_currency_forms)
    .find(|forms| forms[0].split(' ').next() == Some(noun))?;
    let noun = base.map(|form| form.split(' ').next().unwrap_or(form));
    // Shared CLDR currency names supply the lexical country adjective. Only
    // its grammatical endings are generated: e.g. CLDR Arabic EGP already
    // documents مصري / مصريان / مصرية / مصريًا for these count categories.
    // https://raw.githubusercontent.com/unicode-org/cldr-json/48.0.0/cldr-json/cldr-numbers-full/main/ar/currencies.json
    Some(if feminine {
        [
            format!("{} {qualifier}ية", noun[0]),
            format!("{} {qualifier}يتان", noun[1]),
            format!("{} {qualifier}ية", noun[2]),
            format!("{} {qualifier}ية", noun[3]),
        ]
    } else {
        [
            name.into(),
            format!("{} {qualifier}يان", noun[1]),
            format!("{} {qualifier}ية", noun[2]),
            format!("{} {qualifier}يا", noun[3]),
        ]
    })
}

fn arabic_currency_is_feminine(code: &str) -> bool {
    matches!(code, "INR" | "PKR" | "IDR" | "TRY" | "BDT" | "NGN")
}

/// A regional spoken alternative, never the default money renderer. Gadalla
/// (2000), §1 example 9, documents long numerals 3–10 + masculine singular unit
/// in Egyptian Colloquial Arabic; Holes (2016), §3.8 p.137, attests singular
/// dinar among Bahraini Baḥārna speakers. Only reviewed noun paradigms enter
/// this construction; existing naming policy supplies the complete name.
pub(super) fn regional_arabic_currency_unit_variant(
    typed: &SpeechValue,
    options: &SpeechOptions,
) -> Option<String> {
    let SpeechValue::Currency { code, value } = typed else {
        return None;
    };
    if options.locale != SpeechLocale::Ar
        || options.reading_style == ReadingStyle::Digits
        || value.fraction.is_some()
        || arabic_currency_is_feminine(code)
        || !(3..=10).contains(&value.integer.parse::<u64>().ok()?)
    {
        return None;
    }
    let name = if options.currency_naming == CurrencyNaming::Full {
        arabic_full_currency_forms(code)?[0].clone()
    } else if let Some(forms) = arabic_currency_forms(code) {
        forms[0].to_string()
    } else {
        // The same reviewed masculine noun can underlie another ISO code,
        // such as Canadian dollars. Keep its lexical country qualifier.
        arabic_full_currency_forms(code)?[0].clone()
    };
    let words = verbalize_with_options(&canonical_count(value), options, NumberReading::Cardinal)?;
    Some(format!("{words} {name}"))
}

fn arabic_currency_major(
    code: &str,
    value: &NumberValue,
    options: &SpeechOptions,
) -> Option<String> {
    // Standalone (nominative) currency quantities. Small-number agreement is
    // explicit; display names alone do not encode Arabic one/two/3–10 forms.
    arabic_noun_count(
        arabic_currency_forms(code)?,
        value,
        options,
        arabic_currency_is_feminine(code),
    )
}

fn arabic_one_two_count(
    one: &str,
    dual: &str,
    value: &NumberValue,
    feminine: bool,
) -> Option<String> {
    if value.fraction.is_some() {
        return None;
    }
    let result = match value.integer.parse::<u64>().ok()? {
        1 => format!("{one} {}", if feminine { "واحدة" } else { "واحد" }),
        2 => dual.to_string(),
        _ => return None,
    };
    Some(if value.negative {
        format!("سالب {result}")
    } else {
        result
    })
}

fn arabic_currency_forms(code: &str) -> Option<[&'static str; 4]> {
    Some(match code {
        // Other dollar currencies share the same unqualified noun paradigm in
        // the short style. Their qualifiers belong only to full naming.
        "USD" | "CAD" | "AUD" | "NZD" | "SGD" | "TWD" | "HKD" => {
            ["دولار", "دولاران", "دولارات", "دولارا"]
        }
        // Egyptian and pound-sterling style names share the unqualified noun
        // جنيه; full naming derives the country adjective separately.
        "GBP" | "EGP" => ["جنيه", "جنيهان", "جنيهات", "جنيها"],
        "KWD" => [
            "دينار كويتي",
            "ديناران كويتيان",
            "دنانير كويتية",
            "دينارا كويتيا",
        ],
        "SAR" | "BRL" => ["ريال", "ريالان", "ريالات", "ريالا"],
        "AED" => ["درهم", "درهمان", "دراهم", "درهما"],
        // Masculine noun and plural: Birzeit Qabas, lemma 2023241124;
        // SKIU / Pyatigorsk State University, Arabic textbook part 2 (2024),
        // lesson 36, gives رُوبِل / رُوبْلَات. Country adjectives still come from
        // CLDR through arabic_full_currency_forms, not this noun paradigm.
        // https://sina.birzeit.edu/qabas/lemma/2023241124
        // https://skiu.ru/wp-content/uploads/2024/12/uchebnoe-posobie-arabskij-yazyk-v-religioznoj-sfere.-2-chast.pdf
        "RUB" => ["روبل", "روبلان", "روبلات", "روبلا"],
        // WIPO's own Arabic fee schedules attest the masculine noun, dual,
        // plural and accusative, with سويسري agreement in each category.
        // https://www.wipo.int/ar/web/hague-system/hague-system-certified-documents
        // https://www.wipo.int/edocs/mdocs/govbody/ar/wo_pbc_6/wo_pbc_6_2-appendix3.pdf
        // https://www.wipo.int/ar/web/hague-system/w/news/2024/news_0004
        "CHF" => ["فرنك", "فرنكان", "فرنكات", "فرنكا"],
        // Reviewed feminine nouns; ة has no extra accusative alif. MANUU's
        // Arabic Grammar and Communication-I (2021) documents روبية/روبيات;
        // Al-Mustaqbal's number lecture explicitly uses إحدى عشرة ليرة.
        // https://dde.manuu.edu.in/sites/default/files/2021-12/Arabic%20Grammar%20and%20Communication_0.pdf
        // https://uomus.edu.iq/img/lectures21/MUCLecture_2024_51215125.pdf
        "INR" | "PKR" | "IDR" => ["روبية", "روبيتان", "روبيات", "روبية"],
        "TRY" => ["ليرة", "ليرتان", "ليرات", "ليرة"],
        // UNTERM country records document the singular and plural of these
        // consonant-final loan nouns. Their dual/accusative forms are the
        // regular masculine endings already used by the reviewed paradigms.
        // Japan ين/ينات: https://unterm.un.org/unterm2/view/unhq/6bf9481b-2a49-475f-a0cb-eaa6544800ea
        // China يوان/يوانات: https://unterm.un.org/unterm2/view/unhq/f9584092-3401-4a9a-a43f-6697c61fcf33
        // Malaysia رينغيت/رينغيتات: https://unterm.un.org/unterm2/view/unhq/819631a7-37d3-4b1e-b777-f1b6a2ccdd62
        // Peru سول/سولات: https://unterm.un.org/unterm2/view/unhq/fe3f7326-ddc8-40ba-8b9f-ac295ed610fc
        "JPY" => ["ين", "ينان", "ينات", "ينا"],
        "CNY" => ["يوان", "يوانان", "يوانات", "يوانا"],
        "MYR" => ["رينغيت", "رينغيتان", "رينغيتات", "رينغيتا"],
        "PEN" => ["سول", "سولان", "سولات", "سولا"],
        // UNTERM South Africa: masculine راند, plural راندات.
        "ZAR" => ["راند", "راندان", "راندات", "راندا"],
        // Euro one/dual forms are attested in UN Arabic documents; PONS gives
        // the sound loan plural يوروهات and uses it with خمسة. The singular is
        // case-invariable because the borrowed noun ends in a long vowel.
        // https://digitallibrary.un.org/record/3950930/files/A_CONF.219_2021_INF_1-AR.pdf
        // https://digitallibrary.un.org/record/3993942/files/E_C.12_69_D_48_2018-AR.pdf
        // https://de.pons.com/%C3%BCbersetzung/deutsch-arabisch/Euro
        "EUR" => ["يورو", "يوروان", "يوروهات", "يورو"],
        // KBS Arabic explicitly uses 5 وونات; كوريا الجنوبية is the fixed
        // proper-country name, so only the noun inflects.
        // https://rki.kbs.co.kr/service/contents_view.htm?board_code=&board_seq=63917&id=&lang=a&menu_cate=travel&page=8
        "KRW" => [
            "وون كوريا الجنوبية",
            "وونان كوريا الجنوبية",
            "وونات كوريا الجنوبية",
            "وونا كوريا الجنوبية",
        ],
        // UNTERM gives تاكا/تاكات. Arabic usage treats تاكا as feminine and
        // attests the dual oblique form التاكاتين, hence nominative تاكاتان.
        // https://unterm.un.org/unterm2/view/unhq/c73a28a0-bba7-4de3-9674-0f06d0aac7a8
        // https://ar.wikipedia.org/wiki/%D8%AA%D8%A7%D9%83%D8%A7_%D8%A8%D9%86%D8%BA%D9%84%D8%A7%D8%AF%D9%8A%D8%B4%D9%8A
        "BDT" => [
            "تاكا بنغلاديشية",
            "تاكاتان بنغلاديشيتان",
            "تاكات بنغلاديشية",
            "تاكا بنغلاديشية",
        ],
        // UNTERM attests the same consonant-final loan morphology for دونغ;
        // contemporary Arabic news usage spells this currency دونج and uses
        // دونجات/دونجا. The CLDR spelling is retained for lexical consistency.
        // https://unterm.un.org/unterm2/view/unhq/cc63aa1f-62b8-4edf-b7c8-03111fa66d89
        "VND" => ["دونج", "دونجان", "دونجات", "دونجا"],
        // UNTERM gives بيسو/بيسوات. Arabic reference usage also attests the
        // CLDR spelling بيزو and the plural بيزوات; the vowel-final singular
        // remains case-invariable while its adjective inflects.
        // https://unterm.un.org/unterm2/view/unhq/86af548b-4937-46a5-892c-e2143400e530
        // https://ar.wikipedia.org/wiki/%D8%A8%D9%8A%D8%B2%D9%88_%D9%81%D9%84%D8%A8%D9%8A%D9%86%D9%8A
        "MXN" | "ARS" | "CLP" | "COP" | "PHP" => ["بيزو", "بيزوان", "بيزوات", "بيزو"],
        // UNTERM uses the feminine noun نيرة and plural نيرات; unlike the
        // invariant CLDR label, this reviewed terminology has a complete
        // feminine paradigm with regular تاء مربوطة dual formation.
        // https://unterm.un.org/unterm2/view/unhq/f8b274b0-f014-4b25-99fd-6f2953f4aa1c
        "NGN" => [
            "نيرة نيجيرية",
            "نيرتان نيجيريتان",
            "نيرات نيجيرية",
            "نيرة نيجيرية",
        ],
        // UNTERM records بات as both singular and plural. Leskoff's Arabic
        // currency glossary independently uses بات تايلاندي rather than the
        // CLDR transliteration باخت.
        // https://unterm.un.org/unterm2/view/unhq/0159ec12-b34f-4a0d-852a-359ede7b2de9
        // https://www.leskoff.com/s02720-0
        "THB" => [
            "بات تايلاندي",
            "باتان تايلانديان",
            "بات تايلاندية",
            "باتا تايلانديا",
        ],
        // The Bank of Israel's Arabic pages pair singular شيكل جديد with the
        // broken plural شواكل جديدة (for example, خمسة شواكل جديدة). Broken
        // nonhuman plurals take feminine-singular adjectives.
        // https://www.boi.org.il/ar/roles-ar/%D8%A7%D9%84%D8%A3%D9%88%D8%B1%D8%A7%D9%82-%D8%A7%D9%84%D9%86%D9%82%D8%AF%D9%8A-%D8%A9/first-series-of-the-new-sheqel/
        "ILS" => [
            "شيكل إسرائيلي جديد",
            "شيكلان إسرائيليان جديدان",
            "شواكل إسرائيلية جديدة",
            "شيكلا إسرائيليا جديدا",
        ],
        // UNTERM Poland gives زلوتي/زلوتيات. The noun behaves like other
        // masculine nisba-final loans in the dual and accusative.
        // https://unterm.un.org/unterm2/view/unhq/f1b285eb-6097-463b-ac1a-495b33b115b5
        "PLN" => ["زلوتي", "زلوتيان", "زلوتيات", "زلوتيا"],
        _ => return None,
    })
}
fn arabic_noun_count(
    forms: [&str; 4],
    value: &NumberValue,
    options: &SpeechOptions,
    feminine: bool,
) -> Option<String> {
    let n = value.integer.parse::<u64>().ok()?;
    if matches!(n, 1 | 2) {
        return arabic_one_two_count(forms[0], forms[1], value, feminine);
    }
    let mut positive = value.clone();
    positive.negative = false;
    let words = if feminine {
        verbalize_with_gender(&positive, options, Some("feminine"))?
    } else {
        verbalize_with_options(&positive, options, NumberReading::Cardinal)?
    };
    let words = arabic_construct_count(words);
    let result = format!(
        "{words} {}",
        match n % 100 {
            3..=10 => forms[2],
            11..=99 => forms[3],
            _ => forms[0],
        }
    );
    Some(if value.negative {
        format!("سالب {result}")
    } else {
        result
    })
}

fn sign(locale: SpeechLocale) -> &'static str {
    match locale {
        SpeechLocale::Ar => "سالب",
        SpeechLocale::Es | SpeechLocale::Pt => "menos",
        SpeechLocale::De => "minus",
        SpeechLocale::Fr => "moins",
        SpeechLocale::Ru => "минус",
        _ => "minus",
    }
}
fn short_currency_name(locale: SpeechLocale, code: &str, one: bool) -> Option<&'static str> {
    use SpeechLocale::*;
    Some(match (locale, code, one) {
        (En, "USD", true) => "dollar",
        (En, "USD", false) => "dollars",
        (En, "GBP", true) => "pound",
        (En, "GBP", false) => "pounds",
        (En, "JPY", _) => "yen",
        (En, "KRW", _) => "won",
        (Es, "USD", true) => "dólar",
        (Es, "USD", false) => "dólares",
        (Es, "EUR", true) => "euro",
        (Es, "EUR", false) => "euros",
        (Es, "GBP", true) => "libra",
        (Es, "GBP", false) => "libras",
        (Pt, "BRL", true) => "real",
        (Pt, "BRL", false) => "reais",
        (De, "USD", _) => "dollar",
        (De, "EUR", _) => "euro",
        (Ru, "USD", true) => "доллар",
        (Ru, "USD", false) => "долларов",
        (Ru, "RUB", true) => "рубль",
        (Ru, "RUB", false) => "рублей",
        (Hi, "INR", _) => "रुपए",
        (Hi, "GBP", _) => "पाउंड",
        (Hi, "USD", _) => "डॉलर",
        (Hi, "KRW", _) => "वॉन",
        (Hi, "TRY", _) => "लीरा",
        (Ja, "JPY", _) => "円",
        (Ko, "KRW", _) => "원",
        (Zh, "USD", _) => "美元",
        (Zh, "CNY", _) => "人民币",
        (Zh, "EUR", _) => "欧元",
        (Zh, "GBP", _) => "英镑",
        (Zh, "KRW", _) => "韩元",
        (Vi, "VND", _) => "đồng",
        (Vi, "USD", _) => "đô la",
        (Vi, "EUR", _) => "ơ rô",
        (Vi, "JPY", _) => "yên nhật",
        (Ar, "USD", _) => "دولار",
        (Ar, "EUR", _) => "يورو",
        (Ar, "KWD", _) => "دينار كويتي",
        _ => return None,
    })
}
fn minor_name(locale: SpeechLocale, code: &str) -> Option<(&'static str, &'static str)> {
    use SpeechLocale::*;
    Some(match (locale, code) {
        (En, "USD" | "EUR" | "BRL" | "PEN") => ("cent", "cents"),
        (En, "GBP") => ("penny", "pence"),
        (Es, "USD" | "BRL") => ("centavo", "centavos"),
        (Es, "EUR" | "PEN") => ("céntimo", "céntimos"),
        (Es, "GBP") => ("penique", "peniques"),
        (Pt, "BRL" | "USD" | "EUR") => ("centavo", "centavos"),
        (Ru, "RUB") => ("копейка", "копеек"),
        (Ru, "USD") => ("цент", "центов"),
        (De, "USD" | "EUR") => ("cent", "cent"),
        (Ar, "USD" | "EUR") => ("سنت", "سنتات"),
        (Vi, "USD") => ("xu", "xu"),
        _ => return None,
    })
}

const UNIT_ALIASES: &[(&str, &str)] = &[
    ("mbps", "digital-megabit/per:duration-second"),
    ("kbps", "digital-kilobit/per:duration-second"),
    ("gbps", "digital-gigabit/per:duration-second"),
    ("cc", "volume-cubic-centimeter"),
    ("km/h", "speed-kilometer-per-hour"),
    ("км/ч", "speed-kilometer-per-hour"),
    ("m/s", "speed-meter-per-second"),
    ("mph", "speed-mile-per-hour"),
    ("km²", "area-square-kilometer"),
    ("km2", "area-square-kilometer"),
    ("m²", "area-square-meter"),
    ("m2", "area-square-meter"),
    ("cm³", "volume-cubic-centimeter"),
    ("cm3", "volume-cubic-centimeter"),
    ("m³", "volume-cubic-meter"),
    ("m3", "volume-cubic-meter"),
    ("°c", "temperature-celsius"),
    ("℃", "temperature-celsius"),
    ("°f", "temperature-fahrenheit"),
    ("℉", "temperature-fahrenheit"),
    ("kilograms", "mass-kilogram"),
    ("kilogram", "mass-kilogram"),
    ("milligrams", "mass-milligram"),
    ("milligram", "mass-milligram"),
    ("millimeters", "length-millimeter"),
    ("millimeter", "length-millimeter"),
    ("centimeters", "length-centimeter"),
    ("centimeter", "length-centimeter"),
    ("kilometers", "length-kilometer"),
    ("kilometer", "length-kilometer"),
    ("meters", "length-meter"),
    ("meter", "length-meter"),
    ("kg", "mass-kilogram"),
    ("кг", "mass-kilogram"),
    ("千克", "mass-kilogram"),
    ("公斤", "mass-kilogram"),
    ("キログラム", "mass-kilogram"),
    ("キロ", "mass-kilogram"),
    ("킬로그램", "mass-kilogram"),
    ("mg", "mass-milligram"),
    ("мг", "mass-milligram"),
    ("м", "length-meter"),
    ("км", "length-kilometer"),
    ("см", "length-centimeter"),
    ("мм", "length-millimeter"),
    ("г", "mass-gram"),
    ("л", "volume-liter"),
    ("km", "length-kilometer"),
    ("cm", "length-centimeter"),
    ("mm", "length-millimeter"),
    ("inch", "length-inch"),
    ("inches", "length-inch"),
    ("ft", "length-foot"),
    ("ha", "area-hectare"),
    ("ac", "area-acre"),
    ("g", "mass-gram"),
    ("m", "length-meter"),
    ("l", "volume-liter"),
    ("ml", "volume-milliliter"),
    ("ms", "duration-millisecond"),
    ("min", "duration-minute"),
    ("h", "duration-hour"),
    ("s", "duration-second"),
    ("kwh", "energy-kilowatt-hour"),
    ("khz", "frequency-kilohertz"),
    ("mhz", "frequency-megahertz"),
    ("ghz", "frequency-gigahertz"),
    ("hz", "frequency-hertz"),
    ("kb", "digital-kilobyte"),
    ("mb", "digital-megabyte"),
    ("gb", "digital-gigabyte"),
    ("tb", "digital-terabyte"),
    ("kw", "power-kilowatt"),
    ("mw", "power-megawatt"),
    ("w", "power-watt"),
    ("v", "electric-volt"),
    ("a", "electric-ampere"),
    ("k", "temperature-kelvin"),
];
const CURRENCY_ALIASES: &[(&str, &str)] = &[
    ("US$", "USD"),
    ("de euros", "EUR"),
    ("euros", "EUR"),
    ("euro", "EUR"),
    ("R$", "BRL"),
    ("S/.", "PEN"),
    ("S/", "PEN"),
    ("د.ك", "KWD"),
    ("دولار", "USD"),
    ("долл. США", "USD"),
    ("руб.", "RUB"),
    ("руб", "RUB"),
    ("美元", "USD"),
    ("人民币", "CNY"),
    ("欧元", "EUR"),
    ("英镑", "GBP"),
    ("韩元", "KRW"),
    ("円", "JPY"),
    ("원", "KRW"),
    ("元", "CNY"),
    ("đồng", "VND"),
    ("đô la", "USD"),
    ("USD", "USD"),
    ("EUR", "EUR"),
    ("GBP", "GBP"),
    ("PEN", "PEN"),
    ("JPY", "JPY"),
    ("CNY", "CNY"),
    ("INR", "INR"),
    ("BDT", "BDT"),
    ("PKR", "PKR"),
    ("IDR", "IDR"),
    ("BRL", "BRL"),
    ("RUB", "RUB"),
    ("KRW", "KRW"),
    ("TRY", "TRY"),
    ("VND", "VND"),
    ("KWD", "KWD"),
    ("SAR", "SAR"),
    ("AED", "AED"),
    ("EGP", "EGP"),
    ("CHF", "CHF"),
    ("CAD", "CAD"),
    ("AUD", "AUD"),
    ("NZD", "NZD"),
    ("HKD", "HKD"),
    ("SGD", "SGD"),
    ("MXN", "MXN"),
    ("ARS", "ARS"),
    ("CLP", "CLP"),
    ("COP", "COP"),
    ("NGN", "NGN"),
    ("ZAR", "ZAR"),
    ("TWD", "TWD"),
    ("THB", "THB"),
    ("MYR", "MYR"),
    ("PHP", "PHP"),
    ("ILS", "ILS"),
    ("PLN", "PLN"),
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
    ("￥", "?"),
];
