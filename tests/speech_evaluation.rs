use textintel::speech::evaluation::{SpeechCorpus, evaluate_speech};

#[test]
fn annotated_corpus_is_valid_and_covers_twenty_locales() {
    let corpus = SpeechCorpus::from_file("data/speech/evaluation.json").unwrap();
    let locales: std::collections::BTreeSet<_> =
        corpus.cases.iter().map(|c| c.locale.code()).collect();
    assert_eq!(locales.len(), 20);
    assert!(corpus.cases.iter().any(|c| c.category == "ambiguous"));
    assert!(corpus.cases.iter().any(|c| c.category == "identifier"));
}

#[test]
fn evaluation_never_drops_cases_or_confuses_code_coverage_with_accuracy() {
    let mut corpus = SpeechCorpus::from_file("data/speech/evaluation.json").unwrap();
    corpus
        .cases
        .retain(|c| c.category == "plain" || c.id == "en-number");
    let report = evaluate_speech(&corpus).unwrap();
    assert_eq!(report.totals.cases, corpus.cases.len());
    assert_eq!(report.totals.parse_exact, corpus.cases.len());
    assert_eq!(report.totals.spoken_cases, 5);
    assert_eq!(report.totals.spoken_exact, 5);
    assert_eq!(
        report.by_locale.values().map(|v| v.cases).sum::<usize>(),
        report.totals.cases
    );
    corpus.cases[0].spoken = vec!["deliberately incorrect independent gold".into()];
    let wrong = evaluate_speech(&corpus).unwrap();
    assert!(!wrong.failures.is_empty());
    assert!(wrong.totals.spoken_exact < wrong.totals.spoken_cases);
}

#[test]
fn rejects_broken_gold_offsets_and_duplicate_ids() {
    let corpus = SpeechCorpus::from_file("data/speech/evaluation.json").unwrap();
    let mut duplicate = corpus.clone();
    duplicate.cases.push(duplicate.cases[0].clone());
    assert!(duplicate.validate().is_err());
    let mut broken = corpus;
    broken.cases[0].expected_spans.as_mut().unwrap()[0].end = 999;
    assert!(broken.validate().is_err());
}

#[test]
fn evaluation_policy_rejects_typos_and_silent_locale_alias_overrides() {
    use textintel::speech::evaluation::SpeechEvaluationPolicy;
    assert!(
        serde_json::from_str::<SpeechEvaluationPolicy>(r#"{"number_notaton":"orthographic"}"#)
            .is_err()
    );
    for value in [
        r#"{"locales":{"en-US":{}}}"#,
        r#"{"locales":{"xx":{}}}"#,
        r#"{"locales":{"en":{"decimal_separator":"!"}}}"#,
        r#"{"locales":{"en":{"currency_symbols":{"$":"usd"}}}}"#,
        r#"{"locales":{"en":{"unit_aliases":{"f":"not-a-real-unit"}}}}"#,
    ] {
        let policy: SpeechEvaluationPolicy = serde_json::from_str(value).unwrap();
        assert!(policy.validate().is_err(), "{value}");
    }
}

#[test]
fn reviewed_variants_are_disjoint_and_do_not_hide_exact_or_parser_failures() {
    let mut corpus:SpeechCorpus=serde_json::from_value(serde_json::json!({
        "version":"variant-contract","purpose":"Separate literal and independently bounded variants",
        "cases":[{"id":"spacing","locale":"de","text":"21","date_order":"reject_ambiguous","decimal_separator":null,
        "category":"number","expected_spans":null,"spoken":["ein und zwanzig"]}]
    })).unwrap();
    let report = evaluate_speech(&corpus).unwrap();
    assert_eq!(report.totals.spoken_exact, 0);
    assert_eq!(report.totals.spoken_variant, 1);
    assert_eq!(report.totals.spoken_accepted, 1);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.variant_matches.len(), 1);
    assert!(!report.passes(false));
    assert!(report.passes(true));
    corpus.cases[0].expected_spans = Some(vec![]);
    assert!(!evaluate_speech(&corpus).unwrap().passes(true));
    corpus.cases[0].expected_spans = None;
    corpus.cases[0].spoken = vec!["zwei und zwanzig".into()];
    let wrong = evaluate_speech(&corpus).unwrap();
    assert_eq!(wrong.totals.spoken_accepted, 0);
    assert!(!wrong.passes(true));
}

