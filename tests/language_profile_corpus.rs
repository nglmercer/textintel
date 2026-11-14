use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use textintel::core::providers::LanguageDetectionProvider;
use textintel::{NgramLanguageDetector, ResourceLoader};

#[derive(Deserialize)]
struct Corpus {
    version: String,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    language: String,
    text: String,
    article: Option<u32>,
}
#[derive(Default, Serialize)]
struct Metrics {
    cases: usize,
    top1: usize,
    top3: usize,
    unknown_first: usize,
}

fn evaluate(corpus: &Corpus, resources: &ResourceLoader) -> serde_json::Value {
    let detector = NgramLanguageDetector::from_resources(resources);
    let mut totals = Metrics::default();
    let mut locales = BTreeMap::<String, Metrics>::new();
    let mut failures = vec![];
    for case in &corpus.cases {
        let result = detector.detect(&case.text).unwrap();
        assert!(
            result
                .iter()
                .all(|c| c.probability.is_finite() && (0.0..=1.0).contains(&c.probability))
        );
        let top1 = result.first().is_some_and(|c| c.language == case.language);
        let top3 = result.iter().take(3).any(|c| c.language == case.language);
        let unknown = result.first().is_some_and(|c| c.language == "unknown");
        for metrics in [
            &mut totals,
            locales.entry(case.language.clone()).or_default(),
        ] {
            metrics.cases += 1;
            metrics.top1 += usize::from(top1);
            metrics.top3 += usize::from(top3);
            metrics.unknown_first += usize::from(unknown);
        }
        if !top1 {
            failures.push(
                serde_json::json!({"id":case.id,"expected":case.language,"candidates":result}),
            );
        }
    }
    serde_json::json!({"corpus_version":corpus.version,"totals":totals,"by_language":locales,"failures":failures})
}

#[test]
fn pinned_udhr_profiles_and_heldout_queries_are_disjoint_and_report_actual_quality() {
    let corpus: Corpus =
        serde_json::from_str(include_str!("../data/language/udhr-heldout.json")).unwrap();
    let native: Corpus =
        serde_json::from_str(include_str!("../data/language/native-prose-heldout.json")).unwrap();
    assert_eq!(corpus.cases.len(), 147);
    assert_eq!(native.cases.len(), 7);
    let resources = ResourceLoader::embedded().unwrap();
    let mut ids = BTreeSet::new();
    let mut counts = BTreeMap::<&str, usize>::new();
    for case in &corpus.cases {
        assert!(ids.insert(&case.id));
        assert!((21..=30).contains(&case.article.unwrap()));
        *counts.entry(&case.language).or_default() += 1;
        let profile = resources.language_pack(&case.language).unwrap();
        assert_eq!(
            profile.revision.as_deref(),
            Some("udhr-588b3f4-articles1-20-v1")
        );
        assert!(
            !profile.examples.iter().any(|text| text == &case.text),
            "profile/holdout overlap: {}",
            case.id
        );
    }
    assert_eq!(counts.len(), 7);
    assert!(counts.values().all(|count| *count == 21));
    for case in &native.cases {
        let profile = resources.language_pack(&case.language).unwrap();
        let profile_text = profile.examples.join(" ").to_lowercase();
        assert!(
            !profile_text.contains(&case.text.to_lowercase()),
            "native query inserted into profile: {}",
            case.id
        );
    }
    let report = serde_json::json!({
        "detector":"NgramLanguageDetector with shared Unicode-word handling; default abstention threshold, no per-query tuning",
        "udhr_articles21_30":evaluate(&corpus,&resources),
        "native_prose":evaluate(&native,&resources),
        "limitations":"UDHR is one legal-domain source; handwritten prose is small and not native-speaker-certified. Counts are observed results, not production-accuracy guarantees."
    });
    eprintln!("LID_AUDIT={}", serde_json::to_string(&report).unwrap());
    if let Ok(path) = std::env::var("TEXTINTEL_LID_REPORT_PATH") {
        std::fs::write(path, serde_json::to_string_pretty(&report).unwrap() + "\n").unwrap();
    }
}
