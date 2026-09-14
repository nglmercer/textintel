use std::env;

use textintel::comparison::SimilarityProfile;
use textintel::evaluation::{EvaluateOptions, EvaluationDataset, EvaluationReport};
use textintel::{EngineConfig, ResourceLoader, TextIntelligence};

fn usage() -> &'static str {
    "Usage:\n  textintel analyze <text> [--json]\n  textintel explain <text> [--json]\n  textintel decode <text> [--languages <es,en>] [--json]\n  textintel compare <message-a> <message-b> [--json]\n  textintel spam <text> [--json]\n  textintel batch <input.jsonl> [--json]\n  textintel resources [resource-root] [--json]\n  textintel resources validate <path> [--json]\n  textintel diagnostics [--json]\n  textintel provider-info [--json]\n  textintel schema-version [--json]\n  textintel eval <evaluation.json> [--split train|validation|test] [--profile <name>] [--gates <quality-gates.json>] [--no-ranking] [--json]\n  textintel evaluate <evaluation.json> [--split train|validation|test] [--profile <name>] [--gates <quality-gates.json>] [--no-ranking] [--json]\n  textintel index <store.json> <id> <text>\n  textintel search <store.json> <text> <limit> [--json]"
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

fn profile_named(name: &str) -> Option<SimilarityProfile> {
    match name {
        "general" | "general_similarity" => Some(SimilarityProfile::general_similarity()),
        "duplicate" => Some(SimilarityProfile::duplicate()),
        "spam" | "spam_pattern" => Some(SimilarityProfile::spam()),
        "obfuscation" => Some(SimilarityProfile::obfuscation()),
        "rebus" => Some(SimilarityProfile::rebus()),
        "search" | "search_rerank" => Some(SimilarityProfile::general_similarity()),
        _ => None,
    }
}

fn print_eval_human(report: &EvaluationReport) {
    if !report.split.is_empty() {
        println!("split: {}", report.split);
    }
    println!("dataset: {}", report.dataset_version);
    println!("cases: {}", report.metrics.count);
    println!(
        "similarity: accuracy={:.3} precision={:.3} recall={:.3} f1={:.3}",
        report.metrics.accuracy, report.metrics.precision, report.metrics.recall, report.metrics.f1
    );
    println!(
        "similarity: roc_auc={:.3} pr_auc={:.3} brier={:.3} ece={:.3}",
        report.metrics.roc_auc,
        report.metrics.pr_auc,
        report.metrics.brier,
        report.metrics.expected_calibration_error
    );
    println!(
        "rebus: top1={:.3} top3={:.3} top5={:.3} mrr={:.3} (n={})",
        report.rebus.top1_accuracy,
        report.rebus.top3_accuracy,
        report.rebus.top5_accuracy,
        report.rebus.mrr,
        report.rebus.labelled_cases
    );
    println!(
        "language: top1={:.3} top3={:.3} unknown_p={:.3} unknown_r={:.3} (n={})",
        report.language.top1_accuracy,
        report.language.top3_accuracy,
        report.language.unknown_precision,
        report.language.unknown_recall,
        report.language.labelled_cases
    );
    println!(
        "search: recall@1={:.3} recall@5={:.3} recall@10={:.3} mrr={:.3} ndcg@10={:.3} (queries={})",
        report.ranking.recall_at_1,
        report.ranking.recall_at_5,
        report.ranking.recall_at_10,
        report.ranking.mrr,
        report.ranking.ndcg_at_10,
        report.ranking.queries
    );
    println!(
        "latency: compare_mean={:.0}us p50={:.0} p95={:.0} p99={:.0}",
        report.compare_latency.mean_micros,
        report.compare_latency.p50_micros,
        report.compare_latency.p95_micros,
        report.compare_latency.p99_micros
    );
    if report.expectations_total > 0 {
        println!(
            "expectations: {}/{} met",
            report.expectations_met, report.expectations_total
        );
    }
}