#[test]
fn currency_naming_serde_preserves_older_options_and_policy_json_defaults() {
    use serde_json::json;
    use textintel::speech::evaluation::SpeechEvaluationPolicy;
    use textintel::speech::{CurrencyNaming, SpeechLocale, SpeechOptions};
    assert_eq!(CurrencyNaming::default(), CurrencyNaming::Style);
    assert_eq!(
        SpeechOptions::new(SpeechLocale::En).currency_naming,
        CurrencyNaming::Style
    );
    let old_options = json!({
        "locale":"en", "date_order":"reject_ambiguous", "decimal_separator":".",
        "max_input_chars":256, "max_spans":32
    });
    let mut options: SpeechOptions = serde_json::from_value(old_options).unwrap();
    assert_eq!(options.currency_naming, CurrencyNaming::Style);
    assert_eq!(
        serde_json::to_value(&options).unwrap()["currency_naming"],
        "style"
    );
    options.currency_naming = CurrencyNaming::Full;
    let serialized = serde_json::to_value(&options).unwrap();
    assert_eq!(serialized["currency_naming"], "full");
    assert_eq!(
        serde_json::from_value::<SpeechOptions>(serialized).unwrap(),
        options
    );

    let mut policy: SpeechEvaluationPolicy =
        serde_json::from_value(json!({"reading_style":"natural"})).unwrap();
    assert_eq!(policy.currency_naming, None);
    assert_eq!(SpeechEvaluationPolicy::default().currency_naming, None);
    policy.currency_naming = Some(CurrencyNaming::Full);
    let serialized = serde_json::to_value(&policy).unwrap();
    assert_eq!(serialized["currency_naming"], "full");
    let roundtrip: SpeechEvaluationPolicy = serde_json::from_value(serialized).unwrap();
    assert_eq!(roundtrip.currency_naming, Some(CurrencyNaming::Full));
    for invalid in [json!("invented"), json!("short"), json!(1)] {
        assert!(serde_json::from_value::<CurrencyNaming>(invalid.clone()).is_err());
        assert!(
            serde_json::from_value::<SpeechEvaluationPolicy>(json!({"currency_naming":invalid}))
                .is_err()
        );
    }
}

#[test]
fn evaluation_full_currency_policy_changes_reading_without_rewriting_gold_or_spans() {
    use serde_json::json;
    use textintel::speech::evaluation::{SpeechEvaluationPolicy, evaluate_speech_with_policy};
    use textintel::speech::{CurrencyNaming, ReadingStyle};
    let cases = [
        ("major", "en", "USD7", "seven US dollars"),
        ("minor", "en", "USD-0.05", "minus zero US dollars five cents"),
        ("dual", "ar", "USD2", "دولاران أمريكيان"),
    ]
    .into_iter()
    .map(|(id, locale, text, spoken)| json!({
        "id":id,"locale":locale,"text":text,"date_order":"reject_ambiguous","decimal_separator":".",
        "category":"currency", "spoken":[spoken],
        "expected_spans":[{"start":0,"end":text.len(),"source":text,"kind":"currency"}]
    }))
    .collect::<Vec<_>>();
    let corpus: SpeechCorpus = serde_json::from_value(json!({
        "version":"currency-policy-contract","purpose":"Qualified currency identity is an explicit rendering policy",
        "cases":cases
    })).unwrap();
    let original = serde_json::to_value(&corpus).unwrap();
    let mut policy: SpeechEvaluationPolicy =
        serde_json::from_value(json!({"reading_style":"natural"})).unwrap();
    assert_eq!(policy.reading_style, Some(ReadingStyle::Natural));
    let old = evaluate_speech_with_policy(&corpus, &policy).unwrap();
    assert_eq!(old.totals.cases, 3);
    assert_eq!(old.totals.parse_exact, 3);
    assert_eq!(old.totals.spoken_exact, 0);
    assert_eq!(old.totals.fully_supported, 3);
    assert_eq!(old.failures[0].spoken.as_deref(), Some("seven dollars"));
    assert_eq!(old.failures[1].spoken.as_deref(), Some("minus five cents"));
    policy.currency_naming = Some(CurrencyNaming::Full);
    let full = evaluate_speech_with_policy(&corpus, &policy).unwrap();
    assert_eq!(full.policy.currency_naming, Some(CurrencyNaming::Full));
    assert_eq!(full.totals.cases, 3);
    assert_eq!(full.totals.parse_exact, 3);
    assert_eq!(full.totals.spoken_exact, 3);
    assert_eq!(full.totals.spoken_variant, 0);
    assert_eq!(full.totals.fully_supported, 3);
    assert!(full.failures.is_empty());
    assert_eq!(serde_json::to_value(&corpus).unwrap(), original);
    policy.currency_naming = Some(CurrencyNaming::Style);
    let style = evaluate_speech_with_policy(&corpus, &policy).unwrap();
    assert_eq!(style.totals.spoken_exact, old.totals.spoken_exact);
    assert_eq!(style.totals.parse_exact, full.totals.parse_exact);
    assert_eq!(style.failures[1].spoken, old.failures[1].spoken);
}

