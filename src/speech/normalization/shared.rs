//! Adapter from the versioned API to the shared typed speech pipeline.
//! No lexer, decimal parser, or language-specific verbalizer lives here.
use super::{
    ChangeKind, Confidence, CoverageLevel, NormalizationWarning, NormalizeError, NormalizeOptions,
    TextChange, WarningCode,
};
use crate::speech::{
    AbbreviationPolicy, SpeechDocument, SpeechLocale, SpeechNormalization, SpeechOptions,
    SpeechReading, SpeechSpan, SpeechValue,
};
use std::collections::BTreeMap;

/// Languages covered by the shared parser that have no legacy rule adapter.
pub(super) fn uses_shared(language: &str) -> bool {
    crate::speech::locales::for_language(language).is_some_and(|entry| !entry.has_versioned_adapter)
}
pub(super) fn coverage(
    language: &str,
    policy: AbbreviationPolicy,
) -> BTreeMap<String, CoverageLevel> {
    super::FEATURE_KEYS
        .iter()
        .map(|key| {
            (
                (*key).into(),
                match *key {
                    "cardinal_numbers" | "decimals" | "percentages" | "currency" | "dates"
                    | "times" | "units" | "ordinals" | "ranges" | "fractions" => {
                        CoverageLevel::Partial
                    }
                    "abbreviations"
                        if language.parse::<SpeechLocale>().is_ok_and(|locale| {
                            crate::speech::abbreviations::enabled_coverage(locale, policy) > 0
                        }) =>
                    {
                        CoverageLevel::Partial
                    }
                    _ => CoverageLevel::None,
                },
            )
        })
        .collect()
}

/// One typed parse/render pass over the original lexical source. Keeping the
/// document beside its readings lets compatibility adapters project semantic
/// values without running a second classifier.
pub(super) struct TypedAnalysis {
    document: SpeechDocument,
    result: SpeechNormalization,
}

impl TypedAnalysis {
    fn rows(&self) -> impl Iterator<Item = (&SpeechSpan, &SpeechReading)> {
        document_rows(&self.document, &self.result)
    }
}

fn document_rows<'a>(
    document: &'a SpeechDocument,
    result: &'a SpeechNormalization,
) -> impl Iterator<Item = (&'a SpeechSpan, &'a SpeechReading)> {
    document.spans.iter().zip(&result.readings)
}

fn analyze(
    source: &crate::speech::lexer::LexicalSource<'_>,
    opts: &NormalizeOptions,
) -> Result<TypedAnalysis, NormalizeError> {
    let locale: SpeechLocale = opts
        .language
        .parse()
        .map_err(|_| NormalizeError::InvalidLanguage)?;
    let mut options = SpeechOptions::new(locale);
    options.max_input_chars = opts.limits.max_input_chars.max(1);
    options.max_spans = opts.limits.max_input_chars.saturating_add(1).max(1);
    options.reading_style = opts.reading_style;
    options.abbreviation_policy = opts.abbreviation_policy;
    if let Some(profile) = crate::speech::locales::for_locale(&opts.locale) {
        options.decimal_separator = Some(profile.decimal_separator);
        if opts.locale_explicit {
            options.date_order = profile.date_order;
        }
    }
    let document = crate::speech::parser::parse_lexical_source(source, &options)
        .map_err(|_| NormalizeError::InvalidReadingOptions)?;
    let result = crate::speech::verbalize::verbalize_document_with_options(&document, &options)
        .map_err(|_| NormalizeError::InvalidReadingOptions)?;
    Ok(TypedAnalysis { document, result })
}

fn change_kind(value: &SpeechValue) -> Option<ChangeKind> {
    Some(match value {
        SpeechValue::Text | SpeechValue::Identifier { .. } => return None,
        SpeechValue::Number { value, .. } => {
            if value.fraction.is_some() {
                ChangeKind::Decimal
            } else {
                ChangeKind::Number
            }
        }
        SpeechValue::Abbreviation { .. } => ChangeKind::Abbreviation,
        SpeechValue::Date { .. } => ChangeKind::Date,
        SpeechValue::Time { .. } => ChangeKind::Time,
        SpeechValue::Currency { .. } => ChangeKind::Currency,
        SpeechValue::Percent { .. } => ChangeKind::Percentage,
        SpeechValue::InflectedNumber { .. } => ChangeKind::Number,
        SpeechValue::MeasuredExpression { .. } => ChangeKind::Unit,
        SpeechValue::LabelledNumber { .. } => ChangeKind::Unit,
        SpeechValue::Measure { .. } => ChangeKind::Unit,
        SpeechValue::Expression { value } => match crate::speech::expressions::kind(value) {
            "ordinal" => ChangeKind::Ordinal,
            "fraction" => ChangeKind::Fraction,
            "range" => ChangeKind::Range,
            "duration" => ChangeKind::Time,
            _ => ChangeKind::Number,
        },
        SpeechValue::Temporal { value } => {
            if crate::speech::temporal::kind(value) == "time" {
                ChangeKind::Time
            } else {
                ChangeKind::Date
            }
        }
        SpeechValue::Ambiguous { .. } => ChangeKind::Number,
    })
}

