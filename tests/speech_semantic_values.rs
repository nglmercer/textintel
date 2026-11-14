use std::collections::BTreeSet;

use textintel::speech::evaluation::{SpeechCase, SpeechCorpus, evaluate_speech};
use textintel::speech::temporal::TemporalValue;
use textintel::speech::{DateOrder, SpeechLocale, SpeechValue};

const CORPUS: &str = "data/speech/semantic-values.json";
const PAIRS: [(&str, &str); 12] = [
    ("decimal-positive", "decimal-negative"),
    ("zero-positive", "zero-negative"),
    ("precision-three", "precision-four"),
    ("magnitude-small", "magnitude-large"),
    ("asset-usd", "asset-eur"),
    ("measure-positive", "measure-negative"),
    ("calendar-leap", "calendar-invalid"),
    ("clock-am", "clock-pm"),
    ("second-absent", "second-zero"),
    ("fraction-three", "fraction-four"),
    ("integer-zero", "integer-padded"),
    ("native-small", "native-large"),
];

fn case<'a>(corpus: &'a SpeechCorpus, locale: SpeechLocale, suffix: &str) -> &'a SpeechCase {
    let id = format!("{}-semantic-{suffix}", locale.code());
    corpus.cases.iter().find(|case| case.id == id).unwrap()
}

#[test]
fn designed_gold_has_twenty_locales_twelve_value_contrasts_and_exact_byte_offsets() {
    let corpus = SpeechCorpus::from_file(CORPUS).unwrap();
    assert_eq!(corpus.cases.len(), 560);
    assert_eq!(
        corpus
            .cases
            .iter()
            .map(|case| case.locale.code())
            .collect::<BTreeSet<_>>()
            .len(),
        SpeechLocale::ALL.len()
    );
    for locale in SpeechLocale::ALL {
        assert_eq!(
            corpus
                .cases
                .iter()
                .filter(|case| case.locale == locale)
                .count(),
            28
        );
        for (left, right) in PAIRS {
            let left = case(&corpus, locale, left);
            let right = case(&corpus, locale, right);
            let left = &left.expected_values.as_ref().unwrap()[0].value;
            let right = &right.expected_values.as_ref().unwrap()[0].value;
            assert_ne!(left, right, "collapsed designed contrast in {locale:?}");
        }
        let plain = case(&corpus, locale, "plain-prose");
        assert_eq!(plain.expected_values.as_deref(), Some([].as_slice()));
        for suffix in ["protected-code", "protected-json"] {
            let expected = case(&corpus, locale, suffix)
                .expected_values
                .as_ref()
                .unwrap();
            assert_eq!(expected.len(), 1);
            assert!(matches!(expected[0].value, SpeechValue::Identifier { .. }));
        }
        assert!(
            case(&corpus, locale, "fraction-four")
                .expected_spans
                .is_none()
        );
    }
    for case in &corpus.cases {
        assert!(case.spoken.is_empty());
        assert_eq!(case.decimal_separator, Some('.'));
        assert_eq!(case.date_order, DateOrder::RejectAmbiguous);
        let mut previous_end = 0;
        for span in case.expected_values.as_ref().unwrap() {
            assert!(span.start >= previous_end);
            assert_eq!(
                case.text.get(span.start..span.end),
                Some(span.source.as_str())
            );
            assert!(!matches!(span.value, SpeechValue::Text));
            previous_end = span.end;
        }
    }
}

#[test]
fn default_evaluation_checks_the_complete_semantic_matrix_without_spoken_gold() {
    let corpus = SpeechCorpus::from_file(CORPUS).unwrap();
    let report = evaluate_speech(&corpus).unwrap();
    let semantic = report.totals.semantic.as_ref().unwrap();
    assert_eq!(semantic.cases, 560);
    assert_eq!(semantic.exact, semantic.cases, "{:?}", report.failures);
    assert_eq!(semantic.expected_spans, 540);
    assert_eq!(semantic.predicted_spans, 540);
    assert_eq!(semantic.correct_spans, 540);
    assert_eq!(report.totals.parse_cases, 540);
    assert_eq!(report.totals.parse_exact, report.totals.parse_cases);
    assert_eq!(report.totals.spoken_cases, 0);
    assert_eq!(report.totals.spoken_exact, 0);
    assert_eq!(report.totals.spoken_variant, 0);
    assert!(report.failures.is_empty());
    assert!(report.passes(false));
    for locale in SpeechLocale::ALL {
        let semantic = report.by_locale[locale.code()].semantic.as_ref().unwrap();
        assert_eq!((semantic.cases, semantic.exact), (28, 28), "{locale:?}");
        assert_eq!(
            (
                semantic.expected_spans,
                semantic.predicted_spans,
                semantic.correct_spans
            ),
            (27, 27, 27)
        );
    }
}