#[test]
fn clock_policy_serde_preserves_strict_defaults_for_older_json() {
    use serde_json::json;
    use textintel::speech::evaluation::SpeechEvaluationPolicy;
    use textintel::speech::{ClockPolicy, SpeechLocale, SpeechOptions};
    assert_eq!(ClockPolicy::default(), ClockPolicy::Strict);
    assert_eq!(
        SpeechOptions::new(SpeechLocale::Zh).clock_policy,
        ClockPolicy::Strict
    );
    let mut options: SpeechOptions = serde_json::from_value(json!({
        "locale":"zh", "date_order":"reject_ambiguous", "decimal_separator":".",
        "max_input_chars":256, "max_spans":32
    }))
    .unwrap();
    assert_eq!(options.clock_policy, ClockPolicy::Strict);
    assert_eq!(
        serde_json::to_value(&options).unwrap()["clock_policy"],
        "strict"
    );
    options.clock_policy = ClockPolicy::NormalizeRedundantPeriod;
    let serialized = serde_json::to_value(&options).unwrap();
    assert_eq!(serialized["clock_policy"], "normalize_redundant_period");
    assert_eq!(
        serde_json::from_value::<SpeechOptions>(serialized).unwrap(),
        options
    );
    let old: SpeechEvaluationPolicy =
        serde_json::from_value(json!({"reading_style":"natural"})).unwrap();
    assert_eq!(old.clock_policy, None);
    assert_eq!(SpeechEvaluationPolicy::default().clock_policy, None);
    let explicit: SpeechEvaluationPolicy = serde_json::from_value(json!({
        "clock_policy":"normalize_redundant_period"
    }))
    .unwrap();
    assert_eq!(
        explicit.clock_policy,
        Some(ClockPolicy::NormalizeRedundantPeriod)
    );
    let serialized = serde_json::to_value(&explicit).unwrap();
    assert_eq!(serialized["clock_policy"], "normalize_redundant_period");
    assert_eq!(
        serde_json::from_value::<SpeechEvaluationPolicy>(serialized)
            .unwrap()
            .clock_policy,
        explicit.clock_policy
    );
    for invalid in [
        json!("invented"),
        json!("normalize-redundant-period"),
        json!(true),
    ] {
        assert!(serde_json::from_value::<ClockPolicy>(invalid.clone()).is_err());
        assert!(
            serde_json::from_value::<SpeechEvaluationPolicy>(json!({"clock_policy":invalid}))
                .is_err()
        );
    }
}

#[test]
fn evaluation_clock_policy_preserves_gold_sources_and_changes_only_explicit_interpretation() {
    use serde_json::json;
    use textintel::speech::ClockPolicy;
    use textintel::speech::evaluation::{SpeechEvaluationPolicy, evaluate_speech_with_policy};
    let cases = [
        ("afternoon", "zh", "13:10pm", "一点十分pm"),
        ("midnight", "en", "0:05am", "twelve oh five AM"),
    ].into_iter().map(|(id, locale, text, spoken)| json!({
        "id":id,"locale":locale,"text":text,"date_order":"reject_ambiguous","decimal_separator":null,
        "category":"time","spoken":[spoken],
        "expected_spans":[{"start":0,"end":text.len(),"source":text,"kind":"time"}]
    })).collect::<Vec<_>>();
    let corpus: SpeechCorpus = serde_json::from_value(json!({
        "version":"clock-policy-contract","purpose":"Source normalization must be an explicit interpretation policy",
        "cases":cases
    })).unwrap();
    let original = serde_json::to_value(&corpus).unwrap();
    let mut policy: SpeechEvaluationPolicy =
        serde_json::from_value(json!({"reading_style":"natural"})).unwrap();
    let strict = evaluate_speech_with_policy(&corpus, &policy).unwrap();
    assert_eq!(strict.totals.cases, 2);
    assert_eq!(strict.totals.spoken_exact, 0);
    assert_eq!(strict.totals.parse_exact, 0);
    assert_eq!(strict.totals.fully_supported, 0);
    assert_eq!(strict.failures[0].spoken.as_deref(), Some("13:10pm"));
    assert_eq!(strict.failures[0].predicted[0].kind, "ambiguous");
    assert_eq!(strict.failures[0].predicted[0].source, "13:10pm");
    assert_eq!(
        (
            strict.failures[0].predicted[0].start,
            strict.failures[0].predicted[0].end
        ),
        (0, 7)
    );
    policy.clock_policy = Some(ClockPolicy::NormalizeRedundantPeriod);
    let normalized = evaluate_speech_with_policy(&corpus, &policy).unwrap();
    assert_eq!(
        normalized.policy.clock_policy,
        Some(ClockPolicy::NormalizeRedundantPeriod)
    );
    assert_eq!(normalized.totals.cases, 2);
    assert_eq!(normalized.totals.parse_exact, 2);
    assert_eq!(normalized.totals.spoken_exact, 2);
    assert_eq!(normalized.totals.spoken_variant, 0);
    assert_eq!(normalized.totals.fully_supported, 2);
    assert!(normalized.failures.is_empty());
    assert_eq!(serde_json::to_value(&corpus).unwrap(), original);
    policy.clock_policy = Some(ClockPolicy::Strict);
    let explicit_strict = evaluate_speech_with_policy(&corpus, &policy).unwrap();
    assert_eq!(
        explicit_strict.totals.spoken_exact,
        strict.totals.spoken_exact
    );
    assert_eq!(
        explicit_strict.totals.parse_exact,
        strict.totals.parse_exact
    );
    assert_eq!(
        explicit_strict.failures[0].predicted,
        strict.failures[0].predicted
    );
}