fn warning_kind(kind: ChangeKind) -> WarningCode {
    match kind {
        ChangeKind::Date => WarningCode::AmbiguousDate,
        ChangeKind::Time => WarningCode::AmbiguousTime,
        ChangeKind::Currency => WarningCode::AmbiguousCurrency,
        _ => WarningCode::AmbiguousNumber,
    }
}

fn generic_row(
    span: &SpeechSpan,
    reading: &SpeechReading,
) -> Option<(Option<TextChange>, Option<NormalizationWarning>)> {
    let kind = change_kind(&span.value)?;
    if reading.supported {
        if reading.spoken != span.source {
            return Some((
                Some(TextChange {
                    kind,
                    start: span.start,
                    end: span.end,
                    replacement: reading.spoken.clone(),
                    confidence: if kind == ChangeKind::Abbreviation {
                        Confidence::Medium
                    } else {
                        Confidence::High
                    },
                }),
                None,
            ));
        }
        Some((None, None))
    } else {
        Some((
            None,
            Some(NormalizationWarning {
                code: warning_kind(kind),
                start: span.start,
                end: span.end,
            }),
        ))
    }
}

pub(super) fn classify(
    source: &crate::speech::lexer::LexicalSource<'_>,
    opts: &NormalizeOptions,
) -> Result<(Vec<TextChange>, Vec<NormalizationWarning>), NormalizeError> {
    let analysis = analyze(source, opts)?;
    let mut changes = Vec::new();
    let mut warnings = Vec::new();
    for (span, reading) in analysis.rows() {
        if let Some((change, warning)) = generic_row(span, reading) {
            if let Some(change) = change {
                changes.push(change);
            }
            if let Some(warning) = warning {
                warnings.push(warning);
            }
        }
    }
    Ok((changes, warnings))
}

pub(super) fn feature(kind: ChangeKind) -> super::rules::Feature {
    use super::rules::Feature;
    match kind {
        ChangeKind::Number => Feature::CardinalNumbers,
        ChangeKind::Decimal => Feature::Decimals,
        ChangeKind::Date => Feature::Dates,
        ChangeKind::Time => Feature::Times,
        ChangeKind::Currency => Feature::Currency,
        ChangeKind::Percentage => Feature::Percentages,
        ChangeKind::Unit => Feature::Units,
        ChangeKind::Ordinal => Feature::Ordinals,
        ChangeKind::Abbreviation => Feature::Abbreviations,
        ChangeKind::Range => Feature::Ranges,
        ChangeKind::Fraction => Feature::Fractions,
    }
}

fn typed_expression_options(opts: &NormalizeOptions) -> Option<SpeechOptions> {
    let locale: SpeechLocale = opts.language.parse().ok()?;
    let profile = crate::speech::locales::for_locale(&opts.locale)?;
    let mut options = SpeechOptions::new(locale);
    options.reading_style = opts.reading_style;
    options.decimal_separator = Some(profile.decimal_separator);
    Some(options)
}

fn exact_number(value: u64) -> crate::speech::NumberValue {
    crate::speech::NumberValue {
        negative: false,
        integer: value.to_string(),
        fraction: None,
    }
}

fn amount_number(value: &super::rules::Amount) -> crate::speech::NumberValue {
    crate::speech::NumberValue {
        negative: value.negative,
        integer: value.int.to_string(),
        fraction: value.frac.clone(),
    }
}

fn render_typed_expression(
    value: crate::speech::expressions::ExpressionValue,
    opts: &NormalizeOptions,
) -> Option<String> {
    crate::speech::expressions::render(&value, &typed_expression_options(opts)?)
}

fn legacy_ordinal_marker(marker: &str) -> Option<super::rules::OrdinalMarker<'_>> {
    use super::rules::OrdinalMarker;
    Some(match marker {
        "st" | "nd" | "rd" | "th" => OrdinalMarker::Suffix(marker),
        "º" | ".º" => OrdinalMarker::Masculine,
        "ª" | ".ª" => OrdinalMarker::Feminine,
        _ => return None,
    })
}

/// Locate the lexical field corresponding to an ordinal span and apply the
/// versioned next-word rule: no punctuation at either edge and no line break.
fn next_word_after_span<'a>(
    source: &crate::speech::lexer::LexicalSource<'a>,
    span: &SpeechSpan,
) -> Option<&'a str> {
    let fields = source.fields();
    let index = fields.iter().position(|field| {
        field.core_start == span.start && field.core_end == span.end && field.has_core()
    })?;
    let current = fields[index];
    let next = *fields.get(index + 1)?;
    let gap = &source.text()[current.end..next.start];
    (current.bare_end() && next.bare_start() && next.has_core() && !gap.contains('\n'))
        .then(|| &source.text()[next.core_start..next.core_end])
}

