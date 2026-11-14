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