/// Quality-gate file shape: `{ "<section>": { "<metric>_min": f64 } }`.
/// Unknown sections and metrics are ignored so gates stay forwards compatible.
fn check_gates(report: &EvaluationReport, gates: &serde_json::Value) -> Vec<String> {
    let observed: Vec<(&str, &str, f64)> = vec![
        ("similarity", "roc_auc", report.metrics.roc_auc),
        ("similarity", "pr_auc", report.metrics.pr_auc),
        ("similarity", "f1", report.metrics.f1),
        ("similarity", "accuracy", report.metrics.accuracy),
        ("rebus", "top1", report.rebus.top1_accuracy),
        ("rebus", "top3", report.rebus.top3_accuracy),
        ("rebus", "top5", report.rebus.top_k_accuracy),
        ("language", "top1", report.language.top1_accuracy),
        ("language", "top3", report.language.top3_accuracy),
        ("search", "recall_at_1", report.ranking.recall_at_1),
        ("search", "recall_at_5", report.ranking.recall_at_5),
        ("search", "recall_at_10", report.ranking.recall_at_10),
        ("search", "mrr", report.ranking.mrr),
    ];
    let mut failures = Vec::new();
    let Some(sections) = gates.as_object() else {
        return failures;
    };
    for (section, metric, value) in observed {
        let key = format!("{metric}_min");
        if let Some(minimum) = sections
            .get(section)
            .and_then(|value| value.get(&key))
            .and_then(serde_json::Value::as_f64)
        {
            if value < minimum {
                failures.push(format!(
                    "{section}.{metric}={value:.3} below minimum {minimum:.3}"
                ));
            }
        }
    }
    failures
}