/// Project typed ordinal semantics into the versioned gender/context-aware
/// ordinal renderers. Other ordinal marker families remain with typed rendering.
fn legacy_ordinal(
    source: &crate::speech::lexer::LexicalSource<'_>,
    span: &SpeechSpan,
    rules: &dyn super::rules::LanguageRules,
    locale: &str,
) -> Option<TextChange> {
    use crate::speech::expressions::ExpressionValue;
    let SpeechValue::Expression {
        value: ExpressionValue::Ordinal { value, marker },
    } = &span.value
    else {
        return None;
    };
    let marker = legacy_ordinal_marker(marker)?;
    let replacement = rules.ordinal(*value, marker, next_word_after_span(source, span), locale)?;
    Some(TextChange {
        kind: ChangeKind::Ordinal,
        start: span.start,
        end: span.end,
        replacement,
        confidence: Confidence::High,
    })
}

/// Project a typed timestamp into the established versioned renderer when the
/// legacy adapter supports its precision. Longer exact fractions remain with
/// the shared typed rendering, matching the existing augmentation contract.
fn legacy_timestamp(
    span: &SpeechSpan,
    rules: &dyn super::rules::LanguageRules,
    locale: &str,
) -> Option<TextChange> {
    use crate::speech::temporal::{TemporalValue, TimestampOffset};
    use chrono::TimeZone;
    let SpeechValue::Temporal {
        value:
            TemporalValue::Timestamp {
                year,
                month,
                day,
                hour,
                minute,
                second,
                fraction,
                offset,
            },
    } = &span.value
    else {
        return None;
    };
    if fraction
        .as_deref()
        .is_some_and(|fraction| fraction.len() > 9)
    {
        return None;
    }
    let year = year.parse::<u32>().ok()?;
    if !super::rules::valid_date(year, *month, *day) {
        return None;
    }
    let seconds = match offset {
        TimestampOffset::Utc | TimestampOffset::UnknownLocal => 0,
        TimestampOffset::Known {
            negative,
            hours,
            minutes,
        } => {
            let total = (hours * 3600 + minutes * 60) as i32;
            if *negative { -total } else { total }
        }
    };
    let offset = chrono::FixedOffset::east_opt(seconds)?;
    let value = offset
        .with_ymd_and_hms(year as i32, *month, *day, *hour, *minute, *second)
        .single()?;
    let replacement = rules.datetime(&value, fraction.as_deref().unwrap_or(""), locale)?;
    Some(TextChange {
        kind: ChangeKind::Date,
        start: span.start,
        end: span.end,
        replacement,
        confidence: Confidence::High,
    })
}

fn legacy_scientific(
    span: &SpeechSpan,
    rules: &dyn super::rules::LanguageRules,
    opts: &NormalizeOptions,
) -> Option<TextChange> {
    // Accept either a precise-style protected identifier or the Natural typed
    // expression: the Versioned shape profile itself decides compatibility.
    let (mantissa, exponent) = super::classify::parse_scientific(&span.source)?;
    let replacement = rules
        .scientific(&mantissa, exponent, &opts.locale)
        .or_else(|| {
            render_typed_expression(
                crate::speech::expressions::ExpressionValue::Scientific {
                    mantissa: amount_number(&mantissa),
                    exponent,
                },
                opts,
            )
        })?;
    Some(TextChange {
        kind: ChangeKind::Decimal,
        start: span.start,
        end: span.end,
        replacement,
        confidence: Confidence::High,
    })
}

fn legacy_duration(
    span: &SpeechSpan,
    rules: &dyn super::rules::LanguageRules,
    opts: &NormalizeOptions,
) -> Option<TextChange> {
    if opts.reading_style != super::ReadingStyle::Natural {
        return None;
    }
    let (hours, minutes, seconds) = super::classify::parse_duration(&span.source)?;
    if ![hours, minutes, seconds]
        .iter()
        .all(|value| *value <= super::classify::MAX_INTEGER)
    {
        return None;
    }
    let replacement = rules
        .duration(hours, minutes, seconds, &opts.locale)
        .or_else(|| {
            use crate::speech::expressions::{DurationComponent, DurationUnit, ExpressionValue};
            let components = [
                (hours, DurationUnit::Hours),
                (minutes, DurationUnit::Minutes),
                (seconds, DurationUnit::Seconds),
            ]
            .into_iter()
            .filter(|(value, _)| *value > 0)
            .map(|(value, unit)| DurationComponent { unit, value })
            .collect();
            render_typed_expression(ExpressionValue::Duration { components }, opts)
        })?;
    Some(TextChange {
        kind: ChangeKind::Unit,
        start: span.start,
        end: span.end,
        replacement,
        confidence: Confidence::High,
    })
}