#[test]
fn invalid_date_policy_serde_preserves_old_json_and_requires_explicit_component_reading() {
    use serde_json::json;
    use textintel::speech::evaluation::SpeechEvaluationPolicy;
    use textintel::speech::{InvalidDatePolicy, SpeechLocale, SpeechOptions};
    assert_eq!(InvalidDatePolicy::default(), InvalidDatePolicy::Preserve);
    assert_eq!(
        SpeechOptions::new(SpeechLocale::Es).invalid_date_policy,
        InvalidDatePolicy::Preserve
    );
    let mut options: SpeechOptions = serde_json::from_value(json!({
        "locale":"es", "date_order":"reject_ambiguous", "decimal_separator":null,
        "max_input_chars":256, "max_spans":32
    }))
    .unwrap();
    assert_eq!(options.invalid_date_policy, InvalidDatePolicy::Preserve);
    assert_eq!(
        serde_json::to_value(&options).unwrap()["invalid_date_policy"],
        "preserve"
    );
    options.invalid_date_policy = InvalidDatePolicy::ReadComponents;
    let serialized = serde_json::to_value(&options).unwrap();
    assert_eq!(serialized["invalid_date_policy"], "read_components");
    assert_eq!(
        serde_json::from_value::<SpeechOptions>(serialized).unwrap(),
        options
    );
    let old: SpeechEvaluationPolicy =
        serde_json::from_value(json!({"reading_style":"natural"})).unwrap();
    assert_eq!(old.invalid_date_policy, None);
    assert_eq!(SpeechEvaluationPolicy::default().invalid_date_policy, None);
    let explicit: SpeechEvaluationPolicy =
        serde_json::from_value(json!({"invalid_date_policy":"read_components"})).unwrap();
    assert_eq!(
        explicit.invalid_date_policy,
        Some(InvalidDatePolicy::ReadComponents)
    );
    let serialized = serde_json::to_value(&explicit).unwrap();
    assert_eq!(serialized["invalid_date_policy"], "read_components");
    assert_eq!(
        serde_json::from_value::<SpeechEvaluationPolicy>(serialized)
            .unwrap()
            .invalid_date_policy,
        explicit.invalid_date_policy
    );
    for invalid in [json!("invented"), json!("read-components"), json!(false)] {
        assert!(serde_json::from_value::<InvalidDatePolicy>(invalid.clone()).is_err());
        assert!(
            serde_json::from_value::<SpeechEvaluationPolicy>(
                json!({"invalid_date_policy":invalid})
            )
            .is_err()
        );
    }
}

