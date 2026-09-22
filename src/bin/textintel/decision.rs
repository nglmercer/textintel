//! The `decide` / `classify` / `decision-model-info` / `eval-decision`
//! subcommands: typed decisions through the configured adapters, task
//! files, model introspection, and decision-dataset evaluation.

use std::sync::Arc;

use textintel::decision::{
    DecisionDataset, DecisionProvider, DecisionQuestion, DecisionRequest,
    SimilarityDecisionProvider, SpamDecisionProvider, check_decision_gates, evaluate_decisions,
    selective_decision,
};

use super::{build_engine, flag_value, print_json};

fn decision_provider_named(
    args: &[String],
    name: &str,
) -> Result<Arc<dyn DecisionProvider>, Box<dyn std::error::Error>> {
    match name {
        "spam" => {
            let predictor: Arc<dyn textintel::SpamPredictor> = match decision_spam_artifact(args)? {
                Some(predictor) => Arc::new(predictor),
                None => Arc::new(textintel::HeuristicSpamPredictor),
            };
            Ok(Arc::new(SpamDecisionProvider::new(predictor)))
        }
        "similarity" => {
            let scorer: Arc<dyn textintel::SimilarityScorer> =
                match decision_similarity_artifact(args)? {
                    Some(scorer) => Arc::new(scorer),
                    None => Arc::new(textintel::ProfileSimilarityScorer::default()),
                };
            Ok(Arc::new(SimilarityDecisionProvider::new(scorer)))
        }
        #[cfg(feature = "semantic-transformer")]
        "interaction" => {
            let head = flag_value(args, "--head")
                .ok_or("interaction provider requires --head <artifact.json>")?;
            let embeddings_dir = flag_value(args, "--embeddings")
                .ok_or("interaction provider requires --embeddings <dir>")?;
            let artifact = textintel::InteractionArtifact::from_file(&head)
                .map_err(|error| format!("invalid interaction head: {error}"))?;
            let backbone = Arc::new(
                textintel::TransformerEmbeddingProvider::open(&embeddings_dir)
                    .map_err(|error| error.to_string())?,
            );
            let provider = textintel::InteractionDecisionProvider::new(backbone, &artifact)
                .map_err(|error| format!("invalid interaction head: {error}"))?;
            Ok(Arc::new(provider))
        }
        other => {
            #[cfg(feature = "semantic-transformer")]
            let options = "spam|similarity|interaction";
            #[cfg(not(feature = "semantic-transformer"))]
            let options = "spam|similarity";
            Err(format!("unknown decision provider '{other}' (use {options})").into())
        }
    }
}

/// Best-effort trained spam artifact for decision CLIs: `--model-path`
/// (or `./models`) `spam-v2.json` when present — invalid files fail
/// loudly, a missing file falls back to the heuristic predictor.
fn decision_spam_artifact(
    args: &[String],
) -> Result<Option<textintel::TrainedSpamPredictor>, Box<dyn std::error::Error>> {
    let dir = flag_value(args, "--model-path").unwrap_or_else(|| "models".to_string());
    let path = textintel::engine::preferred_spam_artifact_in(std::path::Path::new(&dir));
    if !path.is_file() {
        return Ok(None);
    }
    let source = std::fs::read_to_string(&path)?;
    let artifact = textintel::SpamModelArtifact::from_json(&source)
        .map_err(|error| format!("invalid spam artifact {}: {error}", path.display()))?;
    Ok(Some(artifact.to_predictor()))
}

/// Best-effort trained similarity artifact (same semantics as
/// [`decision_spam_artifact`]); a missing file falls back to the
/// deterministic profile scorer.
fn decision_similarity_artifact(
    args: &[String],
) -> Result<Option<textintel::LogisticSimilarityScorer>, Box<dyn std::error::Error>> {
    let dir = flag_value(args, "--model-path").unwrap_or_else(|| "models".to_string());
    let path = textintel::engine::preferred_similarity_artifact_in(std::path::Path::new(&dir));
    if !path.is_file() {
        return Ok(None);
    }
    let source = std::fs::read_to_string(&path)?;
    let artifact = textintel::SimilarityModelArtifact::from_json(&source)
        .map_err(|error| format!("invalid scorer artifact {}: {error}", path.display()))?;
    Ok(Some(artifact.to_scorer()))
}