fn versioned_relation(source: &str) -> Option<crate::speech::shapes::RelationShape> {
    use crate::speech::shapes::{self, ShapePolicy};
    let options = crate::speech::SpeechOptions::new(crate::speech::SpeechLocale::En);
    shapes::relation(source, 0, &options, ShapePolicy::Versioned)
        .filter(|shape| shape.end == source.len())
}

fn fraction_like(source: &str) -> bool {
    let parts: Vec<&str> = source.split('/').collect();
    parts.len() == 2
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Fractions retain the versioned ambiguity contract: only Natural rendering
/// with a reviewed fractional noun becomes speech; otherwise the whole slash
/// form carries one AmbiguousFraction warning.
fn legacy_fraction(
    span: &SpeechSpan,
    rules: &dyn super::rules::LanguageRules,
    opts: &NormalizeOptions,
) -> Option<(Option<TextChange>, Option<NormalizationWarning>)> {
    if !fraction_like(&span.source) {
        return None;
    }
    use crate::speech::shapes::RelationValue;
    let values = versioned_relation(&span.source).and_then(|shape| match shape.value {
        RelationValue::Fraction {
            numerator,
            denominator,
        } => Some((
            numerator.integer.parse::<u64>().ok()?,
            denominator.integer.parse::<u64>().ok()?,
        )),
        _ => None,
    });
    if opts.reading_style == super::ReadingStyle::Natural
        && let Some((numerator, denominator)) = values
        && numerator <= super::classify::MAX_INTEGER
        && denominator <= super::classify::MAX_INTEGER
        && let Some(replacement) = rules.fraction(numerator, denominator, &opts.locale)
    {
        return Some((
            Some(TextChange {
                kind: ChangeKind::Fraction,
                start: span.start,
                end: span.end,
                replacement,
                confidence: Confidence::High,
            }),
            None,
        ));
    }
    Some((
        None,
        Some(NormalizationWarning {
            code: WarningCode::AmbiguousFraction,
            start: span.start,
            end: span.end,
        }),
    ))
}

fn versioned_range(source: &str) -> Option<(u64, u64)> {
    use crate::speech::shapes::RelationValue;
    let shape = versioned_relation(source)?;
    let RelationValue::Range { from, to } = shape.value else {
        return None;
    };
    Some((
        from.integer.parse::<u64>().ok()?,
        to.integer.parse::<u64>().ok()?,
    ))
}

/// Reproduce the legacy field join for `10–15 kg`: exactly one space-like
/// separator, no punctuation, and an unprotected unit field.
fn joined_range_unit(
    source: &crate::speech::lexer::LexicalSource<'_>,
    span: &SpeechSpan,
) -> Option<(usize, super::rules::Unit)> {
    let fields = source.fields();
    let index = fields.iter().position(|field| {
        field.core_start == span.start && field.core_end == span.end && field.has_core()
    })?;
    let current = fields[index];
    let next = *fields.get(index + 1)?;
    let gap = &source.text()[current.end..next.start];
    if !(current.bare_end()
        && next.bare_start()
        && !next.protected
        && next.has_core()
        && matches!(gap, " " | "\u{a0}" | "\u{202f}"))
    {
        return None;
    }
    let unit = super::rules::unit_from_str(&source.text()[next.core_start..next.core_end])?;
    Some((next.core_end, unit))
}

fn legacy_unit_from_key(key: &str) -> Option<super::rules::Unit> {
    use super::rules::Unit;
    Some(match key {
        "mass-kilogram" => Unit::Kilogram,
        "mass-gram" => Unit::Gram,
        "length-kilometer" => Unit::Kilometer,
        "length-meter" => Unit::Meter,
        "length-centimeter" => Unit::Centimeter,
        "length-millimeter" => Unit::Millimeter,
        "volume-liter" => Unit::Liter,
        "volume-milliliter" => Unit::Milliliter,
        "temperature-celsius" => Unit::Celsius,
        "temperature-fahrenheit" => Unit::Fahrenheit,
        "digital-gigabyte" => Unit::Gigabyte,
        "frequency-megahertz" => Unit::Megahertz,
        "duration-millisecond" => Unit::Millisecond,
        _ => return None,
    })
}

fn key_for_legacy_unit(unit: super::rules::Unit) -> &'static str {
    use super::rules::Unit;
    match unit {
        Unit::Kilogram => "mass-kilogram",
        Unit::Gram => "mass-gram",
        Unit::Kilometer => "length-kilometer",
        Unit::Meter => "length-meter",
        Unit::Centimeter => "length-centimeter",
        Unit::Millimeter => "length-millimeter",
        Unit::Liter => "volume-liter",
        Unit::Milliliter => "volume-milliliter",
        Unit::Celsius => "temperature-celsius",
        Unit::Fahrenheit => "temperature-fahrenheit",
        Unit::KilometersPerHour => "speed-kilometer-per-hour",
        Unit::MetersPerSecond => "speed-meter-per-second",
        Unit::Gigabyte => "digital-gigabyte",
        Unit::Megahertz => "frequency-megahertz",
        Unit::Millisecond => "duration-millisecond",
    }
}

