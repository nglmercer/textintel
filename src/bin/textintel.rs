use std::env;

use textintel::{EngineConfig, TextIntelligence};

fn usage() -> &'static str {
    "Usage:\n  textintel analyze <text> [--json]\n  textintel decode <text> [--json]\n  textintel compare <message-a> <message-b> [--json]\n  textintel spam <text> [--json]"
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let json = args.iter().any(|arg| arg == "--json");
    args.retain(|arg| arg != "--json");
    let Some(command) = args.first().map(String::as_str) else {
        eprintln!("{}", usage());
        std::process::exit(2);
    };
    let engine = TextIntelligence::new(EngineConfig::default());
    match command {
        "analyze" => {
            let text = args.get(1).ok_or("analyze requires <text>")?;
            let result = engine.analyze(text)?;
            if json {
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
        }
        "decode" => {
            let text = args.get(1).ok_or("decode requires <text>")?;
            let result = engine.decode(text)?;
            if json {
                print_json(&result)?;
            } else {
                for candidate in result {
                    println!("{:.3}\t{}", candidate.score, candidate.text);
                }
            }
        }
        "compare" => {
            let left = args
                .get(1)
                .ok_or("compare requires <message-a> <message-b>")?;
            let right = args
                .get(2)
                .ok_or("compare requires <message-a> <message-b>")?;
            let result = engine.compare(left, right)?;
            if json {
                print_json(&result)?;
            } else {
                println!("score: {:.3}", result.score);
                for explanation in result.explanations {
                    println!("- {}", explanation);
                }
            }
        }
        "spam" => {
            let text = args.get(1).ok_or("spam requires <text>")?;
            let result = engine.detect_spam(text)?;
            if json {
                print_json(&result)?;
            } else {
                println!("probability: {:.3}", result.probability);
                println!("labels: {:?}", result.labels);
                for reason in result.reasons {
                    println!("- {}", reason);
                }
            }
        }
        _ => {
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    }
    Ok(())
}
