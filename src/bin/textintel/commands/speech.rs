//! Explicit speech parsing, normalization, and corpus evaluation.
use super::common::print_json;
use textintel::cli::ParsedArgs;
use textintel::speech::{DateOrder, SpeechLocale, SpeechOptions};

fn options(parsed: &ParsedArgs) -> Result<SpeechOptions, Box<dyn std::error::Error>> {
    let locale: SpeechLocale = parsed.required_value("locale")?.parse()?;
    let mut options = SpeechOptions::new(locale);
    options.date_order = match parsed.value("date-order").unwrap_or("reject") {
        "dmy" => DateOrder::DayMonthYear,
        "mdy" => DateOrder::MonthDayYear,
        "reject" => DateOrder::RejectAmbiguous,
        other => return Err(format!("unknown date order {other:?}; use dmy|mdy|reject").into()),
    };
    options.decimal_separator = match parsed.value("decimal-separator") {
        None => None,
        Some("." | "dot") => Some('.'),
        Some("," | "comma") => Some(','),
        Some("٫" | "arabic") => Some('٫'),
        Some(other) => return Err(format!("unsupported decimal separator {other:?}").into()),
    };
    Ok(options)
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
    let result = textintel::speech::evaluation::evaluate_speech(&corpus)?;
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
        for (locale, metrics) in &result.by_locale {
            println!(
                "{locale}: parse {}/{} spoken {}/{} fully_supported {}",
                metrics.parse_exact,
                metrics.parse_cases,
                metrics.spoken_exact,
                metrics.spoken_cases,
                metrics.fully_supported
            );
        }
        for failure in &result.failures {
            eprintln!("failed: {}", failure.id);
        }
    }
    Ok(i32::from(!result.failures.is_empty()))
}