fn render_typed_range(
    from: u64,
    to: u64,
    unit: Option<super::rules::Unit>,
    opts: &NormalizeOptions,
) -> Option<String> {
    use crate::speech::expressions::ExpressionValue;
    let value = ExpressionValue::Range {
        from: exact_number(from),
        to: exact_number(to),
    };
    if let Some(unit) = unit {
        let measured = SpeechValue::MeasuredExpression {
            unit: key_for_legacy_unit(unit).into(),
            value,
        };
        crate::speech::quantities::render(&measured, "", &typed_expression_options(opts)?)
    } else {
        render_typed_expression(value, opts)
    }
}

fn number_count(value: &crate::speech::NumberValue) -> Option<u64> {
    (value.fraction.is_none() && !value.negative).then(|| value.integer.parse::<u64>().ok())?
}

/// Natural typed parsing recognizes a measured range as one semantic span.
/// Project that same span back to the established versioned range+unit rule.
fn legacy_measured_range(
    span: &SpeechSpan,
    rules: &dyn super::rules::LanguageRules,
    opts: &NormalizeOptions,
) -> Option<TextChange> {
    use crate::speech::expressions::ExpressionValue;
    let SpeechValue::MeasuredExpression { unit, value } = &span.value else {
        return None;
    };
    let ExpressionValue::Range { from, to } = value else {
        return None;
    };
    let unit = legacy_unit_from_key(unit)?;
    let from = number_count(from)?;
    let to = number_count(to)?;
    let replacement = rules
        .range(from, to, Some(unit), &opts.locale)
        .or_else(|| render_typed_range(from, to, Some(unit), opts))?;
    Some(TextChange {
        kind: ChangeKind::Range,
        start: span.start,
        end: span.end,
        replacement,
        confidence: Confidence::High,
    })
}

fn legacy_range(
    source: &crate::speech::lexer::LexicalSource<'_>,
    span: &SpeechSpan,
    rules: &dyn super::rules::LanguageRules,
    opts: &NormalizeOptions,
) -> Option<TextChange> {
    let (from, to) = versioned_range(&span.source)?;
    let (end, unit) = joined_range_unit(source, span)
        .map(|(end, unit)| (end, Some(unit)))
        .unwrap_or((span.end, None));
    let replacement = rules
        .range(from, to, unit, &opts.locale)
        .or_else(|| render_typed_range(from, to, unit, opts))?;
    Some(TextChange {
        kind: ChangeKind::Range,
        start: span.start,
        end,
        replacement,
        confidence: Confidence::High,
    })
}

fn all_ascii_digits(source: &str) -> bool {
    !source.is_empty() && source.bytes().all(|byte| byte.is_ascii_digit())
}