#[test]
fn invalid_date_component_evaluation_never_classifies_the_literal_as_a_valid_date() {
    use serde_json::json;
    use textintel::speech::InvalidDatePolicy;
    use textintel::speech::evaluation::{
        SpeechEvalReport, SpeechEvaluationPolicy, evaluate_speech_with_policy,
    };
    let cases = [
        ("february", "es", "30/02/1990", "treinta de febrero de mil novecientos noventa"),
        ("june", "hi", "३१-०६", "इकतीस जून"),
    ].into_iter().map(|(id, locale, text, spoken)| json!({
        "id":id,"locale":locale,"text":text,"date_order":"reject_ambiguous","decimal_separator":null,
        "category":"date","spoken":[spoken],
        "expected_spans":[{"start":0,"end":text.len(),"source":text,"kind":"invalid_date"}]
    })).collect::<Vec<_>>();
    let corpus: SpeechCorpus = serde_json::from_value(json!({
        "version":"literal-invalid-date-contract", "purpose":"Literal component reading must retain invalid-date semantics", "cases":cases
    })).unwrap();
    corpus
        .validate()
        .expect("invalid_date is an explicit supported gold kind");
    let original = serde_json::to_value(&corpus).unwrap();
    let mut policy: SpeechEvaluationPolicy =
        serde_json::from_value(json!({"reading_style":"natural","category_hints":true})).unwrap();
    let preserved = evaluate_speech_with_policy(&corpus, &policy).unwrap();
    assert_eq!(preserved.totals.cases, 2);
    assert_eq!(preserved.totals.spoken_exact, 0);
    assert_eq!(preserved.totals.parse_exact, 0);
    assert_eq!(preserved.totals.fully_supported, 0);
    assert_eq!(preserved.failures[0].spoken.as_deref(), Some("30/02/1990"));
    assert_eq!(preserved.failures[1].spoken.as_deref(), Some("३१-०६"));
    assert_eq!(preserved.diagnostics.len(), 2);
    assert_eq!(
        preserved.diagnostics_by_reason,
        preserved.unresolved_by_reason
    );
    assert_eq!(preserved.unresolved_by_reason.values().sum::<usize>(), 2);
    for (diagnostic, case) in preserved.diagnostics.iter().zip(&corpus.cases) {
        assert_eq!(diagnostic.case_id, case.id);
        assert_eq!(diagnostic.locale, case.locale);
        assert_eq!(diagnostic.kind, "ambiguous");
        assert_eq!(diagnostic.source, case.text);
        assert_eq!(diagnostic.spoken, case.text);
        assert_eq!((diagnostic.start, diagnostic.end), (0, case.text.len()));
        assert!(!diagnostic.reading_supported);
        assert!(!diagnostic.reason.is_empty());
    }
    policy.invalid_date_policy = Some(InvalidDatePolicy::ReadComponents);
    let literal = evaluate_speech_with_policy(&corpus, &policy).unwrap();
    assert_eq!(
        literal.policy.invalid_date_policy,
        Some(InvalidDatePolicy::ReadComponents)
    );
    assert_eq!(literal.totals.parse_exact, 2);
    assert_eq!(literal.totals.spoken_exact, 2);
    assert_eq!(literal.totals.spoken_variant, 0);
    assert_eq!(literal.totals.fully_supported, 2);
    assert!(literal.failures.is_empty());
    assert!(literal.passes(false));
    assert_eq!(literal.diagnostics.len(), 2);
    assert_eq!(
        literal.diagnostics_by_reason.get("invalid calendar date"),
        Some(&2)
    );
    assert!(literal.unresolved_by_reason.is_empty());
    for (diagnostic, case) in literal.diagnostics.iter().zip(&corpus.cases) {
        assert_eq!(diagnostic.case_id, case.id);
        assert_eq!(diagnostic.locale, case.locale);
        assert_eq!(diagnostic.kind, "invalid_date");
        assert_eq!(diagnostic.source, case.text);
        assert_eq!(diagnostic.spoken, case.spoken[0]);
        assert_eq!((diagnostic.start, diagnostic.end), (0, case.text.len()));
        assert!(diagnostic.reading_supported);
        assert_eq!(diagnostic.reason, "invalid calendar date");
    }
    assert_eq!(serde_json::to_value(&corpus).unwrap(), original);

    // Reports written before diagnostic retention remain readable, without
    // inventing diagnostic information that the old payload did not contain.
    let mut old_report = serde_json::to_value(&literal).unwrap();
    let fields = old_report.as_object_mut().unwrap();
    fields.remove("diagnostics");
    fields.remove("diagnostics_by_reason");
    let restored: SpeechEvalReport = serde_json::from_value(old_report).unwrap();
    assert!(restored.diagnostics.is_empty());
    assert!(restored.diagnostics_by_reason.is_empty());
    assert_eq!(restored.totals.spoken_exact, 2);
    assert_eq!(restored.totals.parse_exact, 2);

    // Correct spoken components do not turn an impossible date into Date IR.
    let mut claimed_valid = corpus.clone();
    for case in &mut claimed_valid.cases {
        case.expected_spans.as_mut().unwrap()[0].kind = "date".into();
    }
    let mismatch = evaluate_speech_with_policy(&claimed_valid, &policy).unwrap();
    assert_eq!(mismatch.totals.spoken_exact, 2);
    assert_eq!(mismatch.totals.parse_exact, 0);
    assert!(!mismatch.passes(false));
    assert_eq!(
        serde_json::to_value(&mismatch.diagnostics).unwrap(),
        serde_json::to_value(&literal.diagnostics).unwrap()
    );
    assert_eq!(
        mismatch.diagnostics_by_reason,
        literal.diagnostics_by_reason
    );
    assert!(mismatch.unresolved_by_reason.is_empty());
    for (failure, original) in mismatch.failures.iter().zip(&corpus.cases) {
        assert_eq!(failure.predicted[0].kind, "invalid_date");
        assert_eq!(failure.predicted[0].source, original.text);
        assert_eq!(
            (failure.predicted[0].start, failure.predicted[0].end),
            (0, original.text.len())
        );
    }
    let mut wrong_spoken = corpus.clone();
    wrong_spoken.cases[0].spoken = vec!["intentionally incorrect spoken date gold".into()];
    let wrong_spoken = evaluate_speech_with_policy(&wrong_spoken, &policy).unwrap();
    assert_eq!(wrong_spoken.totals.spoken_exact, 1);
    assert_eq!(wrong_spoken.totals.parse_exact, 2);
    assert!(!wrong_spoken.passes(false));
    assert_eq!(
        serde_json::to_value(&wrong_spoken.diagnostics).unwrap(),
        serde_json::to_value(&literal.diagnostics).unwrap()
    );
    policy.invalid_date_policy = Some(InvalidDatePolicy::Preserve);
    let explicit_preserve = evaluate_speech_with_policy(&corpus, &policy).unwrap();
    assert_eq!(
        explicit_preserve.totals.spoken_exact,
        preserved.totals.spoken_exact
    );
    assert_eq!(
        explicit_preserve.totals.parse_exact,
        preserved.totals.parse_exact
    );
}

fn semantic_number(
    integer: &str,
    fraction: Option<&str>,
    negative: bool,
) -> textintel::speech::SpeechValue {
    textintel::speech::SpeechValue::Number {
        value: textintel::speech::NumberValue {
            negative,
            integer: integer.into(),
            fraction: fraction.map(str::to_owned),
        },
        reading: textintel::speech::NumberReading::Cardinal,
    }
}
fn semantic_span(
    source: &str,
    start: usize,
    value: textintel::speech::SpeechValue,
) -> textintel::speech::SpeechSpan {
    textintel::speech::SpeechSpan {
        start,
        end: start + source.len(),
        source: source.into(),
        value,
    }
}
fn semantic_case(
    id: &str,
    source: &str,
    expected: Option<Vec<textintel::speech::SpeechSpan>>,
) -> textintel::speech::evaluation::SpeechCase {
    textintel::speech::evaluation::SpeechCase {
        id: id.into(),
        locale: textintel::speech::SpeechLocale::En,
        text: source.into(),
        date_order: textintel::speech::DateOrder::RejectAmbiguous,
        decimal_separator: Some('.'),
        category: "number".into(),
        expected_spans: None,
        expected_values: expected,
        spoken: vec![],
    }
}
fn semantic_corpus(cases: Vec<textintel::speech::evaluation::SpeechCase>) -> SpeechCorpus {
    SpeechCorpus {
        version: "independent-semantic-contract".into(),
        purpose: "Compare independently constructed typed values, never parser-generated gold"
            .into(),
        cases,
    }
}

