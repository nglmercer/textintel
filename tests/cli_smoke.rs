//! CLI smoke tests: new commands and flags work through the real binary.

use std::process::Command;

fn textintel() -> Command {
    Command::new(env!("CARGO_BIN_EXE_textintel"))
}

fn run_json(args: &[&str]) -> serde_json::Value {
    let output = textintel()
        .args(args)
        .output()
        .expect("run textintel binary");
    assert!(
        output.status.success(),
        "args {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("valid JSON output")
}

#[test]
fn duplicate_command_reports_verdicts() {
    let same = run_json(&["duplicate", "hello world", "hello world", "--json"]);
    assert_eq!(same["duplicate"], true);
    let different = run_json(&[
        "duplicate",
        "hello world",
        "goodbye",
        "--threshold",
        "0.9",
        "--json",
    ]);
    assert_eq!(different["duplicate"], false);
    assert!(different["score"].as_f64().unwrap() < 0.9);
}

#[test]
fn duplicate_rejects_unknown_modes() {
    let output = textintel()
        .args(["duplicate", "a", "b", "--mode", "bogus"])
        .output()
        .expect("run textintel binary");
    assert!(!output.status.success());
}

#[test]
fn engine_flags_flow_into_commands() {
    // Language hints are accepted everywhere an engine is built.
    let value = run_json(&["compare", "hola", "hola", "--language", "es", "--json"]);
    assert!(value["score"].as_f64().unwrap() > 0.9);
    // Resource roots load from disk.
    let value = run_json(&["diagnostics", "--resource-root", "resources", "--json"]);
    assert!(
        value["providers"]["symbols"]["provider"]
            .as_str()
            .unwrap()
            .contains("resource")
    );
    // Model paths attach trained artifacts (score differs from default).
    let plain = run_json(&["compare", "gr8", "great", "--json"]);
    let modeled = run_json(&[
        "compare",
        "gr8",
        "great",
        "--model-path",
        "models",
        "--json",
    ]);
    assert_ne!(plain["score"], modeled["score"]);
}

#[test]
fn explain_shows_transformation_chains() {
    let output = textintel()
        .args(["explain", "Fr4🏠d0", "--language", "es"])
        .output()
        .expect("run textintel binary");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Fracasado"), "missing decode: {text}");
    assert!(text.contains("🏠 = casa"), "missing chain step: {text}");
}

#[test]
fn version_and_help_flags_answer_without_engine() {
    for flag in ["--version", "-V"] {
        let output = textintel()
            .arg(flag)
            .output()
            .expect("run textintel binary");
        assert!(output.status.success(), "{flag} failed");
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            text.trim() == format!("textintel {}", env!("CARGO_PKG_VERSION")),
            "unexpected version output: {text}"
        );
    }
    for flag in ["--help", "-h"] {
        let output = textintel()
            .arg(flag)
            .output()
            .expect("run textintel binary");
        assert!(output.status.success(), "{flag} failed");
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("Usage:"), "missing usage: {text}");
    }
}

#[test]
fn decide_answers_a_request_file() {
    let path = std::env::temp_dir().join("textintel-decide-smoke.json");
    std::fs::write(
        &path,
        r#"{"state": "refund my duplicate payment", "question": {"type": "choice", "instructions": "Which team?", "criteria": {"billing": "Payments and refunds", "technical": "Product problems"}}}"#,
    )
    .expect("write request");
    let response = run_json(&[
        "decide",
        path.to_str().expect("path"),
        "--provider",
        "similarity",
        "--json",
    ]);
    assert_eq!(response["answer"]["type"], "choice");
    assert_eq!(response["provider"], "similarity_decision_adapter");
    let probabilities = response["answer"]["probabilities"]
        .as_object()
        .expect("probabilities");
    let sum: f64 = probabilities
        .values()
        .map(|value| value.as_f64().unwrap_or(0.0))
        .sum();
    assert!((sum - 1.0).abs() < 1e-6, "probabilities sum to {sum}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn classify_uses_task_files() {
    let response = run_json(&[
        "classify",
        "The app crashes on login",
        "--task",
        "support-routing",
        "--json",
    ]);
    assert_eq!(response["answer"]["type"], "choice");
    assert_eq!(response["task"], "support-routing");
    // Unknown tasks fail with a helpful error, never a wrong answer.
    let output = textintel()
        .args(["classify", "hello", "--task", "no-such-task"])
        .output()
        .expect("run textintel binary");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown task"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn decision_model_info_reports_adapters() {
    let info = run_json(&["decision-model-info", "--provider", "spam", "--json"]);
    assert_eq!(info["model"]["provider"], "spam_decision_adapter");
    assert_eq!(info["model"]["local"], true);
    assert!(info["model"]["supported_questions"].as_array().is_some());
}

#[test]
fn eval_decision_scores_the_seed_split() {
    let report = run_json(&[
        "eval-decision",
        "data/decision",
        "--split",
        "validation",
        "--provider",
        "similarity",
        "--json",
    ]);
    assert_eq!(report["split"], "validation");
    assert_eq!(report["count"], 6);
    assert_eq!(report["skipped"], 0);
    assert!(report["accuracy"].as_f64().unwrap() > 0.33);
    assert_eq!(report["coverage"].as_array().expect("coverage").len(), 6);
}

#[test]
fn eval_decision_enforces_gates() {
    let output = textintel()
        .args([
            "eval-decision",
            "data/decision",
            "--split",
            "test",
            "--gates",
            "data/quality-gates-decision.json",
        ])
        .output()
        .expect("run textintel binary");
    assert!(
        output.status.success(),
        "seed gates must pass: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("quality gates: pass"),
        "missing gate verdict"
    );
}

#[test]
fn json_outputs_are_versioned() {
    let schema = run_json(&["schema-version", "--json"]);
    assert!(schema["api_version"].is_string());
    let diagnostics = run_json(&["diagnostics", "--json"]);
    assert_eq!(diagnostics["api_version"], schema["api_version"]);
    let analyzed = run_json(&["analyze", "hello", "--json"]);
    assert_eq!(
        analyzed["schema_version"],
        schema["fingerprint_schema_version"]
    );
}