/// Project a typed plain-number span into the versioned numeric policy. The
/// typed document proves that the source is one semantic numeric candidate;
/// the shared versioned parser then retains locale separator ambiguity,
/// leading-zero, quantity-context, gender/counter, digit-style and digit-budget
/// policies exactly.
fn legacy_plain_number(
    source: &crate::speech::lexer::LexicalSource<'_>,
    span: &SpeechSpan,
    rules: &dyn super::rules::LanguageRules,
    opts: &NormalizeOptions,
) -> Option<(Option<TextChange>, Option<NormalizationWarning>)> {
    if !matches!(
        span.value,
        SpeechValue::Number { .. } | SpeechValue::Ambiguous { .. }
    ) {
        return None;
    }
    let conv = rules.conventions(&opts.locale)?;
    let warning = |code| {
        Some((
            None,
            Some(NormalizationWarning {
                code,
                start: span.start,
                end: span.end,
            }),
        ))
    };
    let change = |kind, replacement| {
        Some((
            Some(TextChange {
                kind,
                start: span.start,
                end: span.end,
                replacement,
                confidence: Confidence::High,
            }),
            None,
        ))
    };
    let raw = span.source.as_str();
    let next_word = next_word_after_span(source, span);
    let quantity_context = next_word.is_some_and(|word| rules.quantity_context(word));
    let context = if quantity_context {
        super::classify::NumberContext::Quantity
    } else {
        super::classify::NumberContext::Plain
    };
    let parsed = super::classify::parse_number_in(raw, &conv, context);
    let (negative, body) = super::classify::strip_sign(raw);
    let pure_digits =
        !negative && all_ascii_digits(body) && !crate::speech::lexer::is_protected_identifier(raw);
    if opts.reading_style == super::ReadingStyle::Digits
        && pure_digits
        && let Some(replacement) = rules.digits(body, &opts.locale)
    {
        return change(ChangeKind::Number, replacement);
    }
    use super::classify::NumParse;
    let amount = match parsed {
        NumParse::Value(amount) => amount,
        NumParse::Ambiguous => return warning(WarningCode::AmbiguousNumber),
        NumParse::TooLarge => return warning(WarningCode::NumberTooLarge),
        NumParse::NotNumber => return None,
    };
    if amount.frac.is_none()
        && next_word.is_some_and(|word| rules.number_gender_ambiguous(amount.int, word))
    {
        return warning(WarningCode::AmbiguousNumber);
    }
    if opts.reading_style == super::ReadingStyle::Digits && amount.frac.is_none() {
        let digits: String = body
            .bytes()
            .filter(u8::is_ascii_digit)
            .map(char::from)
            .collect();
        if let Some(replacement) = rules.digits(&digits, &opts.locale) {
            return change(ChangeKind::Number, replacement);
        }
    }
    let kind = if amount.frac.is_some() {
        ChangeKind::Decimal
    } else {
        ChangeKind::Number
    };
    let replacement = rules.number(&amount, &opts.locale)?;
    change(kind, replacement)
}

fn legacy_clock_warning(source: &str) -> Option<WarningCode> {
    // Typed spans own the whole separated meridiem (`13:30 PM`), while the
    // versioned clock grammar examined only the numeric clock body here.
    let mut clock = source.trim_end();
    let lower = clock.to_ascii_lowercase();
    for form in ["a.m.", "p.m.", "a.m", "p.m", "am", "pm"] {
        if lower.ends_with(form) {
            clock = clock[..clock.len() - form.len()].trim_end();
            break;
        }
    }
    let parts: Vec<&str> = clock.split(':').collect();
    if !parts
        .iter()
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    match parts.as_slice() {
        [hour, minute] if minute.len() == 2 && hour.len() <= 2 => Some(WarningCode::AmbiguousTime),
        [_, _] => Some(WarningCode::AmbiguousFraction),
        [hour, minute, second] if hour.len() <= 2 && minute.len() == 2 && second.len() == 2 => {
            Some(WarningCode::AmbiguousTime)
        }
        _ => None,
    }
}

fn source_meridiem(source: &str) -> Option<super::rules::Meridiem> {
    let lower = source.trim_end().to_ascii_lowercase();
    if lower.ends_with("p.m.") || lower.ends_with("p.m") || lower.ends_with("pm") {
        Some(super::rules::Meridiem::Pm)
    } else if lower.ends_with("a.m.") || lower.ends_with("a.m") || lower.ends_with("am") {
        Some(super::rules::Meridiem::Am)
    } else {
        None
    }
}

fn written_clock_hour(hour: u32, meridiem: Option<super::rules::Meridiem>) -> u32 {
    match meridiem {
        Some(super::rules::Meridiem::Pm) if hour >= 13 => hour - 12,
        Some(super::rules::Meridiem::Am) if hour == 0 => 12,
        _ => hour,
    }
}

/// The dotted meridiem's final period is ambiguous with sentence punctuation.
/// Typed parsing owns the complete written meridiem; compatibility keeps the
/// versioned boundary rule and leaves that period outside the replacement when
/// it is sentence-final. Lowercase continuations treat it as abbreviation
/// punctuation, exactly as the legacy joining logic did.
fn dotted_meridiem_period_is_sentence(
    source: &crate::speech::lexer::LexicalSource<'_>,
    span: &SpeechSpan,
) -> bool {
    let lower = span.source.trim_end().to_ascii_lowercase();
    if !lower.ends_with("a.m.") && !lower.ends_with("p.m.") {
        return false;
    }
    let text = source.text();
    let Some(rest) = text.get(span.end..) else {
        return true;
    };
    let mut whitespace = String::new();
    for ch in rest.chars() {
        if ch.is_whitespace() {
            whitespace.push(ch);
            continue;
        }
        if matches!(ch, ',' | ';' | ':' | ')') {
            return false;
        }
        return whitespace.contains('\n') || !(ch.is_lowercase() || ch.is_ascii_digit());
    }
    true
}

