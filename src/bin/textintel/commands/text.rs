//! Single-text commands: `analyze`, `explain`, `decode`, `spam`, `batch`.

use textintel::cli::ParsedArgs;

use super::common::{build_engine, print_json};

pub fn analyze(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let text = parsed.positional(0).ok_or("analyze requires <text>")?;
    let engine = build_engine(parsed)?;
    let result = engine.analyze(text)?;
    if parsed.flag("json") {
        print_json(&result)?;
    } else {
        println!("raw: {}", result.raw);
        println!("normalized: {}", result.normalized.as_deref().unwrap_or(""));
        println!("languages: {:?}", result.language_candidates);
        println!(
            "decoded: {:?}",
            result
                .rebus_candidates
                .iter()
                .map(|candidate| (&candidate.text, candidate.score))
                .collect::<Vec<_>>()
        );
        println!(
            "obfuscation: {:.3} {:?}",
            result.obfuscation_features.score, result.obfuscation_features.flags
        );
    }
    Ok(0)
}

pub fn explain(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let text = parsed.positional(0).ok_or("explain requires <text>")?;
    let engine = build_engine(parsed)?;
    let result = engine.analyze(text)?;
    if parsed.flag("json") {
        print_json(&result)?;
    } else {
        println!("raw: {}", result.raw);
        for candidate in result.rebus_candidates.iter().take(3) {
            println!("decode {:.3}\t{}", candidate.score, candidate.text);
            for step in &candidate.transformations {
                println!("  → {}", step.explain());
            }
        }
        for (name, view) in &result.normalization_views {
            if name.starts_with("transliteration:") {
                println!("view {name}: {view}");
            }
        }
    }
    Ok(0)
}

pub fn decode(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let text = parsed.positional(0).ok_or("decode requires <text>")?;
    let engine = build_engine(parsed)?;
    let languages = parsed.value("languages").map(|value| {
        value
            .split(',')
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
    });
    let languages = languages.filter(|values| !values.is_empty());
    let result = engine.decode_with_languages(text, languages.as_deref(), None)?;
    if parsed.flag("json") {
        print_json(&result)?;
    } else {
        for candidate in result {
            println!("{:.3}\t{}", candidate.score, candidate.text);
        }
    }
    Ok(0)
}

pub fn spam(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let text = parsed.positional(0).ok_or("spam requires <text>")?;
    let engine = build_engine(parsed)?;
    let result = engine.detect_spam(text)?;
    if parsed.flag("json") {
        print_json(&result)?;
    } else {
        println!("probability: {:.3}", result.probability);
        println!("labels: {:?}", result.labels);
        for reason in result.reasons {
            println!("- {}", reason);
        }
    }
    Ok(0)
}

pub fn batch(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let path = parsed.positional(0).ok_or("batch requires <input.jsonl>")?;
    let engine = build_engine(parsed)?;
    let source = std::fs::read_to_string(path)?;
    let texts = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    let results = engine.analyze_batch(&texts)?;
    if parsed.flag("json") {
        print_json(&results)?;
    } else {
        for result in results {
            println!(
                "{}\t{}",
                result.top_language().unwrap_or("unknown"),
                result.raw
            );
        }
    }
    Ok(0)
}