#[test]
fn semantic_values_detect_sign_magnitude_reading_and_fraction_zeros_with_identical_kind_offsets() {
    use textintel::speech::evaluation::ExpectedSpeechSpan;
    use textintel::speech::{NumberReading, SpeechValue};
    let mut wrong_reading = semantic_number("12", Some("00"), false);
    if let SpeechValue::Number { reading, .. } = &mut wrong_reading {
        *reading = NumberReading::Digits;
    }
    for expected in [
        semantic_number("12", Some("00"), true),
        semantic_number("13", Some("00"), false),
        semantic_number("12", Some("0"), false),
        wrong_reading,
    ] {
        let mut case = semantic_case(
            "wrong-value",
            "12.00",
            Some(vec![semantic_span("12.00", 0, expected)]),
        );
        case.expected_spans = Some(vec![ExpectedSpeechSpan {
            start: 0,
            end: 5,
            source: "12.00".into(),
            kind: "number".into(),
        }]);
        case.spoken = vec!["twelve point zero zero".into()];
        let corpus = semantic_corpus(vec![case]);
        corpus
            .validate()
            .expect("independent gold is not checked against parser output");
        let report = evaluate_speech(&corpus).unwrap();
        assert_eq!(report.totals.parse_exact, 1);
        assert_eq!(report.totals.spoken_exact, 1);
        let semantic = report.totals.semantic.as_ref().unwrap();
        assert_eq!(
            (
                semantic.cases,
                semantic.exact,
                semantic.expected_spans,
                semantic.predicted_spans,
                semantic.correct_spans
            ),
            (1, 0, 1, 1, 0)
        );
        assert_eq!(
            (semantic.accuracy(), semantic.precision(), semantic.recall()),
            (0.0, 0.0, 0.0)
        );
        assert!(!report.passes(false));
        assert!(!report.passes(true));
        assert_eq!(report.failures.len(), 1);
        assert_ne!(
            report.failures[0].expected_values,
            report.failures[0].predicted_values
        );
    }
    let mut correct = semantic_case(
        "correct",
        "12.00",
        Some(vec![semantic_span(
            "12.00",
            0,
            semantic_number("12", Some("00"), false),
        )]),
    );
    correct.spoken = vec!["twelve point zero zero".into()];
    let report = evaluate_speech(&semantic_corpus(vec![correct])).unwrap();
    assert!(report.passes(false) && report.passes(true));
    let semantic = report.totals.semantic.unwrap();
    assert_eq!((semantic.exact, semantic.correct_spans), (1, 1));
    assert_eq!(
        (semantic.accuracy(), semantic.precision(), semantic.recall()),
        (1.0, 1.0, 1.0)
    );
}

#[test]
fn semantic_currency_and_meridiem_mismatches_are_not_hidden_by_span_kinds() {
    use textintel::speech::evaluation::ExpectedSpeechSpan;
    use textintel::speech::{NumberValue, SpeechValue};
    for (source, kind, wrong_value) in [
        (
            "USD2",
            "currency",
            SpeechValue::Currency {
                code: "EUR".into(),
                value: NumberValue {
                    negative: false,
                    integer: "2".into(),
                    fraction: None,
                },
            },
        ),
        (
            "3:15 PM",
            "time",
            SpeechValue::Time {
                hour: 3,
                minute: 15,
                second: None,
            },
        ),
    ] {
        let mut case = semantic_case(
            "semantic-false-positive",
            source,
            Some(vec![semantic_span(source, 0, wrong_value)]),
        );
        case.expected_spans = Some(vec![ExpectedSpeechSpan {
            start: 0,
            end: source.len(),
            source: source.into(),
            kind: kind.into(),
        }]);
        let report = evaluate_speech(&semantic_corpus(vec![case])).unwrap();
        assert_eq!(report.totals.parse_exact, 1);
        assert_eq!(report.totals.semantic.as_ref().unwrap().exact, 0);
        assert!(!report.passes(false) && !report.passes(true));
        assert_eq!(
            report.failures[0].expected.as_ref().unwrap(),
            &report.failures[0].predicted
        );
    }
}

#[test]
fn semantic_failure_cannot_pass_even_when_a_reviewed_spoken_variant_matches() {
    for (gold_integer, expected_gate) in [("42", true), ("43", false)] {
        let mut case = semantic_case(
            "variant-with-values",
            "42",
            Some(vec![semantic_span(
                "42",
                0,
                semantic_number(gold_integer, None, false),
            )]),
        );
        case.spoken = vec!["forty two".into()];
        let report = evaluate_speech(&semantic_corpus(vec![case])).unwrap();
        assert_eq!(report.totals.spoken_exact, 0);
        assert_eq!(report.totals.spoken_variant, 1);
        assert_eq!(report.totals.spoken_accepted, 1);
        assert!(!report.passes(false));
        assert_eq!(report.passes(true), expected_gate);
        assert_eq!(
            report.totals.semantic.as_ref().unwrap().exact,
            usize::from(expected_gate)
        );
    }
}