fn print_decision_human(response: &textintel::DecisionResponse) {
    match &response.answer {
        textintel::DecisionAnswer::Choice {
            choice,
            confidence,
            probabilities,
        } => {
            println!("choice: {choice}");
            println!("confidence: {confidence:.3}");
            let mut entries: Vec<(&String, &f64)> = probabilities.iter().collect();
            entries.sort_by(|left, right| right.1.total_cmp(left.1));
            for (id, probability) in entries {
                println!("  {id}: {probability:.3}");
            }
        }
        textintel::DecisionAnswer::Binary {
            probability_true,
            probability_false,
            confidence,
        } => {
            println!("probability_true: {probability_true:.3}");
            println!("probability_false: {probability_false:.3}");
            println!("confidence: {confidence:.3}");
        }
        textintel::DecisionAnswer::Score {
            expected_score,
            confidence,
            probabilities,
        } => {
            println!("expected_score: {expected_score:.3}");
            println!("confidence: {confidence:.3}");
            for (index, probability) in probabilities.iter().enumerate() {
                println!("  {index}: {probability:.3}");
            }
        }
    }
    println!(
        "decision: {}",
        match response.decision {
            textintel::Decision::Accept => "accept",
            textintel::Decision::Escalate => "escalate",
        }
    );
    println!("provider: {}", response.provider);
}

pub(crate) fn run_decide(
    args: &[String],
    positionals: &[String],
    json: bool,
    production: bool,
) -> Result<i32, Box<dyn std::error::Error>> {
    let path = positionals.get(1).ok_or("decide requires <request.json>")?;
    let source = std::fs::read_to_string(path)?;
    let request: DecisionRequest = serde_json::from_str(&source)
        .map_err(|error| format!("invalid decision request {path}: {error}"))?;
    request
        .validate()
        .map_err(|error| format!("invalid decision request {path}: {error}"))?;
    // Auto-select the adapter by question type unless `--provider` pins one.
    // Score questions have no v1 provider and fail with an explicit error.
    let provider_name = flag_value(args, "--provider").unwrap_or_else(|| match &request.question {
        DecisionQuestion::Choice { .. } => "similarity".to_string(),
        DecisionQuestion::Binary { .. } => "spam".to_string(),
        DecisionQuestion::Score { .. } => "none".to_string(),
    });
    if provider_name == "none" {
        return Err(
            "no v1 provider answers score questions (spam/similarity adapters cover binary/choice)"
                .into(),
        );
    }
    let provider = decision_provider_named(args, &provider_name)?;
    let engine = build_engine(args, production)?.with_decision_provider(provider);
    let mut response = engine.decide(&request).map_err(|error| error.to_string())?;
    if let Some(threshold) = flag_value(args, "--threshold") {
        let threshold: f64 = threshold.parse()?;
        if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
            return Err(
                format!("threshold {threshold} must be finite and within [0.0, 1.0]").into(),
            );
        }
        response.decision = selective_decision(response.confidence(), threshold);
    }
    if json {
        print_json(&response)?;
    } else {
        print_decision_human(&response);
    }
    Ok(0)
}

/// Resolve `--task` to a choice question: a direct file path, or
/// `models/decision-tasks/<name>.json`. Task files are [`DecisionQuestion`]
/// documents (a `task` label field is allowed and ignored).
fn load_task_question(args: &[String]) -> Result<DecisionQuestion, Box<dyn std::error::Error>> {
    let task = flag_value(args, "--task").ok_or("classify requires --task <name|path>")?;
    let candidate = std::path::PathBuf::from(&task);
    let fallback = std::path::Path::new("models")
        .join("decision-tasks")
        .join(format!("{task}.json"));
    let path = if candidate.is_file() {
        candidate.clone()
    } else {
        fallback.clone()
    };
    let source = std::fs::read_to_string(&path).map_err(|_| {
        format!(
            "unknown task '{task}': no file at {} or {}",
            candidate.display(),
            path.display()
        )
    })?;
    let mut question: DecisionQuestion = serde_json::from_str(&source)
        .map_err(|error| format!("invalid task {}: {error}", path.display()))?;
    if let Some(instructions) = flag_value(args, "--question") {
        match &mut question {
            DecisionQuestion::Choice {
                instructions: current,
                ..
            }
            | DecisionQuestion::Score {
                instructions: current,
                ..
            } => *current = instructions,
            DecisionQuestion::Binary { statement } => *statement = instructions,
        }
    }
    question
        .validate()
        .map_err(|error| format!("invalid task {}: {error}", path.display()))?;
    Ok(question)
}

