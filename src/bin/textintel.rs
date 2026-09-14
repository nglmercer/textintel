use std::env;

use textintel::evaluation::EvaluationDataset;
use textintel::{EngineConfig, ResourceLoader, TextIntelligence};

fn usage() -> &'static str {
    "Usage:\n  textintel analyze <text> [--json]\n  textintel explain <text> [--json]\n  textintel decode <text> [--json]\n  textintel compare <message-a> <message-b> [--json]\n  textintel spam <text> [--json]\n  textintel batch <input.jsonl> [--json]\n  textintel resources [resource-root] [--json]\n  textintel diagnostics [--json]\n  textintel evaluate [evaluation.json] [--json]\n  textintel index <store.json> <id> <text>\n  textintel search <store.json> <text> <limit> [--json]"
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
        "analyze" | "explain" => {
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
        "batch" => {
            let path = args.get(1).ok_or("batch requires <input.jsonl>")?;
            let source = std::fs::read_to_string(path)?;
            let texts = source
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| line.to_string())
                .collect::<Vec<_>>();
            let results = engine.analyze_batch(&texts)?;
            if json {
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
        }
        "resources" => {
            let root = args.get(1).map(String::as_str).unwrap_or("resources");
            let resources = ResourceLoader::from_resource_root(root)?;
            let report = serde_json::json!({
                "languages": resources.languages(),
                "language_count": resources.language_count(),
                "word_count": resources.word_count(),
                "symbol_count": resources.symbol_count(),
                "symbol_tokens": resources.symbol_tokens(),
            });
            if json {
                print_json(&report)?;
            } else {
                println!("languages: {:?}", resources.languages());
                println!("words: {}", resources.word_count());
                println!("symbols: {}", resources.symbol_count());
            }
        }
        "diagnostics" => {
            let report = serde_json::json!({
                "api_version": textintel::API_VERSION,
                "fingerprint_schema_version": textintel::FINGERPRINT_SCHEMA_VERSION,
                "providers": engine.provider_capabilities(),
            });
            if json {
                print_json(&report)?;
            } else {
                println!("api: {}", textintel::API_VERSION);
                println!("providers: {:?}", engine.provider_capabilities());
            }
        }
        "evaluate" => {
            let path = args
                .get(1)
                .map(String::as_str)
                .unwrap_or("data/evaluation.json");
            let source = std::fs::read_to_string(path)?;
            let dataset = EvaluationDataset::from_json(&source)?;
            let report = textintel::evaluation::evaluate(&engine, &dataset)?;
            if json {
                print_json(&report)?;
            } else {
                println!("ROC-AUC: {:.3}", report.metrics.roc_auc);
                println!("F1: {:.3}", report.metrics.f1);
                println!("rebus top-1: {:.3}", report.rebus.top1_accuracy);
            }
        }
        "index" => {
            let store = args
                .get(1)
                .ok_or("index requires <store.json> <id> <text>")?;
            let id = args
                .get(2)
                .ok_or("index requires <store.json> <id> <text>")?;
            let text = args
                .get(3..)
                .ok_or("index requires <store.json> <id> <text>")?
                .join(" ");
            let indexed = TextIntelligence::new(EngineConfig::default()).with_json_store(store)?;
            indexed.add_document(id, &text)?;
            println!("indexed {}", id);
        }
        "search" => {
            let store = args
                .get(1)
                .ok_or("search requires <store.json> <text> <limit>")?;
            let text = args
                .get(2)
                .ok_or("search requires <store.json> <text> <limit>")?;
            let limit = args
                .get(3)
                .ok_or("search requires <store.json> <text> <limit>")?
                .parse::<usize>()?;
            let indexed = TextIntelligence::new(EngineConfig::default()).with_json_store(store)?;
            let results = indexed.find_similar(text, limit)?;
            if json {
                print_json(&results)?;
            } else {
                for result in results {
                    println!("{:.3}\t{}", result.score, result.id);
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