#[test]
fn semantic_none_empty_and_parser_errors_have_distinct_denominators() {
    let corpus = semantic_corpus(vec![
        semantic_case("assert-plain", "hello", Some(vec![])),
        semantic_case("unmeasured-number", "42", None),
    ]);
    let report = evaluate_speech(&corpus).unwrap();
    assert_eq!(report.totals.cases, 2);
    assert_eq!(report.totals.parse_cases, 0);
    let semantic = report.totals.semantic.as_ref().unwrap();
    assert_eq!(
        (
            semantic.cases,
            semantic.exact,
            semantic.expected_spans,
            semantic.predicted_spans
        ),
        (1, 1, 0, 0)
    );
    assert_eq!(
        (semantic.accuracy(), semantic.precision(), semantic.recall()),
        (1.0, 0.0, 0.0)
    );
    assert_eq!(report.by_locale["en"].semantic.as_ref().unwrap().cases, 1);
    assert!(report.passes(false));
    let unexpected = evaluate_speech(&semantic_corpus(vec![semantic_case(
        "assert-no-number",
        "42",
        Some(vec![]),
    )]))
    .unwrap();
    assert!(!unexpected.passes(false) && !unexpected.passes(true));
    assert_eq!(
        unexpected.totals.semantic.as_ref().unwrap().predicted_spans,
        1
    );
    assert!(
        unexpected.failures[0]
            .expected_values
            .as_ref()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        unexpected.failures[0]
            .predicted_values
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    let error_source = "a".repeat(8193);
    let error = evaluate_speech(&semantic_corpus(vec![semantic_case(
        "parser-error",
        &error_source,
        Some(vec![]),
    )]))
    .unwrap();
    assert_eq!(
        (
            error.totals.semantic.as_ref().unwrap().cases,
            error.totals.semantic.as_ref().unwrap().exact
        ),
        (1, 0)
    );
    assert!(error.failures[0].error.is_some());
    assert_eq!(error.failures[0].predicted_values, Some(vec![]));
    assert!(!error.passes(false) && !error.passes(true));
}

#[test]
fn semantic_span_precision_and_recall_count_only_complete_equal_values() {
    let corpus = semantic_corpus(vec![semantic_case(
        "one-correct-of-two",
        "12 34",
        Some(vec![
            semantic_span("12", 0, semantic_number("12", None, false)),
            semantic_span("34", 3, semantic_number("35", None, false)),
        ]),
    )]);
    let report = evaluate_speech(&corpus).unwrap();
    let semantic = report.totals.semantic.as_ref().unwrap();
    assert_eq!(
        (
            semantic.expected_spans,
            semantic.predicted_spans,
            semantic.correct_spans
        ),
        (2, 2, 1)
    );
    assert_eq!(
        (semantic.accuracy(), semantic.precision(), semantic.recall()),
        (0.0, 0.5, 0.5)
    );
    assert!(!report.passes(true));
    let partial = evaluate_speech(&semantic_corpus(vec![semantic_case(
        "only-one-gold",
        "12 34",
        Some(vec![semantic_span(
            "12",
            0,
            semantic_number("12", None, false),
        )]),
    )]))
    .unwrap();
    let semantic = partial.totals.semantic.as_ref().unwrap();
    assert_eq!(
        (
            semantic.expected_spans,
            semantic.predicted_spans,
            semantic.correct_spans
        ),
        (1, 2, 1)
    );
    assert_eq!((semantic.precision(), semantic.recall()), (0.5, 1.0));
    assert!(!partial.passes(true));
}

#[test]
fn semantic_gold_validation_rejects_bad_utf8_order_source_text_and_incoherent_kind_gold() {
    use textintel::speech::SpeechValue;
    use textintel::speech::evaluation::ExpectedSpeechSpan;
    let good = semantic_span("12", 5, semantic_number("12", None, false));
    let corpus = semantic_corpus(vec![semantic_case(
        "unicode-source",
        "📚 12",
        Some(vec![good.clone()]),
    )]);
    assert!(corpus.validate().is_ok());
    assert!(evaluate_speech(&corpus).unwrap().passes(false));
    for bad in [
        textintel::speech::SpeechSpan {
            start: 1,
            end: 4,
            source: "📚".into(),
            value: semantic_number("12", None, false),
        },
        textintel::speech::SpeechSpan {
            start: 7,
            end: 5,
            source: "12".into(),
            value: semantic_number("12", None, false),
        },
        textintel::speech::SpeechSpan {
            start: 5,
            end: 99,
            source: "12".into(),
            value: semantic_number("12", None, false),
        },
        textintel::speech::SpeechSpan {
            start: 5,
            end: 5,
            source: "".into(),
            value: semantic_number("12", None, false),
        },
        textintel::speech::SpeechSpan {
            source: "13".into(),
            ..good.clone()
        },
        textintel::speech::SpeechSpan {
            value: SpeechValue::Text,
            ..good.clone()
        },
    ] {
        let mut invalid = corpus.clone();
        invalid.cases[0].expected_values = Some(vec![bad]);
        assert!(invalid.validate().is_err());
    }
    let mut overlap = corpus.clone();
    overlap.cases[0].expected_values = Some(vec![good.clone(), good.clone()]);
    assert!(overlap.validate().is_err());
    let mut order = semantic_corpus(vec![semantic_case(
        "bad-order",
        "12 34",
        Some(vec![
            semantic_span("34", 3, semantic_number("34", None, false)),
            semantic_span("12", 0, semantic_number("12", None, false)),
        ]),
    )]);
    assert!(order.validate().is_err());
    order.cases[0].expected_values.as_mut().unwrap().reverse();
    assert!(order.validate().is_ok());
    let mut coherent = corpus.clone();
    coherent.cases[0].expected_spans = Some(vec![ExpectedSpeechSpan {
        start: 5,
        end: 7,
        source: "12".into(),
        kind: "number".into(),
    }]);
    assert!(coherent.validate().is_ok());
    coherent.cases[0].expected_spans.as_mut().unwrap()[0].kind = "currency".into();
    assert!(coherent.validate().is_err());
    coherent.cases[0].expected_spans = Some(vec![]);
    assert!(coherent.validate().is_err());
    coherent.cases[0].expected_values = Some(vec![]);
    assert!(coherent.validate().is_ok());
}

#[test]
fn semantic_additions_are_optional_in_old_json_and_failure_metadata_cannot_fail_open() {
    use serde_json::json;
    use textintel::speech::evaluation::SpeechEvalReport;
    let old_case = json!({"id":"old","locale":"en","text":"42","date_order":"reject_ambiguous","decimal_separator":null,"category":"number","expected_spans":null,"spoken":["wrong"]});
    let case: textintel::speech::evaluation::SpeechCase =
        serde_json::from_value(old_case.clone()).unwrap();
    assert!(case.expected_values.is_none());
    assert_eq!(serde_json::to_value(&case).unwrap(), old_case);
    let report = evaluate_speech(&semantic_corpus(vec![case])).unwrap();
    let value = serde_json::to_value(&report).unwrap();
    assert!(value["totals"].get("semantic").is_none());
    assert!(value["by_locale"]["en"].get("semantic").is_none());
    assert!(value["by_category"]["number"].get("semantic").is_none());
    assert!(value["failures"][0].get("expected_values").is_none());
    assert!(value["failures"][0].get("predicted_values").is_none());
    let restored: SpeechEvalReport = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), value);
    let mut annotated = semantic_case("missing-prediction", "42", Some(vec![]));
    annotated.spoken = vec!["forty-two".into()];
    let mut report = evaluate_speech(&semantic_corpus(vec![annotated])).unwrap();
    assert_eq!(report.totals.spoken_exact, 1);
    assert!(report.failures[0].expected_values.is_some());
    report.failures[0].predicted_values = None;
    assert!(!report.passes(false) && !report.passes(true));
    let mut incomplete = serde_json::to_value(&report).unwrap();
    incomplete["failures"][0]
        .as_object_mut()
        .unwrap()
        .remove("expected_values");
    let mut restored: SpeechEvalReport = serde_json::from_value(incomplete).unwrap();
    assert!(!restored.passes(false) && !restored.passes(true));
    restored.failures.clear();
    assert!(!restored.passes(false) && !restored.passes(true));
    let semantic = restored.totals.semantic.as_mut().unwrap();
    semantic.exact = semantic.cases + 1;
    assert!(!restored.passes(false) && !restored.passes(true));
}

