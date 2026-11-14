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
    assert!(report["totals"].get("semantic").is_none());
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
fn semantic_evaluation_reports_exact_values_and_preserves_unannotated_denominators() {
    let corpus = TemporaryCorpus::new(&json!({
        "version":"semantic-cli", "purpose":"Independent value annotations, separate from span/spoken metrics",
        "cases":[
            {"id":"value", "locale":"en", "text":"42", "date_order":"reject_ambiguous",
             "decimal_separator":null, "category":"number", "expected_spans":null, "spoken":[],
             "expected_values":[{"start":0,"end":2,"source":"42","value":{
                 "type":"number","reading":"cardinal","value":{"negative":false,"integer":"42","fraction":null}
             }}]},
            {"id":"unannotated", "locale":"es", "text":"43", "date_order":"reject_ambiguous",
             "decimal_separator":null,"category":"number","expected_spans":null,"spoken":[]}
        ]
    }));
    let report = successful_json(&["eval-speech", corpus.0.to_str().unwrap(), "--json"]);
    assert_eq!(report["totals"]["cases"], 2);
    assert_eq!(report["totals"]["semantic"]["cases"], 1);
    assert_eq!(report["totals"]["semantic"]["exact"], 1);
    assert_eq!(report["totals"]["semantic"]["correct_spans"], 1);
    assert!(report["by_locale"]["es"].get("semantic").is_none());
    let text = run(&["eval-speech", corpus.0.to_str().unwrap()]);
    assert!(text.status.success());
    let output = String::from_utf8(text.stdout).unwrap();
    assert!(output.contains("semantic_exact=1/1 (100.00%)"));
    assert!(output.contains("value_spans=1/1 predicted=1 precision=100.00% recall=100.00%"));
    assert!(output.contains("en: semantic 1/1 values 1/1 predicted 1"));
    assert!(!output.contains("es: semantic"));
}