fn run_eval(args: &[String], json: bool) -> Result<i32, Box<dyn std::error::Error>> {
    let path = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("data/evaluation.json");
    let split = flag_value(args, "--split");
    let profile_name = flag_value(args, "--profile");
    let gates_path = flag_value(args, "--gates");
    let no_ranking = args.iter().any(|arg| arg == "--no-ranking");
    let source = std::fs::read_to_string(path)?;
    let dataset = EvaluationDataset::from_json(&source)?;
    let mut engine = TextIntelligence::new(EngineConfig::default());
    if let Some(name) = &profile_name {
        let profile = profile_named(name).ok_or_else(|| format!("unknown profile '{name}'"))?;
        engine = engine.with_similarity_profile(profile);
    }
    let options = EvaluateOptions {
        split: split.clone(),
        ranking_queries: if no_ranking { 0 } else { 100 },
        ranking_documents: 500,
    };
    let report = textintel::evaluation::evaluate_with_options(&engine, &dataset, &options)?;
    if json {
        print_json(&report)?;
    } else {
        print_eval_human(&report);
    }
    if let Some(gates_path) = gates_path {
        let gates_source = std::fs::read_to_string(&gates_path)?;
        let gates: serde_json::Value = serde_json::from_str(&gates_source)?;
        let failures = check_gates(&report, &gates);
        if failures.is_empty() {
            if !json {
                println!("quality gates: pass");
            }
        } else {
            for failure in &failures {
                eprintln!("gate failure: {failure}");
            }
            return Ok(1);
        }
    }
    Ok(0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let json = args.iter().any(|arg| arg == "--json");
    args.retain(|arg| arg != "--json");
    let Some(command) = args.first().map(String::as_str) else {
        eprintln!("{}", usage());
        std::process::exit(2);
    };
    // Flags with values must not be mistaken for positional arguments.
    let positionals = positional_args(&args);
    let engine = TextIntelligence::new(EngineConfig::default());
    let exit_code = match command {
        "analyze" | "explain" => {
            let text = positionals.get(1).ok_or("analyze requires <text>")?;
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
            0
        }
        "decode" => {
            let text = positionals.get(1).ok_or("decode requires <text>")?;
            let languages = flag_value(&args, "--languages")
                .map(|value| {
                    value
                        .split(',')
                        .map(|part| part.trim().to_string())
                        .filter(|part| !part.is_empty())
                        .collect::<Vec<_>>()
                })
                .filter(|values| !values.is_empty());
            let result = engine.decode_with_languages(text, languages.as_deref(), None)?;
            if json {
                print_json(&result)?;
            } else {
                for candidate in result {
                    println!("{:.3}\t{}", candidate.score, candidate.text);
                }
            }
            0
        }
        "compare" => {
            let left = positionals
                .get(1)
                .ok_or("compare requires <message-a> <message-b>")?;
            let right = positionals
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
            0
        }
        "spam" => {
            let text = positionals.get(1).ok_or("spam requires <text>")?;
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
            0
        }
        "batch" => {
            let path = positionals.get(1).ok_or("batch requires <input.jsonl>")?;
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
            0
        }
        "resources" => {
            if positionals.get(1).map(String::as_str) == Some("validate") {
                let path = positionals
                    .get(2)
                    .ok_or("resources validate requires <path>")?;
                let mut loader = ResourceLoader::with_limits(Default::default());
                let result = loader
                    .load_language_file(path)
                    .or_else(|_| loader.load_symbol_file(path));
                let report = match result {
                    Ok(()) => serde_json::json!({"path": path, "valid": true, "issues": []}),
                    Err(error) => {
                        serde_json::json!({"path": path, "valid": false, "issues": [error.to_string()]})
                    }
                };
                if json {
                    print_json(&report)?;
                } else {
                    println!("valid: {}", report["valid"]);
                    if let Some(issues) = report["issues"].as_array() {
                        for issue in issues {
                            println!("- {issue}");
                        }
                    }
                }
                return Ok(());
            }
            let root = positionals
                .get(1)
                .map(String::as_str)
                .unwrap_or("resources");
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
            0
        }
        "diagnostics" | "provider-info" => {
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
            0
        }
        "schema-version" => {
            let report = serde_json::json!({
                "api_version": textintel::API_VERSION,
                "fingerprint_schema_version": textintel::FINGERPRINT_SCHEMA_VERSION,
            });
            if json {
                print_json(&report)?;
            } else {
                println!("api: {}", textintel::API_VERSION);
                println!(
                    "fingerprint_schema: {}",
                    textintel::FINGERPRINT_SCHEMA_VERSION
                );
            }
            0
        }
        "evaluate" | "eval" => run_eval(&args, json)?,
        "index" => {
            let store = positionals
                .get(1)
                .ok_or("index requires <store.json> <id> <text>")?;
            let id = positionals
                .get(2)
                .ok_or("index requires <store.json> <id> <text>")?;
            let text = positionals
                .get(3..)
                .ok_or("index requires <store.json> <id> <text>")?
                .join(" ");
            let indexed = TextIntelligence::new(EngineConfig::default()).with_json_store(store)?;
            indexed.add_document(id, &text)?;
            println!("indexed {}", id);
            0
        }
        "search" => {
            let store = positionals
                .get(1)
                .ok_or("search requires <store.json> <text> <limit>")?;
            let text = positionals
                .get(2)
                .ok_or("search requires <store.json> <text> <limit>")?;
            let limit = positionals
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
            0
        }
        _ => {
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    };
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

/// Positional arguments with `--flag value` pairs removed so dataset paths
/// and texts are not confused with option values.
fn positional_args(args: &[String]) -> Vec<String> {
    let mut output = Vec::new();
    let mut skip_next = false;
    for (index, arg) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if index > 0
            && (arg == "--split" || arg == "--profile" || arg == "--gates" || arg == "--languages")
        {
            skip_next = true;
            continue;
        }
        if arg == "--no-ranking" {
            continue;
        }
        output.push(arg.clone());
    }
    output
}
