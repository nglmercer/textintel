//! Category-level evaluation: per-slice metrics, legacy label inference,
//! and dataset coverage of all twelve production categories.

use textintel::evaluation::{EVALUATION_CATEGORIES, EvaluationCase, EvaluationDataset, evaluate};
use textintel::{EngineConfig, TextIntelligence};

fn case(id: &str, category: &str, a: &str, b: &str, similar: bool) -> EvaluationCase {
    EvaluationCase {
        id: id.to_string(),
        a: a.to_string(),
        b: b.to_string(),
        languages: Vec::new(),
        split: "test".to_string(),
        difficulty: "medium".to_string(),
        category: category.to_string(),
        labels: [(String::from("similar"), similar)].into_iter().collect(),
        expected: Default::default(),
        tags: Vec::new(),
    }
}

#[test]
fn report_contains_per_category_metrics() {
    let dataset = EvaluationDataset {
        version: "category-test".to_string(),
        cases: vec![
            case("a1", "leetspeak", "h3llo", "hello", true),
            case("a2", "leetspeak", "h3llo", "hollow", false),
            case(
                "b1",
                "spam",
                "WIN FREE PRIZE now",
                "WIN FREE PRIZE today",
                true,
            ),
            case("b2", "spam", "WIN FREE PRIZE now", "meeting notes", false),
        ],
    };
    let engine = TextIntelligence::new(EngineConfig::default());
    let report = evaluate(&engine, &dataset).unwrap();
    assert_eq!(report.metrics.count, 4);
    for name in ["leetspeak", "spam"] {
        let metrics = report
            .categories
            .get(name)
            .unwrap_or_else(|| panic!("missing category {name}: {:?}", report.categories.keys()));
        assert_eq!(metrics.count, 2);
        assert_eq!(metrics.positives, 1);
        assert_eq!(metrics.negatives, 1);
    }
    assert_eq!(report.categories.len(), 2);
}

#[test]
fn legacy_cases_without_category_are_inferred_from_labels() {
    let mut rebus = case("r1", "", "gr8", "great", true);
    rebus.labels.insert("rebus".to_string(), true);
    assert_eq!(rebus.primary_category(), "rebus");
    let mut homo = case("h1", "", "аpple", "apple", true);
    homo.labels.insert("homoglyph".to_string(), true);
    assert_eq!(homo.primary_category(), "homoglyph");
    let plain = case("g1", "", "hello", "hello", true);
    assert_eq!(plain.primary_category(), "general");
    // Explicit categories always win over labels.
    let mut explicit = case("x1", "hard_negatives", "a", "b", false);
    explicit.labels.insert("rebus".to_string(), true);
    assert_eq!(explicit.primary_category(), "hard_negatives");

    let dataset = EvaluationDataset {
        version: "inference-test".to_string(),
        cases: vec![rebus, plain],
    };
    let engine = TextIntelligence::new(EngineConfig::default());
    let report = evaluate(&engine, &dataset).unwrap();
    assert!(report.categories.contains_key("rebus"));
    assert!(report.categories.contains_key("general"));
}

#[test]
fn production_dataset_covers_all_twelve_categories() {
    let dataset = EvaluationDataset::from_dir("data/evaluation").unwrap();
    assert_eq!(
        EVALUATION_CATEGORIES.len(),
        12,
        "the contract is twelve categories"
    );
    for category in EVALUATION_CATEGORIES {
        let count = dataset
            .cases
            .iter()
            .filter(|case| case.primary_category() == *category)
            .count();
        assert!(
            count >= 8,
            "category {category} has only {count} cases, need >= 8"
        );
    }
}

#[test]
fn category_gates_file_pins_every_category() {
    let source = std::fs::read_to_string("data/quality-gates.json").unwrap();
    let gates: serde_json::Value = serde_json::from_str(&source).unwrap();
    let categories = gates
        .get("categories")
        .and_then(|value| value.as_object())
        .expect("gates must pin per-category bars");
    for category in EVALUATION_CATEGORIES {
        assert!(
            categories.contains_key(*category),
            "no gate for category {category}"
        );
    }
}