fn legacy_clock(
    source: &crate::speech::lexer::LexicalSource<'_>,
    span: &SpeechSpan,
    rules: &dyn super::rules::LanguageRules,
    opts: &NormalizeOptions,
) -> Option<(Option<TextChange>, Option<NormalizationWarning>)> {
    use crate::speech::temporal::TemporalValue;
    let warning = |code| {
        Some((
            None,
            Some(NormalizationWarning {
                code,
                start: span.start,
                end: span.end,
            }),
        ))
    };
    let (hour, minute, meridiem) = match &span.value {
        // Precise/phonetic parsing uses the compact compatibility IR, where
        // AM/PM is already folded into a 24-hour value. Recover only the
        // written 12-hour hour for the versioned renderer; no new clock
        // semantics are invented here.
        SpeechValue::Time {
            hour,
            minute,
            second: None,
        } => {
            let meridiem = source_meridiem(&span.source);
            (written_clock_hour(*hour, meridiem), *minute, meridiem)
        }
        SpeechValue::Temporal {
            value:
                TemporalValue::Clock {
                    hour,
                    minute: Some(minute),
                    second: None,
                    period: None,
                    meridiem,
                    half: false,
                },
        } => {
            let meridiem = match meridiem.as_deref() {
                None => None,
                Some(value) if value.eq_ignore_ascii_case("am") => Some(super::rules::Meridiem::Am),
                Some(value) if value.eq_ignore_ascii_case("pm") => Some(super::rules::Meridiem::Pm),
                Some(_) => return None,
            };
            (*hour, *minute, meridiem)
        }
        _ => {
            if matches!(span.value, SpeechValue::Ambiguous { .. })
                && span.source.contains(':')
                && let Some(code) = legacy_clock_warning(&span.source)
            {
                return warning(code);
            }
            return None;
        }
    };
    if let Some(replacement) =
        rules.time_styled(hour, minute, meridiem, opts.reading_style, &opts.locale)
    {
        let end = if dotted_meridiem_period_is_sentence(source, span) {
            span.end - 1
        } else {
            span.end
        };
        return Some((
            Some(TextChange {
                kind: ChangeKind::Time,
                start: span.start,
                end,
                replacement,
                confidence: Confidence::Medium,
            }),
            None,
        ));
    }
    // A valid typed clock whose versioned renderer intentionally fails closed
    // (for example Russian case-dependent times) remains eligible for typed
    // rendering. Consuming the row here would regress shared semantic coverage.
    None
}

fn legacy_date(
    span: &SpeechSpan,
    rules: &dyn super::rules::LanguageRules,
    opts: &NormalizeOptions,
) -> Option<(Option<TextChange>, Option<NormalizationWarning>)> {
    use crate::speech::temporal::{DatePresentation, TemporalValue};
    let fields = match &span.value {
        SpeechValue::Date { year, month, day } => Some((*year, *month, *day)),
        SpeechValue::Temporal {
            value:
                TemporalValue::Date {
                    year: Some(year),
                    month: Some(month),
                    day: Some(day),
                    presentation: DatePresentation::Numeric,
                    connector: None,
                    year_label: false,
                },
        } => year.parse::<u32>().ok().map(|year| (year, *month, *day)),
        _ => None,
    };
    if let Some((year, month, day)) = fields {
        let replacement = rules.date(year, month, day, &opts.locale)?;
        return Some((
            Some(TextChange {
                kind: ChangeKind::Date,
                start: span.start,
                end: span.end,
                replacement,
                confidence: Confidence::High,
            }),
            None,
        ));
    }
    if !matches!(span.value, SpeechValue::Ambiguous { .. })
        || span.source.chars().any(|ch| matches!(ch, 'T' | 't' | ':'))
        || !(span.source.contains('/') || span.source.matches('-').count() == 2)
    {
        return None;
    }
    let parts: Vec<&str> = span.source.split('/').collect();
    if matches!(parts.as_slice(), [a, b, year] if a.len() <= 2 && b.len() <= 2 && year.len() == 2) {
        return Some((
            None,
            Some(NormalizationWarning {
                code: WarningCode::AmbiguousDate,
                start: span.start,
                end: span.end,
            }),
        ));
    }
    let conv = rules.conventions(&opts.locale)?;
    match super::classify::shared_date_fields(&span.source, &conv, opts.locale_explicit) {
        Ok((year, month, day)) => {
            let replacement = rules.date(year, month, day, &opts.locale)?;
            Some((
                Some(TextChange {
                    kind: ChangeKind::Date,
                    start: span.start,
                    end: span.end,
                    replacement,
                    confidence: Confidence::High,
                }),
                None,
            ))
        }
        Err(code) => Some((
            None,
            Some(NormalizationWarning {
                code,
                start: span.start,
                end: span.end,
            }),
        )),
    }
}

fn timestamp_shaped(source: &str) -> bool {
    source.len() >= 11
        && source.as_bytes().get(4) == Some(&b'-')
        && source
            .as_bytes()
            .get(10)
            .is_some_and(|b| *b == b'T' || *b == b't')
}

