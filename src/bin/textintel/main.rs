use std::env;

use textintel::{EngineConfig, ResourceLoader, TextIntelligence};

mod eval;

use eval::run_eval;
fn usage() -> &'static str {
    "Usage:\n  textintel analyze <text> [--json] [--production] [--resource-root <dir>] [--model-path <dir>] [--language <code>]\n  textintel explain <text> [--json] [--production] [--resource-root <dir>] [--model-path <dir>] [--language <code>]\n  textintel decode <text> [--languages <es,en>] [--json] [--production] [--resource-root <dir>] [--model-path <dir>]\n  textintel compare <message-a> <message-b> [--json] [--production] [--resource-root <dir>] [--model-path <dir>] [--language <code>]\n  textintel duplicate <message-a> <message-b> [--threshold <0..1>] [--mode combined|near_exact|lexical|semantic|phonetic|decoded|visual] [--json] [--production] [--resource-root <dir>] [--model-path <dir>]\n  textintel spam <text> [--json] [--production] [--resource-root <dir>] [--model-path <dir>]\n  textintel batch <input.jsonl> [--json] [--production] [--resource-root <dir>] [--model-path <dir>]\n  textintel resources [resource-root] [--json]\n  textintel resources validate <path> [--json]\n  textintel diagnostics [--json] [--production] [--resource-root <dir>] [--model-path <dir>]\n  textintel provider-info [--json]\n  textintel schema-version [--json]\n  textintel eval <dataset> [--split train|validation|test] [--profile <name>] [--scorer <artifact.json>] [--gates <quality-gates.json>] [--spam-corpus <spam-eval.json>] [--no-ranking] [--json] [--production]\n  textintel evaluate <dataset> [--split train|validation|test] [--profile <name>] [--scorer <artifact.json>] [--gates <quality-gates.json>] [--spam-corpus <spam-eval.json>] [--no-ranking] [--json] [--production]\n  textintel index <store.json> <id> <text> [--production] [--resource-root <dir>] [--model-path <dir>] [--language <code>]\n  textintel search <store.json> <text> <limit> [--json] [--production] [--resource-root <dir>] [--model-path <dir>]\n\nJSON output contract: every --json payload follows API_VERSION (see\nschema-version); payloads evolve additively only — fields are added, never\nrenamed or removed, within a major version."
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

