//! The `eval`/`evaluate` subcommand: dataset scoring, human/JSON reports,
//! and quality-gate enforcement.

use textintel::cli::ParsedArgs;
use textintel::comparison::SimilarityProfile;
use textintel::evaluation::{
    EvaluateOptions, EvaluationDataset, EvaluationReport, SpamCorpus, check_gates,
};

use super::common::{build_engine, print_json};

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

pub(crate) fn run_eval(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let json = parsed.flag("json");
    let path = parsed.positional(0).unwrap_or("data/evaluation");
    let split = parsed.value("split").map(str::to_string);
    let profile_name = parsed.value("profile").map(str::to_string);
    let gates_path = parsed.value("gates").map(str::to_string);
    let no_ranking = parsed.flag("no-ranking");
    let dataset = EvaluationDataset::load_path(path).map_err(|error| error.to_string())?;
    let mut engine = build_engine(parsed)?;
    if let Some(name) = &profile_name {
        let profile = profile_named(name).ok_or_else(|| format!("unknown profile '{name}'"))?;
        engine = engine.with_similarity_profile(profile);
    }
    if let Some(path) = parsed.value("scorer") {
        let source = std::fs::read_to_string(path)?;
        let artifact = textintel::SimilarityModelArtifact::from_json(&source)
            .map_err(|error| format!("invalid scorer artifact {path}: {error}"))?;
        engine = engine.with_similarity_scorer(artifact.to_scorer());
    }
    // Held-out spam corpus: explicit `--spam-corpus` wins, otherwise the
    // `spam/v2-eval.json` sibling of the dataset directory. Absent or
    // unreadable resolves to `None` (no spam metrics); production gates
    // requiring spam fail closed on the missing section.
    let spam_corpus_path = parsed.value("spam-corpus").map(str::to_string).or_else(|| {
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