#[test]
fn wrong_values_fail_even_when_the_same_source_span_and_kind_still_match() {
    let corpus = SpeechCorpus::from_file(CORPUS).unwrap();
    for suffix in [
        "decimal-positive",
        "magnitude-small",
        "precision-three",
        "asset-usd",
        "clock-pm",
        "second-absent",
        "fraction-three",
    ] {
        let mut wrong = corpus.clone();
        wrong.cases = vec![case(&corpus, SpeechLocale::En, suffix).clone()];
        let expected = wrong.cases[0].expected_values.as_mut().unwrap();
        match suffix {
            "decimal-positive" => {
                let SpeechValue::Number { value, .. } = &mut expected[0].value else {
                    panic!()
                };
                value.negative = true;
            }
            "magnitude-small" => {
                let SpeechValue::Number { value, .. } = &mut expected[0].value else {
                    panic!()
                };
                value.integer = "125".into();
            }
            "precision-three" => {
                let SpeechValue::Number { value, .. } = &mut expected[0].value else {
                    panic!()
                };
                value.fraction = Some("0000".into());
            }
            "asset-usd" => {
                let SpeechValue::Currency { code, .. } = &mut expected[0].value else {
                    panic!()
                };
                *code = "EUR".into();
            }
            "clock-pm" => {
                let SpeechValue::Time { hour, .. } = &mut expected[0].value else {
                    panic!()
                };
                *hour = 3;
            }
            "second-absent" => {
                let SpeechValue::Time { second, .. } = &mut expected[0].value else {
                    panic!()
                };
                *second = Some(0);
            }
            "fraction-three" => {
                let SpeechValue::Temporal {
                    value: TemporalValue::FractionalClock { fraction, .. },
                } = &mut expected[0].value
                else {
                    panic!()
                };
                *fraction = "0010".into();
            }
            _ => unreachable!(),
        }
        let report = evaluate_speech(&wrong).unwrap();
        assert_eq!(
            report.totals.parse_exact, 1,
            "{suffix}: {:?}",
            report.failures
        );
        let semantic = report.totals.semantic.as_ref().unwrap();
        assert_eq!(semantic.cases, 1);
        assert_eq!(semantic.exact, 0, "{suffix}");
        assert_eq!(semantic.correct_spans, 0, "{suffix}");
        assert!(!report.failures.is_empty(), "{suffix}");
        assert!(!report.passes(false), "{suffix}");
        assert!(!report.passes(true), "{suffix}");
    }
}

#[test]
fn semantic_only_empty_gold_cannot_erase_protected_identifiers() {
    let corpus = SpeechCorpus::from_file(CORPUS).unwrap();
    for suffix in ["protected-code", "protected-json"] {
        let mut wrong = corpus.clone();
        wrong.cases = vec![case(&corpus, SpeechLocale::En, suffix).clone()];
        wrong.cases[0].expected_spans = None;
        wrong.cases[0].expected_values = Some(vec![]);
        let report = evaluate_speech(&wrong).unwrap();
        assert_eq!(report.totals.parse_cases, 0);
        let semantic = report.totals.semantic.as_ref().unwrap();
        assert_eq!((semantic.cases, semantic.exact), (1, 0));
        assert_eq!(
            (
                semantic.expected_spans,
                semantic.predicted_spans,
                semantic.correct_spans
            ),
            (0, 1, 0)
        );
        assert!(!report.passes(false));
    }
}