#[test]
fn semantic_failure_cannot_pass_with_correct_spoken_text_or_reviewed_variants() {
    for spoken in ["forty-two", "forty two"] {
        let corpus = TemporaryCorpus::new(&json!({
            "version":"wrong-semantic-cli","purpose":"Same recognized kind and utterance cannot hide a different value",
            "cases":[{"id":"wrong-value","locale":"en","text":"42","date_order":"reject_ambiguous",
                "decimal_separator":null,"category":"number","spoken":[spoken],
                "expected_spans":[{"start":0,"end":2,"source":"42","kind":"number"}],
                "expected_values":[{"start":0,"end":2,"source":"42","value":{
                    "type":"number","reading":"cardinal","value":{"negative":false,"integer":"43","fraction":null}
                }}]}]
        }));
        for accept in [false, true] {
            let mut args = vec!["eval-speech", corpus.0.to_str().unwrap(), "--json"];
            if accept {
                args.push("--accept-variants");
            }
            let output = run(&args);
            assert_eq!(output.status.code(), Some(1));
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["totals"]["parse_exact"], 1);
            assert_eq!(report["totals"]["semantic"]["exact"], 0);
            assert_eq!(report["totals"]["semantic"]["cases"], 1);
            assert_eq!(report["totals"]["spoken_accepted"], 1);
            assert_eq!(
                report["failures"][0]["expected_values"][0]["value"]["value"]["integer"],
                "43"
            );
            assert_eq!(
                report["failures"][0]["predicted_values"][0]["value"]["value"]["integer"],
                "42"
            );
        }
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

#[test]
fn reviewed_variant_gate_retains_literal_failures_in_json() {
    let corpus = TemporaryCorpus::new(&json!({
        "version":"cli-variant-gate","purpose":"Check an explicit gate without erasing original failures",
        "cases":[{"id":"spacing","locale":"de","text":"21","date_order":"reject_ambiguous","decimal_separator":null,
        "category":"number","expected_spans":null,"spoken":["ein und zwanzig"]}]
    }));
    let exact = run(&["eval-speech", corpus.0.to_str().unwrap(), "--json"]);
    assert_eq!(exact.status.code(), Some(1));
    let reviewed = successful_json(&[
        "eval-speech",
        corpus.0.to_str().unwrap(),
        "--accept-variants",
        "--json",
    ]);
    assert_eq!(reviewed["totals"]["spoken_exact"], 0);
    assert_eq!(reviewed["totals"]["spoken_variant"], 1);
    assert_eq!(reviewed["totals"]["spoken_accepted"], 1);
    assert_eq!(reviewed["failures"].as_array().unwrap().len(), 1);
    assert!(
        !reviewed["variant_matches"][0]["rules"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn speech_reading_controls_are_explicit_and_validate_values() {
    let written = successful_json(&[
        "normalize-speech",
        "1234",
        "--locale",
        "ja",
        "--number-notation",
        "orthographic",
        "--json",
    ]);
    assert_eq!(written["text"], "千二百三十四");
    let padded = successful_json(&[
        "normalize-speech",
        "03",
        "--locale",
        "ru",
        "--interpret-as",
        "number",
        "--json",
    ]);
    assert_eq!(padded["text"], "три");
    for (flag, value) in [
        ("--reading-style", "invented"),
        ("--number-notation", "invented"),
        ("--interpret-as", "invented"),
        ("--currency", "BADCODE"),
    ] {
        let output = run(&["normalize-speech", "42", "--locale", "en", flag, value]);
        assert!(!output.status.success(), "{flag} {value}");
    }
}

#[test]
fn currency_naming_cli_is_opt_in_and_preserves_parse_identity() {
    let natural = ["--locale", "en", "--reading-style", "natural", "--json"];
    let mut args = vec!["normalize-speech", "USD7"];
    args.extend(natural);
    let default = successful_json(&args);
    assert_eq!(default["text"], "seven dollars");
    for (naming, expected) in [("style", "seven dollars"), ("full", "seven US dollars")] {
        let mut args = vec!["normalize-speech", "USD7", "--currency-naming", naming];
        args.extend(natural);
        let normalized = successful_json(&args);
        assert_eq!(normalized["text"], expected);
        assert_eq!(normalized["fully_supported"], true);
        assert_eq!(normalized["readings"][0]["source"], "USD7");
        assert_eq!(normalized["readings"][0]["start"], 0);
        assert_eq!(normalized["readings"][0]["end"], 4);
    }
    let parse = |naming| {
        successful_json(&[
            "parse-speech",
            "USD0.05",
            "--locale",
            "en",
            "--currency-naming",
            naming,
            "--json",
        ])
    };
    let parsed: SpeechDocument = serde_json::from_value(parse("full")).unwrap();
    assert_eq!(parse("full"), parse("style"));
    assert!(matches!(
        &parsed.spans[0].value,
        SpeechValue::Currency { code, value }
            if code == "USD" && value.integer == "0" && value.fraction.as_deref() == Some("05")
    ));
    let minor = successful_json(&[
        "normalize-speech",
        "USD0.05",
        "--locale",
        "en",
        "--reading-style",
        "natural",
        "--currency-naming",
        "full",
        "--json",
    ]);
    assert_eq!(minor["text"], "zero US dollars five cents");
    assert_eq!(minor["fully_supported"], true);
    for command in ["parse-speech", "normalize-speech"] {
        let output = run(&[
            command,
            "USD7",
            "--locale",
            "en",
            "--currency-naming",
            "invented",
            "--json",
        ]);
        assert!(!output.status.success(), "{command}");
        assert!(output.stdout.is_empty(), "{command}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("currency naming"));
    }
}

#[test]
fn evaluation_cli_currency_naming_forwards_policy_and_overrides_file_settings() {
    let corpus = TemporaryCorpus::new(&json!({
        "version":"currency-cli-policy", "purpose":"Keep currency identity under an explicit naming policy",
        "cases":[
            {"id":"qualified-major","locale":"en","text":"USD7","date_order":"reject_ambiguous",
             "decimal_separator":".","category":"currency","expected_spans":null,"spoken":["seven US dollars"]},
            {"id":"qualified-minor","locale":"en","text":"USD0.05","date_order":"reject_ambiguous",
             "decimal_separator":".","category":"currency","expected_spans":null,"spoken":["zero US dollars five cents"]}
        ]
    }));
    // Old policy JSON omits currency_naming and must retain style naming.
    let old_policy = TemporaryCorpus::new(&json!({"reading_style":"natural"}));
    let full_policy = TemporaryCorpus::new(&json!({
        "reading_style":"natural", "currency_naming":"full"
    }));
    let file_full = successful_json(&[
        "eval-speech",
        corpus.0.to_str().unwrap(),
        "--speech-policy",
        full_policy.0.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(file_full["policy"]["currency_naming"], "full");
    assert_eq!(file_full["totals"]["spoken_exact"], 2);
    assert_eq!(file_full["totals"]["fully_supported"], 2);
    assert!(file_full["failures"].as_array().unwrap().is_empty());
    let flag_full = successful_json(&[
        "eval-speech",
        corpus.0.to_str().unwrap(),
        "--speech-policy",
        old_policy.0.to_str().unwrap(),
        "--currency-naming",
        "full",
        "--json",
    ]);
    assert_eq!(flag_full["policy"]["currency_naming"], "full");
    assert_eq!(flag_full["totals"]["spoken_exact"], 2);
    for (path, naming) in [
        (old_policy.0.to_str().unwrap(), None),
        (full_policy.0.to_str().unwrap(), Some("style")),
    ] {
        let mut args = vec![
            "eval-speech",
            corpus.0.to_str().unwrap(),
            "--speech-policy",
            path,
            "--json",
        ];
        if let Some(naming) = naming {
            args.extend(["--currency-naming", naming]);
        }
        let output = run(&args);
        assert_eq!(output.status.code(), Some(1));
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["totals"]["cases"], 2);
        assert_eq!(report["totals"]["spoken_exact"], 0);
        assert_eq!(report["totals"]["fully_supported"], 2);
        assert_eq!(report["failures"][0]["spoken"], "seven dollars");
        assert_eq!(report["failures"][1]["spoken"], "five cents");
        if naming.is_some() {
            assert_eq!(report["policy"]["currency_naming"], "style");
        } else {
            assert!(report["policy"]["currency_naming"].is_null());
        }
    }
    let invalid = run(&[
        "eval-speech",
        corpus.0.to_str().unwrap(),
        "--currency-naming",
        "invented",
        "--json",
    ]);
    assert!(!invalid.status.success());
    assert!(invalid.stdout.is_empty());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("currency naming"));
}

#[test]
fn clock_policy_cli_normalizes_only_explicit_compatible_periods_and_keeps_source() {
    use textintel::speech::temporal::TemporalValue;
    let strict = successful_json(&["normalize-speech", "13:10pm", "--locale", "zh", "--json"]);
    assert_eq!(strict["text"], "13:10pm");
    assert_eq!(strict["fully_supported"], false);
    assert_eq!(
        successful_json(&[
            "normalize-speech",
            "13:10pm",
            "--locale",
            "zh",
            "--clock-policy",
            "strict",
            "--json",
        ]),
        strict
    );
    let source = "🙂 13:10pm!";
    let parsed: SpeechDocument = serde_json::from_value(successful_json(&[
        "parse-speech",
        source,
        "--locale",
        "zh",
        "--reading-style",
        "natural",
        "--clock-policy",
        "normalize-redundant-period",
        "--json",
    ]))
    .unwrap();
    assert_eq!(parsed.text, source);
    let clock = parsed
        .spans
        .iter()
        .find(|span| span.source == "13:10pm")
        .unwrap();
    assert_eq!(clock.start, 5);
    assert_eq!(clock.end, 12);
    assert_eq!(source.get(clock.start..clock.end), Some("13:10pm"));
    assert!(matches!(
        &clock.value,
        SpeechValue::Temporal { value: TemporalValue::Clock {
            hour: 1, minute: Some(10), second: None, period: None, meridiem: Some(marker), half: false,
        }} if marker == "pm"
    ));
    // Precise keeps the established public 24-hour Time representation; its
    // 13:10 is semantically the same afternoon instant as canonical 1:10 PM.
    let precise: SpeechDocument = serde_json::from_value(successful_json(&[
        "parse-speech",
        "13:10pm",
        "--locale",
        "zh",
        "--clock-policy",
        "normalize-redundant-period",
        "--json",
    ]))
    .unwrap();
    assert!(matches!(
        precise.spans[0].value,
        SpeechValue::Time {
            hour: 13,
            minute: 10,
            second: None
        }
    ));
    assert_eq!(precise.spans[0].source, "13:10pm");
    let spoken: SpeechNormalization = serde_json::from_value(successful_json(&[
        "normalize-speech",
        source,
        "--locale",
        "zh",
        "--reading-style",
        "natural",
        "--clock-policy",
        "normalize-redundant-period",
        "--json",
    ]))
    .unwrap();
    assert_eq!(spoken.text, "🙂 一点十分pm!");
    assert!(spoken.fully_supported);
    let reading = spoken
        .readings
        .iter()
        .find(|r| r.source == "13:10pm")
        .unwrap();
    assert_eq!((reading.start, reading.end), (5, 12));

    for (source, hour, minute, marker) in [("0:05am", 12, 5, "am"), ("23:59pm", 11, 59, "pm")] {
        let parsed: SpeechDocument = serde_json::from_value(successful_json(&[
            "parse-speech",
            source,
            "--locale",
            "en",
            "--reading-style",
            "natural",
            "--clock-policy",
            "normalize-redundant-period",
            "--json",
        ]))
        .unwrap();
        assert_eq!(parsed.spans.len(), 1, "{parsed:?}");
        assert_eq!(parsed.spans[0].source, source);
        assert_eq!(
            (parsed.spans[0].start, parsed.spans[0].end),
            (0, source.len())
        );
        assert!(
            matches!(
                &parsed.spans[0].value,
                SpeechValue::Temporal { value: TemporalValue::Clock {
                    hour: h, minute: Some(m), meridiem: Some(actual), ..
                }} if *h == hour && *m == minute && actual == marker
            ),
            "{source}: {parsed:?}"
        );
    }
    for source in ["13:10am", "0:05pm", "24:00pm", "24:00am"] {
        for policy in ["strict", "normalize-redundant-period"] {
            let parsed: SpeechDocument = serde_json::from_value(successful_json(&[
                "parse-speech",
                source,
                "--locale",
                "en",
                "--clock-policy",
                policy,
                "--json",
            ]))
            .unwrap();
            assert_eq!(parsed.spans.len(), 1, "{source} {policy}: {parsed:?}");
            assert_eq!(parsed.spans[0].source, source);
            assert!(
                matches!(parsed.spans[0].value, SpeechValue::Ambiguous { .. }),
                "{source} {policy}: {parsed:?}"
            );
        }
    }
    for command in ["parse-speech", "normalize-speech"] {
        let invalid = run(&[
            command,
            "1:10pm",
            "--locale",
            "en",
            "--clock-policy",
            "invented",
            "--json",
        ]);
        assert!(!invalid.status.success());
        assert!(invalid.stdout.is_empty());
        assert!(String::from_utf8_lossy(&invalid.stderr).contains("clock policy"));
    }
}

#[test]
fn evaluation_cli_clock_policy_forwards_json_and_explicit_override() {
    let corpus = TemporaryCorpus::new(&json!({
        "version":"clock-cli-policy", "purpose":"Explicit tolerance changes interpretation while preserving source",
        "cases":[
            {"id":"afternoon","locale":"zh","text":"13:10pm","date_order":"reject_ambiguous",
             "decimal_separator":null,"category":"time","expected_spans":null,"spoken":["一点十分pm"]},
            {"id":"midnight","locale":"en","text":"0:05am","date_order":"reject_ambiguous",
             "decimal_separator":null,"category":"time","expected_spans":null,"spoken":["twelve oh five AM"]}
        ]
    }));
    let old_policy = TemporaryCorpus::new(&json!({"reading_style":"natural"}));
    let tolerant_policy = TemporaryCorpus::new(&json!({
        "reading_style":"natural", "clock_policy":"normalize_redundant_period"
    }));
    let from_file = successful_json(&[
        "eval-speech",
        corpus.0.to_str().unwrap(),
        "--speech-policy",
        tolerant_policy.0.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(
        from_file["policy"]["clock_policy"],
        "normalize_redundant_period"
    );
    assert_eq!(from_file["totals"]["spoken_exact"], 2);
    assert_eq!(from_file["totals"]["fully_supported"], 2);
    let from_flag = successful_json(&[
        "eval-speech",
        corpus.0.to_str().unwrap(),
        "--speech-policy",
        old_policy.0.to_str().unwrap(),
        "--clock-policy",
        "normalize-redundant-period",
        "--json",
    ]);
    assert_eq!(
        from_flag["policy"]["clock_policy"],
        "normalize_redundant_period"
    );
    assert_eq!(from_flag["totals"]["spoken_exact"], 2);
    for (path, policy) in [
        (old_policy.0.to_str().unwrap(), None),
        (tolerant_policy.0.to_str().unwrap(), Some("strict")),
    ] {
        let mut args = vec![
            "eval-speech",
            corpus.0.to_str().unwrap(),
            "--speech-policy",
            path,
            "--json",
        ];
        if let Some(policy) = policy {
            args.extend(["--clock-policy", policy]);
        }
        let output = run(&args);
        assert_eq!(output.status.code(), Some(1));
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["totals"]["cases"], 2);
        assert_eq!(report["totals"]["spoken_exact"], 0);
        assert_eq!(report["totals"]["fully_supported"], 0);
        assert_eq!(report["failures"][0]["spoken"], "13:10pm");
        assert_eq!(report["failures"][1]["spoken"], "0:05am");
        if policy.is_some() {
            assert_eq!(report["policy"]["clock_policy"], "strict");
        } else {
            assert!(report["policy"]["clock_policy"].is_null());
        }
    }
    let invalid = run(&[
        "eval-speech",
        corpus.0.to_str().unwrap(),
        "--clock-policy",
        "invented",
        "--json",
    ]);
    assert!(!invalid.status.success());
    assert!(invalid.stdout.is_empty());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("clock policy"));
}

#[test]
fn invalid_date_cli_reads_components_only_with_explicit_policy_and_retains_diagnostic() {
    use textintel::speech::temporal::TemporalValue;
    let preserved = successful_json(&[
        "normalize-speech",
        "30/02/1990",
        "--locale",
        "es",
        "--reading-style",
        "natural",
        "--json",
    ]);
    assert_eq!(preserved["text"], "30/02/1990");
    assert_eq!(preserved["fully_supported"], false);
    assert_eq!(
        successful_json(&[
            "normalize-speech",
            "30/02/1990",
            "--locale",
            "es",
            "--reading-style",
            "natural",
            "--invalid-date-policy",
            "preserve",
            "--json",
        ]),
        preserved
    );
    let source = "🙂 30/02/1990!";
    let parsed: SpeechDocument = serde_json::from_value(successful_json(&[
        "parse-speech",
        source,
        "--locale",
        "es",
        "--reading-style",
        "natural",
        "--invalid-date-policy",
        "read-components",
        "--json",
    ]))
    .unwrap();
    assert_eq!(parsed.text, source);
    let invalid = parsed
        .spans
        .iter()
        .find(|span| span.source == "30/02/1990")
        .unwrap();
    assert_eq!((invalid.start, invalid.end), (5, 15));
    assert_eq!(source.get(invalid.start..invalid.end), Some("30/02/1990"));
    assert!(matches!(
        &invalid.value,
        SpeechValue::Temporal { value: TemporalValue::InvalidCalendarDate { year: Some(year), month: 2, day: 30, .. } }
            if year == "1990"
    ));
    let spoken: SpeechNormalization = serde_json::from_value(successful_json(&[
        "normalize-speech",
        source,
        "--locale",
        "es",
        "--reading-style",
        "natural",
        "--invalid-date-policy",
        "read-components",
        "--json",
    ]))
    .unwrap();
    assert_eq!(
        spoken.text,
        "🙂 treinta de febrero de mil novecientos noventa!"
    );
    assert!(spoken.fully_supported);
    let reading = spoken
        .readings
        .iter()
        .find(|r| r.source == "30/02/1990")
        .unwrap();
    assert_eq!((reading.start, reading.end), (5, 15));
    assert!(reading.supported);
    assert_eq!(reading.reason.as_deref(), Some("invalid calendar date"));
    let native = "३१-०६";
    // A yearless hyphenated pair can also denote a range. Supply date
    // context rather than letting a reading policy silently change its kind.
    let parsed: SpeechDocument = serde_json::from_value(successful_json(&[
        "parse-speech",
        native,
        "--locale",
        "hi",
        "--reading-style",
        "natural",
        "--interpret-as",
        "date",
        "--invalid-date-policy",
        "read-components",
        "--json",
    ]))
    .unwrap();
    assert_eq!(parsed.spans.len(), 1);
    assert_eq!(parsed.spans[0].source, native);
    assert_eq!((parsed.spans[0].start, parsed.spans[0].end), (0, 13));
    assert!(matches!(
        &parsed.spans[0].value,
        SpeechValue::Temporal {
            value: TemporalValue::InvalidCalendarDate {
                year: None,
                month: 6,
                day: 31,
                ..
            }
        }
    ));
    let spoken: SpeechNormalization = serde_json::from_value(successful_json(&[
        "normalize-speech",
        native,
        "--locale",
        "hi",
        "--reading-style",
        "natural",
        "--interpret-as",
        "date",
        "--invalid-date-policy",
        "read-components",
        "--json",
    ]))
    .unwrap();
    assert_eq!(spoken.text, "इकतीस जून");
    assert!(spoken.fully_supported);
    assert!(spoken.readings[0].supported);
    assert_eq!(
        spoken.readings[0].reason.as_deref(),
        Some("invalid calendar date")
    );

    for source in ["29/02/2024", "03/04/2024"] {
        let parse = |policy| {
            successful_json(&[
                "parse-speech",
                source,
                "--locale",
                "es",
                "--date-order",
                "reject",
                "--reading-style",
                "natural",
                "--invalid-date-policy",
                policy,
                "--json",
            ])
        };
        let preserve = parse("preserve");
        let components = parse("read-components");
        assert_eq!(preserve, components, "{source}");
        let document: SpeechDocument = serde_json::from_value(components).unwrap();
        if source == "29/02/2024" {
            assert!(
                matches!(
                    &document.spans[0].value,
                    SpeechValue::Date {
                        year: 2024,
                        month: 2,
                        day: 29
                    } | SpeechValue::Temporal {
                        value: TemporalValue::Date {
                            month: Some(2),
                            day: Some(29),
                            ..
                        }
                    }
                ),
                "{document:?}"
            );
        } else {
            assert!(
                matches!(document.spans[0].value, SpeechValue::Ambiguous { .. }),
                "{document:?}"
            );
        }
    }
    for source in ["03/04/2024", "32/02/1990", "30/13/1990"] {
        let spoken = successful_json(&[
            "normalize-speech",
            source,
            "--locale",
            "es",
            "--date-order",
            "reject",
            "--reading-style",
            "natural",
            "--invalid-date-policy",
            "read-components",
            "--json",
        ]);
        assert_eq!(spoken["text"], source);
        assert_eq!(spoken["fully_supported"], false);
    }
    for command in ["parse-speech", "normalize-speech"] {
        let invalid = run(&[
            command,
            "30/02/1990",
            "--locale",
            "es",
            "--invalid-date-policy",
            "invented",
            "--json",
        ]);
        assert!(!invalid.status.success());
        assert!(invalid.stdout.is_empty());
        assert!(String::from_utf8_lossy(&invalid.stderr).contains("invalid date policy"));
    }
}

#[test]
fn evaluation_cli_invalid_date_policy_forwards_json_and_override_without_changing_gold() {
    let cases = [
        ("february", "es", "30/02/1990", "treinta de febrero de mil novecientos noventa"),
        ("june", "hi", "३१-०६", "इकतीस जून"),
    ].into_iter().map(|(id, locale, text, spoken)| json!({
        "id":id,"locale":locale,"text":text,"date_order":"reject_ambiguous","decimal_separator":null,
        "category":"date","spoken":[spoken],
        "expected_spans":[{"start":0,"end":text.len(),"source":text,"kind":"invalid_date"}]
    })).collect::<Vec<_>>();
    let corpus = TemporaryCorpus::new(&json!({
        "version":"literal-invalid-date-cli", "purpose":"Read source components while retaining invalid-date semantics", "cases":cases
    }));
    let old_policy =
        TemporaryCorpus::new(&json!({"reading_style":"natural","category_hints":true}));
    let literal_policy = TemporaryCorpus::new(&json!({
        "reading_style":"natural","category_hints":true,"invalid_date_policy":"read_components"
    }));
    let from_file = successful_json(&[
        "eval-speech",
        corpus.0.to_str().unwrap(),
        "--speech-policy",
        literal_policy.0.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(
        from_file["policy"]["invalid_date_policy"],
        "read_components"
    );
    assert_eq!(from_file["totals"]["spoken_exact"], 2);
    assert_eq!(from_file["totals"]["parse_exact"], 2);
    assert_eq!(from_file["totals"]["fully_supported"], 2);
    assert!(from_file["failures"].as_array().unwrap().is_empty());
    assert_eq!(from_file["diagnostics"].as_array().unwrap().len(), 2);
    assert_eq!(
        from_file["diagnostics_by_reason"]["invalid calendar date"],
        2
    );
    assert!(
        from_file["unresolved_by_reason"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    for (index, id, locale, source, spoken, end) in [
        (
            0,
            "february",
            "es",
            "30/02/1990",
            "treinta de febrero de mil novecientos noventa",
            10,
        ),
        (1, "june", "hi", "३१-०६", "इकतीस जून", 13),
    ] {
        let diagnostic = &from_file["diagnostics"][index];
        assert_eq!(diagnostic["case_id"], id);
        assert_eq!(diagnostic["locale"], locale);
        assert_eq!(diagnostic["kind"], "invalid_date");
        assert_eq!(diagnostic["start"], 0);
        assert_eq!(diagnostic["end"], end);
        assert_eq!(diagnostic["source"], source);
        assert_eq!(diagnostic["spoken"], spoken);
        assert_eq!(diagnostic["reading_supported"], true);
        assert_eq!(diagnostic["reason"], "invalid calendar date");
    }
    let plain = run(&[
        "eval-speech",
        corpus.0.to_str().unwrap(),
        "--speech-policy",
        literal_policy.0.to_str().unwrap(),
    ]);
    assert!(plain.status.success());
    let plain = String::from_utf8(plain.stdout).unwrap();
    assert!(plain.contains("spoken_exact=2/2"));
    assert!(plain.contains("diagnostics=2 supported_with_diagnostic=2"));
    let from_flag = successful_json(&[
        "eval-speech",
        corpus.0.to_str().unwrap(),
        "--speech-policy",
        old_policy.0.to_str().unwrap(),
        "--invalid-date-policy",
        "read-components",
        "--json",
    ]);
    assert_eq!(
        from_flag["policy"]["invalid_date_policy"],
        "read_components"
    );
    assert_eq!(from_flag["totals"]["spoken_exact"], 2);
    assert_eq!(from_flag["totals"]["parse_exact"], 2);
    assert_eq!(from_flag["diagnostics"], from_file["diagnostics"]);
    for (path, policy) in [
        (old_policy.0.to_str().unwrap(), None),
        (literal_policy.0.to_str().unwrap(), Some("preserve")),
    ] {
        let mut args = vec![
            "eval-speech",
            corpus.0.to_str().unwrap(),
            "--speech-policy",
            path,
            "--json",
        ];
        if let Some(policy) = policy {
            args.extend(["--invalid-date-policy", policy]);
        }
        let output = run(&args);
        assert_eq!(output.status.code(), Some(1));
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["totals"]["cases"], 2);
        assert_eq!(report["totals"]["spoken_exact"], 0);
        assert_eq!(report["totals"]["parse_exact"], 0);
        assert_eq!(report["totals"]["fully_supported"], 0);
        assert_eq!(report["failures"][0]["spoken"], "30/02/1990");
        assert_eq!(report["failures"][1]["spoken"], "३१-०६");
        assert_eq!(report["diagnostics"].as_array().unwrap().len(), 2);
        assert!(
            report["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .all(|diagnostic| {
                    diagnostic["reading_supported"] == false && diagnostic["kind"] == "ambiguous"
                })
        );
        assert_eq!(
            report["diagnostics_by_reason"],
            report["unresolved_by_reason"]
        );
        if policy.is_some() {
            assert_eq!(report["policy"]["invalid_date_policy"], "preserve");
        } else {
            assert!(report["policy"]["invalid_date_policy"].is_null());
        }
    }
    let plain_preserve = run(&[
        "eval-speech",
        corpus.0.to_str().unwrap(),
        "--speech-policy",
        old_policy.0.to_str().unwrap(),
    ]);
    assert_eq!(plain_preserve.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&plain_preserve.stdout)
            .contains("diagnostics=2 supported_with_diagnostic=0")
    );
    let invalid = run(&[
        "eval-speech",
        corpus.0.to_str().unwrap(),
        "--invalid-date-policy",
        "invented",
        "--json",
    ]);
    assert!(!invalid.status.success());
    assert!(invalid.stdout.is_empty());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("invalid date policy"));
}