pub(crate) fn run_classify(
    args: &[String],
    positionals: &[String],
    json: bool,
    production: bool,
) -> Result<i32, Box<dyn std::error::Error>> {
    let text = positionals.get(1).ok_or("classify requires <text>")?;
    let question = load_task_question(args)?;
    if !matches!(question, DecisionQuestion::Choice { .. }) {
        return Err("classify supports only choice tasks in v1".into());
    }
    let provider_name = flag_value(args, "--provider").unwrap_or_else(|| "similarity".to_string());
    let provider = decision_provider_named(args, &provider_name)?;
    let engine = build_engine(args, production)?.with_decision_provider(provider);
    let task = flag_value(args, "--task").unwrap_or_default();
    let request = DecisionRequest::new(text.clone(), question).with_task(task);
    let response = engine.decide(&request).map_err(|error| error.to_string())?;
    if json {
        print_json(&response)?;
    } else {
        print_decision_human(&response);
    }
    Ok(0)
}

pub(crate) fn run_decision_model_info(
    args: &[String],
    json: bool,
    production: bool,
) -> Result<i32, Box<dyn std::error::Error>> {
    if let Some(dir) = flag_value(args, "--model-dir") {
        let path = std::path::Path::new(&dir);
        let artifact = textintel::DecisionArtifact::from_dir(path)
            .map_err(|error| format!("invalid decision model {dir}: {error}"))?;
        let layout: Vec<serde_json::Value> = textintel::DecisionArtifact::expected_layout(path)
            .iter()
            .map(|file| {
                serde_json::json!({
                    "file": file.file_name().and_then(|name| name.to_str()),
                    "present": file.is_file(),
                })
            })
            .collect();
        let report = serde_json::json!({
            "artifact": artifact,
            "layout": layout,
        });
        if json {
            print_json(&report)?;
        } else {
            println!("kind: {}", artifact.kind);
            println!("architecture: {}", artifact.architecture);
            println!("backbone: {}", artifact.backbone.model_id);
            println!("decision_schema: {}", artifact.decision_schema_version);
            println!("feature_schema: {}", artifact.feature_schema_version);
            for entry in &layout {
                println!(
                    "  {}: {}",
                    entry["file"].as_str().unwrap_or("?"),
                    if entry["present"].as_bool().unwrap_or(false) {
                        "present"
                    } else {
                        "missing"
                    }
                );
            }
        }
        return Ok(0);
    }
    let provider_name = flag_value(args, "--provider").unwrap_or_else(|| "similarity".to_string());
    let provider = decision_provider_named(args, &provider_name)?;
    let info = provider.model_info();
    let capabilities = provider.capabilities();
    let _ = production;
    if json {
        print_json(&serde_json::json!({
            "model": info,
            "capabilities": capabilities,
        }))?;
    } else {
        println!("provider: {}", info.provider);
        println!("architecture: {}", info.architecture);
        println!("supported: {:?}", info.supported_questions);
        println!("local: {}", info.local);
        println!("max_candidates: {}", info.max_candidates);
        println!("quality: {:?}", capabilities.quality);
    }
    Ok(0)
}

pub(crate) fn run_eval_decision(
    args: &[String],
    positionals: &[String],
    json: bool,
    production: bool,
) -> Result<i32, Box<dyn std::error::Error>> {
    let path = positionals
        .get(1)
        .map(String::as_str)
        .unwrap_or("data/decision");
    let split = flag_value(args, "--split").unwrap_or_else(|| "test".to_string());
    let dataset = DecisionDataset::load_path(path).map_err(|error| error.to_string())?;
    let provider_name = flag_value(args, "--provider").unwrap_or_else(|| "similarity".to_string());
    let provider = decision_provider_named(args, &provider_name)?;
    let engine = build_engine(args, production)?;
    let report = evaluate_decisions(
        &engine,
        provider.as_ref(),
        &dataset.version,
        &split,
        dataset.split(&split),
    )
    .map_err(|error| error.to_string())?;
    if json {
        print_json(&report)?;
    } else {
        println!(
            "dataset: {} split: {}",
            report.dataset_version, report.split
        );
        println!("provider: {}", report.provider);
        println!(
            "n={} skipped={} accuracy={:.3} macro_f1={:.3} micro_f1={:.3}",
            report.count, report.skipped, report.accuracy, report.macro_f1, report.micro_f1
        );
        println!(
            "nll={:.3} brier={:.3} ece={:.3} latency_mean={:.0}us",
            report.nll, report.brier, report.ece, report.mean_latency_micros
        );
        println!("coverage:");
        for point in &report.coverage {
            println!(
                "  {:.0}%: accuracy={:.3} n={} threshold={:.3}",
                point.coverage * 100.0,
                point.accuracy,
                point.count,
                point.threshold
            );
        }
    }
    if let Some(gates_path) = flag_value(args, "--gates") {
        let gates_source = std::fs::read_to_string(&gates_path)?;
        let gates: serde_json::Value = serde_json::from_str(&gates_source)?;
        let failures = check_decision_gates(&report, &gates);
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
