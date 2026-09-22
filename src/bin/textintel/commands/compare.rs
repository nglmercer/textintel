//! Pair commands: `compare`, `duplicate`.

use textintel::cli::ParsedArgs;

use super::common::{build_engine, print_json};

pub fn compare(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let left = parsed
        .positional(0)
        .ok_or("compare requires <message-a> <message-b>")?;
    let right = parsed
        .positional(1)
        .ok_or("compare requires <message-a> <message-b>")?;
    let engine = build_engine(parsed)?;
    let result = engine.compare(left, right)?;
    if parsed.flag("json") {
        print_json(&result)?;
    } else {
        println!("score: {:.3}", result.score);
        for explanation in result.explanations {
            println!("- {}", explanation);
        }
    }
    Ok(0)
}

pub fn duplicate(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let left = parsed
        .positional(0)
        .ok_or("duplicate requires <message-a> <message-b>")?;
    let right = parsed
        .positional(1)
        .ok_or("duplicate requires <message-a> <message-b>")?;
    let engine = build_engine(parsed)?;
    let threshold = parsed
        .value("threshold")
        .map(|value| value.parse::<f64>())
        .transpose()?
        .unwrap_or(0.85);
    let mode = match parsed.value("mode") {
        None | Some("combined") => textintel::DuplicateMode::Combined,
        Some("near_exact") | Some("near-exact") => textintel::DuplicateMode::NearExact,
        Some("lexical") => textintel::DuplicateMode::Lexical,
        Some("semantic") => textintel::DuplicateMode::Semantic,
        Some("phonetic") => textintel::DuplicateMode::Phonetic,
        Some("decoded") => textintel::DuplicateMode::Decoded,
        Some("visual") => textintel::DuplicateMode::Visual,
        Some(other) => return Err(format!("unknown duplicate mode '{other}'").into()),
    };
    let result = engine.duplicate_with_mode(left, right, threshold, mode)?;
    if parsed.flag("json") {
        print_json(&result)?;
    } else {
        println!(
            "duplicate: {} score: {:.3} reason: {}",
            result.duplicate, result.score, result.reason
        );
    }
    Ok(0)
}