#[test]
fn semantic_comparison_never_projects_distinct_ir_representations_into_equality() {
    use textintel::speech::evaluation::ExpectedSpeechSpan;
    use textintel::speech::{
        SpeechValue,
        temporal::{DatePresentation, TemporalValue},
    };
    for (source, kind, independent_alternate_ir) in [
        (
            "2024-02-29",
            "date",
            SpeechValue::Temporal {
                value: TemporalValue::Date {
                    year: Some("2024".into()),
                    month: Some(2),
                    day: Some(29),
                    presentation: DatePresentation::Numeric,
                    connector: None,
                    year_label: false,
                },
            },
        ),
        (
            "3:15 PM",
            "time",
            SpeechValue::Temporal {
                value: TemporalValue::Clock {
                    hour: 3,
                    minute: Some(15),
                    second: None,
                    period: None,
                    meridiem: Some("pm".into()),
                    half: false,
                },
            },
        ),
    ] {
        let mut case = semantic_case(
            "different-ir",
            source,
            Some(vec![semantic_span(source, 0, independent_alternate_ir)]),
        );
        case.expected_spans = Some(vec![ExpectedSpeechSpan {
            start: 0,
            end: source.len(),
            source: source.into(),
            kind: kind.into(),
        }]);
        let report = evaluate_speech(&semantic_corpus(vec![case])).unwrap();
        assert_eq!(report.totals.parse_exact, 1);
        assert_eq!(report.totals.semantic.as_ref().unwrap().exact, 0);
        assert!(!report.passes(false) && !report.passes(true));
    }
    let mut contradictory = semantic_case(
        "two-annotations",
        "12 34",
        Some(vec![semantic_span(
            "12",
            0,
            semantic_number("12", None, false),
        )]),
    );
    contradictory.expected_spans = Some(vec![ExpectedSpeechSpan {
        start: 3,
        end: 5,
        source: "34".into(),
        kind: "number".into(),
    }]);
    assert!(semantic_corpus(vec![contradictory]).validate().is_err());
}