/// Compatibility renderers for semantic values that used to have a second
/// classifier. They only replace wording; shared parsing remains authoritative.
fn legacy_row(
    source: &crate::speech::lexer::LexicalSource<'_>,
    span: &SpeechSpan,
    reading: &SpeechReading,
    rules: &dyn super::rules::LanguageRules,
    opts: &NormalizeOptions,
) -> Option<(Option<TextChange>, Option<NormalizationWarning>)> {
    let _ = reading;
    if let Some(row) = legacy_plain_number(source, span, rules, opts) {
        return Some(row);
    }
    if let Some(change) = legacy_ordinal(source, span, rules, &opts.locale) {
        return Some((Some(change), None));
    }
    if let Some(row) = legacy_fraction(span, rules, opts) {
        return Some(row);
    }
    if let Some(change) = legacy_measured_range(span, rules, opts) {
        return Some((Some(change), None));
    }
    if let Some(change) = legacy_range(source, span, rules, opts) {
        return Some((Some(change), None));
    }
    if let Some(change) = legacy_scientific(span, rules, opts) {
        return Some((Some(change), None));
    }
    if let Some(change) = legacy_duration(span, rules, opts) {
        return Some((Some(change), None));
    }
    if let Some(row) = legacy_clock(source, span, rules, opts) {
        return Some(row);
    }
    if let Some(row) = legacy_date(span, rules, opts) {
        return Some(row);
    }
    if timestamp_shaped(&span.source) && matches!(span.value, SpeechValue::Ambiguous { .. }) {
        return Some((
            None,
            Some(NormalizationWarning {
                code: WarningCode::AmbiguousTime,
                start: span.start,
                end: span.end,
            }),
        ));
    }
    if let Some(change) = legacy_timestamp(span, rules, &opts.locale) {
        return Some((Some(change), None));
    }
    None
}

/// Retain established presentation rules while filling missing feature adapters
/// from the same typed core. Existing replacements and preservation diagnostics win.
pub(super) fn augment(
    source: &crate::speech::lexer::LexicalSource<'_>,
    opts: &NormalizeOptions,
    legacy: &dyn super::rules::LanguageRules,
    changes: &mut Vec<TextChange>,
    warnings: &mut Vec<NormalizationWarning>,
) -> Result<(), NormalizeError> {
    if opts.language.parse::<SpeechLocale>().is_err() {
        return Ok(());
    }
    let text = source.text();
    let analysis = analyze(source, opts)?;
    for (span, reading) in analysis.rows() {
        let compatibility = legacy_row(source, span, reading, legacy, opts);
        if let Some((_, Some(warning))) = &compatibility
            && !warnings
                .iter()
                .any(|old| old.start < warning.end && warning.start < old.end)
        {
            warnings.push(warning.clone());
        }
        // A legacy warning alone never blocked typed augmentation in the
        // historical pipeline: it only stayed attached to the preserved source.
        // Compatibility supersedes the typed row when it produces an actual
        // replacement; warning-only rows still allow missing-feature adapters.
        let is_compatibility = compatibility
            .as_ref()
            .and_then(|(change, _)| change.as_ref())
            .is_some();
        let row = if is_compatibility {
            compatibility
        } else {
            generic_row(span, reading)
        };
        let Some((change, warning)) = row else {
            continue;
        };
        // Generic typed warnings remain diagnostic input to the compatibility
        // adapter, but the versioned API's legacy classifier owns public warning
        // spans except when a compatibility rule explicitly projects one above.
        let _ = warning;
        let Some(change) = change else {
            continue;
        };
        if !is_compatibility
            && legacy.coverage(feature(change.kind)) != CoverageLevel::None
            && !matches!(change.kind, ChangeKind::Date | ChangeKind::Time)
        {
            continue;
        }
        // A terminal dot can denote a German ordinal; this adapter must not
        // resolve that ambiguity by stripping the marker and reading a count.
        if opts.language == "de"
            && change.kind == ChangeKind::Number
            && text[change.end..].starts_with('.')
        {
            continue;
        }
        if !is_compatibility
            && change.kind == ChangeKind::Number
            && opts.reading_style != super::ReadingStyle::Digits
        {
            let raw = &text[change.start..change.end];
            if raw.len() > 1 && raw.starts_with('0') && raw.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
        }
        if changes
            .iter()
            .any(|old| old.start < change.end && change.start < old.end)
        {
            continue;
        }
        if warnings.iter().any(|old| {
            old.start < change.end
                && change.start < old.end
                && !(old.start == change.start && old.end == change.end)
        }) {
            continue;
        }
        // A previously unsupported feature can now resolve its entire span.
        warnings.retain(|old| !(old.start == change.start && old.end == change.end));
        changes.push(change);
    }
    changes.sort_by_key(|v| v.start);
    warnings.sort_by_key(|v| v.start);
    Ok(())
}
