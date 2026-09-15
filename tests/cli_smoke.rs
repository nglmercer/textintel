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
    assert!(value["providers"]["symbols"]["provider"]
        .as_str()
        .unwrap()
        .contains("resource"));
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
