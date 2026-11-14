//! Explicit speech parsing, normalization, and corpus evaluation.
use super::common::print_json;
use textintel::cli::ParsedArgs;
use textintel::speech::{
    ClockPolicy, CurrencyNaming, DateOrder, InvalidDatePolicy, NumberNotation, ReadingStyle,
    SpeechInterpretation, SpeechLocale, SpeechOptions,
};

fn options(parsed: &ParsedArgs) -> Result<SpeechOptions, Box<dyn std::error::Error>> {
    let locale: SpeechLocale = parsed.required_value("locale")?.parse()?;
    let mut options = SpeechOptions::new(locale);
    options.date_order = match parsed.value("date-order").unwrap_or("reject") {
        "dmy" => DateOrder::DayMonthYear,
        "mdy" => DateOrder::MonthDayYear,
        "ymd" => DateOrder::YearMonthDay,
        "reject" => DateOrder::RejectAmbiguous,
        other => {
            return Err(format!("unknown date order {other:?}; use dmy|mdy|ymd|reject").into());
        }
    };
    options.decimal_separator = match parsed.value("decimal-separator") {
        None => None,
        Some("." | "dot") => Some('.'),
        Some("," | "comma") => Some(','),
        Some("٫" | "arabic") => Some('٫'),
        Some(other) => return Err(format!("unsupported decimal separator {other:?}").into()),
    };
    if let Some(style) = parsed.value("reading-style") {
        options.reading_style = ReadingStyle::parse(style)
            .ok_or("unknown reading style; use precise|natural|digits")?;
    }
    if let Some(notation) = parsed.value("number-notation") {
        options.number_notation = notation_option(notation)?;
    }
    if let Some(naming) = parsed.value("currency-naming") {
        options.currency_naming = currency_naming_option(naming)?;
    }
    if let Some(clock_policy) = parsed.value("clock-policy") {
        options.clock_policy = clock_policy_option(clock_policy)?;
    }
    if let Some(invalid_date_policy) = parsed.value("invalid-date-policy") {
        options.invalid_date_policy = invalid_date_policy_option(invalid_date_policy)?;
    }
    if let Some(kind) = parsed.value("interpret-as") {
        options.interpret_as = match kind {
            "auto" => SpeechInterpretation::Auto,
            "number" => SpeechInterpretation::Number,
            "date" => SpeechInterpretation::Date,
            "time" => SpeechInterpretation::Time,
            "currency" => SpeechInterpretation::Currency,
            "measure" => SpeechInterpretation::Measure,
            "duration" => SpeechInterpretation::Duration,
            _ => return Err("unknown semantic speech hint".into()),
        };
    }
    if let Some(code) = parsed.value("currency") {
        options.default_currency = Some(code.to_ascii_uppercase());
    }
    Ok(options)
}
fn notation_option(value: &str) -> Result<NumberNotation, Box<dyn std::error::Error>> {
    match value {
        "phonetic" => Ok(NumberNotation::Phonetic),
        "orthographic" => Ok(NumberNotation::Orthographic),
        _ => Err("unknown number notation; use phonetic|orthographic".into()),
    }
}
fn currency_naming_option(value: &str) -> Result<CurrencyNaming, Box<dyn std::error::Error>> {
    CurrencyNaming::parse(value).ok_or_else(|| "unknown currency naming; use style|full".into())
}
fn clock_policy_option(value: &str) -> Result<ClockPolicy, Box<dyn std::error::Error>> {
    ClockPolicy::parse(value)
        .ok_or_else(|| "unknown clock policy; use strict|normalize-redundant-period".into())
}
fn invalid_date_policy_option(
    value: &str,
) -> Result<InvalidDatePolicy, Box<dyn std::error::Error>> {
    InvalidDatePolicy::parse(value)
        .ok_or_else(|| "unknown invalid date policy; use preserve|read-components".into())
}
pub fn parse(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let result =
        textintel::speech::parse_for_speech(parsed.required_positional(0)?, &options(parsed)?)?;
    if parsed.flag("json") {
        print_json(&result)?;
    } else {
        for span in &result.spans {
            println!(
                "{}..{} {}: {}",
                span.start,
                span.end,
                textintel::speech::evaluation::speech_kind(&span.value),
                span.source
            );
        }
    }
    Ok(0)
}
pub fn normalize(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let result =
        textintel::speech::normalize_for_speech(parsed.required_positional(0)?, &options(parsed)?)?;
    if parsed.flag("json") {
        print_json(&result)?;
    } else {
        println!("{}", result.text);
        for reading in result.readings.iter().filter(|s| !s.supported) {
            eprintln!(
                "unresolved {}..{}: {}",
                reading.start,
                reading.end,
                reading.reason.as_deref().unwrap_or("unsupported reading")
            );
        }
    }
    Ok(0)
}
pub fn evaluate(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let corpus = textintel::speech::evaluation::SpeechCorpus::from_file(
        parsed
            .positional(0)
            .unwrap_or("data/speech/evaluation.json"),
    )?;
    let mut policy = if let Some(path) = parsed.value("speech-policy") {
        serde_json::from_str(&std::fs::read_to_string(path)?)?
    } else {
        textintel::speech::evaluation::SpeechEvaluationPolicy::default()
    };
    if let Some(style) = parsed.value("reading-style") {
        policy.reading_style = Some(
            ReadingStyle::parse(style)
                .ok_or("unknown reading style; use precise|natural|digits")?,
        );
    }
    if let Some(notation) = parsed.value("number-notation") {
        policy.number_notation = Some(notation_option(notation)?);
    }
    if let Some(naming) = parsed.value("currency-naming") {
        policy.currency_naming = Some(currency_naming_option(naming)?);
    }
    if let Some(clock_policy) = parsed.value("clock-policy") {
        policy.clock_policy = Some(clock_policy_option(clock_policy)?);
    }
    if let Some(invalid_date_policy) = parsed.value("invalid-date-policy") {
        policy.invalid_date_policy = Some(invalid_date_policy_option(invalid_date_policy)?);
    }
    let result = textintel::speech::evaluation::evaluate_speech_with_policy(&corpus, &policy)?;
    if parsed.flag("json") {
        print_json(&result)?;
    } else {
        println!(
            "cases={} parse_exact={}/{} ({:.2}%) spoken_exact={}/{} fully_supported={}",
            result.totals.cases,
            result.totals.parse_exact,
            result.totals.parse_cases,
            result.totals.parse_accuracy() * 100.0,
            result.totals.spoken_exact,
            result.totals.spoken_cases,
            result.totals.fully_supported
        );
        println!(
            "spoken_variants={} accepted={}/{} reference_exact_ceiling={}",
            result.totals.spoken_variant,
            result.totals.spoken_accepted,
            result.totals.spoken_cases,
            result.exact_spoken_ceiling
        );
        if let Some(semantic) = &result.totals.semantic {
            println!(
                "semantic_exact={}/{} ({:.2}%) value_spans={}/{} predicted={} precision={:.2}% recall={:.2}%",
                semantic.exact,
                semantic.cases,
                semantic.accuracy() * 100.0,
                semantic.correct_spans,
                semantic.expected_spans,
                semantic.predicted_spans,
                semantic.precision() * 100.0,
                semantic.recall() * 100.0
            );
        }
        println!(
            "diagnostics={} supported_with_diagnostic={}",
            result.diagnostics.len(),
            result
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.reading_supported)
                .count()
        );
        for (locale, metrics) in &result.by_locale {
            println!(
                "{locale}: parse {}/{} spoken {}/{} fully_supported {}",
                metrics.parse_exact,
                metrics.parse_cases,
                metrics.spoken_exact,
                metrics.spoken_cases,
                metrics.fully_supported
            );
            if let Some(semantic) = &metrics.semantic {
                println!(
                    "{locale}: semantic {}/{} values {}/{} predicted {}",
                    semantic.exact,
                    semantic.cases,
                    semantic.correct_spans,
                    semantic.expected_spans,
                    semantic.predicted_spans
                );
            }
        }
        for failure in &result.failures {
            eprintln!("failed: {}", failure.id);
        }
    }
    Ok(i32::from(!result.passes(parsed.flag("accept-variants"))))
}
