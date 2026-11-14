//! Exercise speech commands through the shipped binary, including exit codes.
use serde_json::{Value, json};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use textintel::speech::{SpeechDocument, SpeechNormalization, SpeechValue};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_textintel"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .expect("run textintel binary")
}

fn successful_json(args: &[&str]) -> Value {
    let output = run(args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("speech command emits valid JSON")
}

#[test]
fn parsing_and_normalization_require_an_explicit_locale() {
    for command in ["parse-speech", "normalize-speech"] {
        let output = run(&[command, "42", "--json"]);
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("locale"));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn parse_json_keeps_typed_values_and_utf8_byte_offsets() {
    let text = "🙂 café ٤٢ y 2024-02-29.";
    let json = successful_json(&["parse-speech", text, "--locale", "es", "--json"]);
    let document: SpeechDocument = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(document.text, text);
    assert_eq!(json["locale"], "es");
    let mut cursor = 0;
    let mut reconstructed = String::new();
    for span in &document.spans {
        assert_eq!(span.start, cursor);
        assert_eq!(text.get(span.start..span.end), Some(span.source.as_str()));
        reconstructed.push_str(&span.source);
        cursor = span.end;
    }
    assert_eq!(cursor, text.len());
    assert_eq!(reconstructed, text);
    let number = document
        .spans
        .iter()
        .find(|span| span.source == "٤٢")
        .unwrap();
    assert_eq!(number.start, text.find("٤٢").unwrap());
    assert_eq!(number.end - number.start, "٤٢".len());
    assert!(
        matches!(&number.value, SpeechValue::Number { value, .. } if value.integer == "42" && value.fraction.is_none())
    );
    assert!(document.spans.iter().any(|span| matches!(
        span.value,
        SpeechValue::Date {
            year: 2024,
            month: 2,
            day: 29
        }
    )));
    assert!(
        json["spans"]
            .as_array()
            .unwrap()
            .iter()
            .any(|span| span["value"]["type"] == "number")
    );
}

#[test]
fn ambiguous_dates_and_decimals_require_cli_policy() {
    let text = "03/04/2024 1,234 $12";
    let document: SpeechDocument = serde_json::from_value(successful_json(&[
        "parse-speech",
        text,
        "--locale",
        "en",
        "--date-order",
        "reject",
        "--json",
    ]))
    .unwrap();
    for source in ["03/04/2024", "1,234", "$12"] {
        assert!(
            document
                .spans
                .iter()
                .any(|span| span.source == source
                    && matches!(span.value, SpeechValue::Ambiguous { .. })),
            "{source} must not be guessed"
        );
    }
    let explicit: SpeechDocument = serde_json::from_value(successful_json(&[
        "parse-speech",
        "03/04/2024 1,234",
        "--locale",
        "en",
        "--date-order",
        "dmy",
        "--decimal-separator",
        "comma",
        "--json",
    ]))
    .unwrap();
    assert!(explicit.spans.iter().any(|span| matches!(
        span.value,
        SpeechValue::Date {
            year: 2024,
            month: 4,
            day: 3
        }
    )));
    assert!(explicit.spans.iter().any(|span| matches!(&span.value, SpeechValue::Number { value, .. } if value.integer == "1" && value.fraction.as_deref() == Some("234"))));
}

#[test]
fn normalization_json_reports_supported_and_unresolved_readings() {
    let known: SpeechNormalization = serde_json::from_value(successful_json(&[
        "normalize-speech",
        "🙂 42!",
        "--locale",
        "en",
        "--json",
    ]))
    .unwrap();
    assert_eq!(known.text, "🙂 forty-two!");
    assert!(known.fully_supported);
    let number = known
        .readings
        .iter()
        .find(|reading| reading.source == "42")
        .unwrap();
    assert_eq!(number.start, "🙂 ".len());
    assert_eq!(number.end, "🙂 42".len());
    assert_eq!(number.spoken, "forty-two");

    let original = "03/04/2024 $12";
    let unresolved: SpeechNormalization = serde_json::from_value(successful_json(&[
        "normalize-speech",
        original,
        "--locale",
        "en",
        "--json",
    ]))
    .unwrap();
    assert_eq!(unresolved.text, original);
    assert!(!unresolved.fully_supported);
    assert_eq!(
        unresolved
            .readings
            .iter()
            .filter(|reading| !reading.supported && reading.reason.is_some())
            .count(),
        2
    );
    let plain = run(&["normalize-speech", "$12", "--locale", "en"]);
    assert!(plain.status.success());
    assert_eq!(String::from_utf8(plain.stdout).unwrap().trim_end(), "$12");
    assert!(String::from_utf8_lossy(&plain.stderr).contains("unresolved"));
}

#[test]
fn regression_corpus_passes_through_cli_without_losing_cases() {
    let fixture: Value =
        serde_json::from_str(include_str!("../data/speech/evaluation.json")).unwrap();
    let report = successful_json(&["eval-speech", "data/speech/evaluation.json", "--json"]);
    assert_eq!(
        report["totals"]["cases"].as_u64().unwrap() as usize,
        fixture["cases"].as_array().unwrap().len()
    );
    assert_eq!(
        report["totals"]["parse_exact"],
        report["totals"]["parse_cases"]
    );
    assert_eq!(
        report["totals"]["spoken_exact"],
        report["totals"]["spoken_cases"]
    );
    assert!(report["failures"].as_array().unwrap().is_empty());
}

struct TemporaryCorpus(PathBuf);
impl TemporaryCorpus {
    fn new(value: &Value) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "textintel-speech-cli-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(serde_json::to_string(value).unwrap().as_bytes())
            .unwrap();
        Self(path)
    }
}
impl Drop for TemporaryCorpus {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn evaluation_fails_on_wrong_gold_and_retains_unsupported_cases() {
    let case = |id, text, spoken| {
        json!({
            "id": id, "locale": "en", "text": text, "date_order": "reject_ambiguous",
            "decimal_separator": null, "category": "external-challenge", "expected_spans": null,
            "spoken": [spoken],
        })
    };
    let corpus = TemporaryCorpus::new(&json!({
        "version": "intentional-cli-failure", "purpose": "Verify failures remain in denominators",
        "cases": [
            case("correct", "42", "forty-two"),
            case("wrong-gold", "42", "deliberately incorrect gold"),
            case("unsupported", "https://example.test", "this unsupported case must stay in the denominator"),
        ],
    }));
    let output = run(&["eval-speech", corpus.0.to_str().unwrap(), "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["totals"]["cases"], 3);
    assert_eq!(report["totals"]["parse_cases"], 0);
    assert_eq!(report["totals"]["spoken_cases"], 3);
    assert_eq!(report["totals"]["spoken_exact"], 1);
    assert_eq!(report["totals"]["fully_supported"], 2);
    assert_eq!(report["by_locale"]["en"]["cases"], 3);
    let failed_ids: Vec<_> = report["failures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|failure| failure["id"].as_str().unwrap())
        .collect();
    assert_eq!(failed_ids, ["wrong-gold", "unsupported"]);
    assert!(
        !report["unresolved_by_reason"]
            .as_object()
            .unwrap()
            .is_empty()
    );
}