/// Language hints from `--language <code>` or `--languages <a,b>`.
fn language_hints(args: &[String]) -> Option<Vec<String>> {
    flag_value(args, "--languages")
        .or_else(|| flag_value(args, "--language"))
        .map(|value| {
            value
                .split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
}

/// Shared engine construction: default or `--production` preset, then
/// `--language` hints (decode/G2P preference), `--resource-root` (custom
/// packs), and `--model-path` (directory holding `similarity-v2.json` /
/// `spam-v1.json` / `reranker-v1.json`; present-but-invalid artifacts fail
/// loudly).
fn build_engine(
    args: &[String],
    production: bool,
) -> Result<TextIntelligence, Box<dyn std::error::Error>> {
    let mut config = EngineConfig::default();
    if let Some(hints) = language_hints(args) {
        config.language_hints = hints;
    }
    let mut engine = if production {
        TextIntelligence::builder()
            .config(config)
            .production_local()
            .build()
            .map_err(|error| error.to_string())?
    } else {
        TextIntelligence::new(config)
    };
    if let Some(root) = flag_value(args, "--resource-root") {
        engine = engine.with_resources(
            ResourceLoader::from_resource_root(&root).map_err(|error| error.to_string())?,
        );
    }
    if let Some(dir) = flag_value(args, "--model-path") {
        let similarity =
            textintel::engine::preferred_similarity_artifact_in(std::path::Path::new(&dir));
        if similarity.is_file() {
            let source = std::fs::read_to_string(&similarity)?;
            let artifact =
                textintel::SimilarityModelArtifact::from_json(&source).map_err(|error| {
                    format!("invalid scorer artifact {}: {error}", similarity.display())
                })?;
            engine = engine.with_similarity_scorer(artifact.to_scorer());
        }
        let spam = std::path::Path::new(&dir).join("spam-v1.json");
        if spam.is_file() {
            let source = std::fs::read_to_string(&spam)?;
            let artifact = textintel::SpamModelArtifact::from_json(&source)
                .map_err(|error| format!("invalid spam artifact {}: {error}", spam.display()))?;
            engine = engine.with_spam_predictor(artifact.to_predictor());
        }
        let reranker = std::path::Path::new(&dir).join("reranker-v1.json");
        if reranker.is_file() {
            let source = std::fs::read_to_string(&reranker)?;
            let artifact =
                textintel::RerankerModelArtifact::from_json(&source).map_err(|error| {
                    format!("invalid reranker artifact {}: {error}", reranker.display())
                })?;
            engine = engine.with_reranker_provider(
                artifact
                    .to_reranker(64)
                    .map_err(|error| format!("invalid reranker weights: {error}"))?,
            );
        }
    }
    Ok(engine)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let json = args.iter().any(|arg| arg == "--json");
    args.retain(|arg| arg != "--json");
    // `--production` selects the local production preset (resource packs,
    // trained models when present, espeak-ng with fallback). It never
    // downloads models or touches the network.
    let production = args.iter().any(|arg| arg == "--production");
    args.retain(|arg| arg != "--production");
    let Some(command) = args.first().map(String::as_str) else {
        eprintln!("{}", usage());
        std::process::exit(2);
    };
    // Identity flags answer before any engine is built.
    match command {
        "--version" | "-V" => {
            println!("textintel {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        "--help" | "-h" => {
            println!("{}", usage());
            return Ok(());
        }
        _ => {}
    }
    // Flags with values must not be mistaken for positional arguments.
    let positionals = positional_args(&args);
    let engine = build_engine(&args, production)?;
    let exit_code = match command {
        "analyze" => {
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
        "explain" => {
            let text = positionals.get(1).ok_or("explain requires <text>")?;
            let result = engine.analyze(text)?;
            if json {
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
        "duplicate" => {
            let left = positionals
                .get(1)
                .ok_or("duplicate requires <message-a> <message-b>")?;
            let right = positionals
                .get(2)
                .ok_or("duplicate requires <message-a> <message-b>")?;
            let threshold = flag_value(&args, "--threshold")
                .map(|value| value.parse::<f64>())
                .transpose()?
                .unwrap_or(0.85);
            let mode = match flag_value(&args, "--mode").as_deref() {
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
            if json {
                print_json(&result)?;
            } else {
                println!(
                    "duplicate: {} score: {:.3} reason: {}",
                    result.duplicate, result.score, result.reason
                );
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
            let diagnostics = engine.diagnostics();
            let report = serde_json::json!({
                "api_version": textintel::API_VERSION,
                "fingerprint_schema_version": textintel::FINGERPRINT_SCHEMA_VERSION,
                "providers": engine.provider_capabilities(),
                "embedding_model": diagnostics.embedding_model,
                "ann_enabled": diagnostics.ann_enabled,
                "store_capabilities": diagnostics.store_capabilities,
                "candidate_budgets": diagnostics.candidate_budgets,
                "caches": diagnostics.caches,
                "degraded": diagnostics.degraded,
                "symbol_languages": diagnostics.symbol_languages,
                "abbreviation_languages": diagnostics.abbreviation_languages,
            });
            if json {
                print_json(&report)?;
            } else {
                println!("api: {}", textintel::API_VERSION);
                println!("providers: {:?}", engine.provider_capabilities());
                println!("embedding_model: {:?}", diagnostics.embedding_model);
                println!("ann_enabled: {}", diagnostics.ann_enabled);
                println!("store: {:?}", diagnostics.store_capabilities);
                println!("candidate_budgets: {:?}", diagnostics.candidate_budgets);
                println!("caches: {:?}", diagnostics.caches);
                for item in &diagnostics.degraded {
                    println!(
                        "degraded: {} (serving {}; want {}): {}",
                        item.capability, item.configured, item.wanted, item.detail
                    );
                }
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
        "evaluate" | "eval" => run_eval(&args, json, production)?,
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
            let indexed = build_engine(&args, production)?.with_json_store(store)?;
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
            let indexed = build_engine(&args, production)?.with_json_store(store)?;
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
            && (arg == "--split"
                || arg == "--profile"
                || arg == "--gates"
                || arg == "--languages"
                || arg == "--language"
                || arg == "--scorer"
                || arg == "--spam-corpus"
                || arg == "--threshold"
                || arg == "--mode"
                || arg == "--model-path"
                || arg == "--resource-root")
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
