//! The `eval`/`evaluate` subcommand: dataset scoring, human/JSON reports,
//! and quality-gate enforcement.

use textintel::comparison::SimilarityProfile;
use textintel::evaluation::{EvaluateOptions, EvaluationDataset, EvaluationReport, SpamCorpus};

use super::{build_engine, flag_value, print_json};

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
    if let Some(spam) = &report.spam {
        println!(
            "spam: accuracy={:.3} f1={:.3} roc_auc={:.3} brier={:.3} ece={:.3} (n={} corpus={})",
            spam.metrics.accuracy,
            spam.metrics.f1,
            spam.metrics.roc_auc,
            spam.metrics.brier,
            spam.metrics.expected_calibration_error,
            spam.metrics.count,
            spam.corpus_version
        );
    }
    if !report.categories.is_empty() {
        println!("categories:");
        let mut names: Vec<&String> = report.categories.keys().collect();
        names.sort();
        for name in names {
            let metrics = &report.categories[name];
            println!(
                "  {name}: n={} f1={:.3} acc={:.3} roc_auc={:.3} pr_auc={:.3}",
                metrics.count, metrics.f1, metrics.accuracy, metrics.roc_auc, metrics.pr_auc
            );
        }
    }
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

/// Quality-gate file shape: `{ "<section>": { "<metric>_min": f64,
/// "<metric>_max": f64 } }`. Unknown sections and metrics are ignored so
/// gates stay forwards compatible.
fn check_gates(report: &EvaluationReport, gates: &serde_json::Value) -> Vec<String> {
    let mut observed: Vec<(&str, &str, f64)> = vec![
        ("similarity", "roc_auc", report.metrics.roc_auc),
        ("similarity", "pr_auc", report.metrics.pr_auc),
        ("similarity", "f1", report.metrics.f1),
        ("similarity", "accuracy", report.metrics.accuracy),
        ("similarity", "brier", report.metrics.brier),
        (
            "similarity",
            "ece",
            report.metrics.expected_calibration_error,
        ),
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
    if let Some(spam) = &report.spam {
        observed.extend([
            ("spam", "roc_auc", spam.metrics.roc_auc),
            ("spam", "pr_auc", spam.metrics.pr_auc),
            ("spam", "f1", spam.metrics.f1),
            ("spam", "accuracy", spam.metrics.accuracy),
            ("spam", "brier", spam.metrics.brier),
            ("spam", "ece", spam.metrics.expected_calibration_error),
        ]);
    }
    let mut failures = Vec::new();
    let Some(sections) = gates.as_object() else {
        return failures;
    };
    // Spam gates fail closed when the corpus was not measured (missing
    // file): `spam` is the only conditionally-measured section, so it is
    // the only one checked here; genuinely unknown sections stay ignored.
    if report.spam.is_none() && sections.contains_key("spam") {
        failures.push("spam: no metrics measured (missing spam corpus?)".to_string());
    }
    for (section, metric, value) in observed {
        let section_gates = sections.get(section);
        let key = format!("{metric}_min");
        if let Some(minimum) = section_gates
            .and_then(|value| value.get(&key))
            .and_then(serde_json::Value::as_f64)
        {
            if value < minimum {
                failures.push(format!(
                    "{section}.{metric}={value:.3} below minimum {minimum:.3}"
                ));
            }
        }
        let key = format!("{metric}_max");
        if let Some(maximum) = section_gates
            .and_then(|value| value.get(&key))
            .and_then(serde_json::Value::as_f64)
        {
            if value > maximum {
                failures.push(format!(
                    "{section}.{metric}={value:.3} above maximum {maximum:.3}"
                ));
            }
        }
    }
    // Per-category gates: `{ "categories": { "<name>": { "<metric>_min": f64,
    // "count_min": n } } }`. Missing categories fail only when `count_min`
    // is positive, so gates can require coverage without pinning metrics.
    if let Some(categories) = sections
        .get("categories")
        .and_then(|value| value.as_object())
    {
        for (name, thresholds) in categories {
            let Some(thresholds) = thresholds.as_object() else {
                continue;
            };
            let observed = report.categories.get(name);
            if let Some(count_min) = thresholds.get("count_min").and_then(|v| v.as_u64()) {
                let count = observed.map(|metrics| metrics.count as u64).unwrap_or(0);
                if count < count_min {
                    failures.push(format!(
                        "categories.{name}.count={count} below minimum {count_min}"
                    ));
                    continue;
                }
            }
            let Some(metrics) = observed else { continue };
            for (metric, value) in [
                ("accuracy", metrics.accuracy),
                ("f1", metrics.f1),
                ("roc_auc", metrics.roc_auc),
                ("pr_auc", metrics.pr_auc),
                ("brier", metrics.brier),
                ("ece", metrics.expected_calibration_error),
            ] {
                let key = format!("{metric}_min");
                if let Some(minimum) = thresholds.get(&key).and_then(|v| v.as_f64()) {
                    if value < minimum {
                        failures.push(format!(
                            "categories.{name}.{metric}={value:.3} below minimum {minimum:.3}"
                        ));
                    }
                }
                let key = format!("{metric}_max");
                if let Some(maximum) = thresholds.get(&key).and_then(|v| v.as_f64()) {
                    if value > maximum {
                        failures.push(format!(
                            "categories.{name}.{metric}={value:.3} above maximum {maximum:.3}"
                        ));
                    }
                }
            }
        }
    }
    failures
}

pub(crate) fn run_eval(
    args: &[String],
    json: bool,
    production: bool,
) -> Result<i32, Box<dyn std::error::Error>> {
    let path = args.get(1).map(String::as_str).unwrap_or("data/evaluation");
    let split = flag_value(args, "--split");
    let profile_name = flag_value(args, "--profile");
    let gates_path = flag_value(args, "--gates");
    let no_ranking = args.iter().any(|arg| arg == "--no-ranking");
    let dataset = EvaluationDataset::load_path(path).map_err(|error| error.to_string())?;
    let mut engine = build_engine(args, production)?;
    if let Some(name) = &profile_name {
        let profile = profile_named(name).ok_or_else(|| format!("unknown profile '{name}'"))?;
        engine = engine.with_similarity_profile(profile);
    }
    if let Some(path) = flag_value(args, "--scorer") {
        let source = std::fs::read_to_string(&path)?;
        let artifact = textintel::SimilarityModelArtifact::from_json(&source)
            .map_err(|error| format!("invalid scorer artifact {path}: {error}"))?;
        engine = engine.with_similarity_scorer(artifact.to_scorer());
    }
    // Held-out spam corpus: explicit `--spam-corpus` wins, otherwise the
    // `spam/v2-eval.json` sibling of the dataset directory. Absent or
    // unreadable resolves to `None` (no spam metrics); production gates
    // requiring spam fail closed on the missing section.
    let spam_corpus_path = flag_value(args, "--spam-corpus").or_else(|| {
        let dataset_path = std::path::Path::new(path);
        let root = if dataset_path.is_dir() {
            dataset_path.parent()
        } else {
            dataset_path.parent()?.parent()
        }?;
        let candidate = root.join("spam").join("v2-eval.json");
        candidate
            .is_file()
            .then(|| candidate.to_string_lossy().to_string())
    });
    let spam_corpus = spam_corpus_path
        .as_deref()
        .and_then(|candidate| SpamCorpus::from_file(candidate).ok());
    let options = EvaluateOptions {
        split: split.clone(),
        ranking_queries: if no_ranking { 0 } else { 100 },
        ranking_documents: 500,
        spam_corpus,
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
