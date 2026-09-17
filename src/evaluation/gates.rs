//! Quality gates: threshold files (`{ "<section>": { "<metric>_min": f64,
//! "<metric>_max": f64 } }`) checked against an [`EvaluationReport`].
//! Pure and total: unknown sections and metrics are ignored so gates stay
//! forwards compatible, and spam gates fail closed when spam was not
//! measured.

use super::report::EvaluationReport;

/// Quality-gate file shape: `{ "<section>": { "<metric>_min": f64,
/// "<metric>_max": f64 } }`. Unknown sections and metrics are ignored so
/// gates stay forwards compatible.
pub fn check_gates(report: &EvaluationReport, gates: &serde_json::Value) -> Vec<String> {
    let mut observed: Vec<(&str, &str, f64)> = vec![
        ("similarity", "roc_auc", report.metrics.roc_auc),
        ("similarity", "pr_auc", report.metrics.pr_auc),
        ("similarity", "f1", report.metrics.f1),
        ("similarity", "accuracy", report.metrics.accuracy),
        ("similarity", "brier", report.metrics.brier),
        (
            "similarity",
            "ece",
            report.metrics.expected_calibration_error,
        ),
        ("rebus", "top1", report.rebus.top1_accuracy),
        ("rebus", "top3", report.rebus.top3_accuracy),
        ("rebus", "top5", report.rebus.top_k_accuracy),
        ("language", "top1", report.language.top1_accuracy),
        ("language", "top3", report.language.top3_accuracy),
        ("search", "recall_at_1", report.ranking.recall_at_1),
        ("search", "recall_at_5", report.ranking.recall_at_5),
        ("search", "recall_at_10", report.ranking.recall_at_10),
        ("search", "mrr", report.ranking.mrr),
    ];
    if let Some(spam) = &report.spam {
        observed.extend([
            ("spam", "roc_auc", spam.metrics.roc_auc),
            ("spam", "pr_auc", spam.metrics.pr_auc),
            ("spam", "f1", spam.metrics.f1),
            ("spam", "accuracy", spam.metrics.accuracy),
            ("spam", "brier", spam.metrics.brier),
            ("spam", "ece", spam.metrics.expected_calibration_error),
        ]);
    }
    let mut failures = Vec::new();
    let Some(sections) = gates.as_object() else {
        return failures;
    };
    // Spam gates fail closed when the corpus was not measured (missing
    // file): `spam` is the only conditionally-measured section, so it is
    // the only one checked here; genuinely unknown sections stay ignored.
    if report.spam.is_none() && sections.contains_key("spam") {
        failures.push("spam: no metrics measured (missing spam corpus?)".to_string());
    }
    for (section, metric, value) in observed {
        let section_gates = sections.get(section);
        let key = format!("{metric}_min");
        if let Some(minimum) = section_gates
            .and_then(|value| value.get(&key))
            .and_then(serde_json::Value::as_f64)
            && value < minimum
        {
            failures.push(format!(
                "{section}.{metric}={value:.3} below minimum {minimum:.3}"
            ));
        }
        let key = format!("{metric}_max");
        if let Some(maximum) = section_gates
            .and_then(|value| value.get(&key))
            .and_then(serde_json::Value::as_f64)
            && value > maximum
        {
            failures.push(format!(
                "{section}.{metric}={value:.3} above maximum {maximum:.3}"
            ));
        }
    }
    // Per-category gates: `{ "categories": { "<name>": { "<metric>_min": f64,
    // "count_min": n } } }`. Missing categories fail only when `count_min`
    // is positive, so gates can require coverage without pinning metrics.
    if let Some(categories) = sections
        .get("categories")
        .and_then(|value| value.as_object())
    {
        for (name, thresholds) in categories {
            let Some(thresholds) = thresholds.as_object() else {
                continue;
            };
            let observed = report.categories.get(name);
            if let Some(count_min) = thresholds.get("count_min").and_then(|v| v.as_u64()) {
                let count = observed.map(|metrics| metrics.count as u64).unwrap_or(0);
                if count < count_min {
                    failures.push(format!(
                        "categories.{name}.count={count} below minimum {count_min}"
                    ));
                    continue;
                }
            }
            let Some(metrics) = observed else { continue };
            for (metric, value) in [
                ("accuracy", metrics.accuracy),
                ("f1", metrics.f1),
                ("roc_auc", metrics.roc_auc),
                ("pr_auc", metrics.pr_auc),
                ("brier", metrics.brier),
                ("ece", metrics.expected_calibration_error),
            ] {
                let key = format!("{metric}_min");
                if let Some(minimum) = thresholds.get(&key).and_then(|v| v.as_f64())
                    && value < minimum
                {
                    failures.push(format!(
                        "categories.{name}.{metric}={value:.3} below minimum {minimum:.3}"
                    ));
                }
                let key = format!("{metric}_max");
                if let Some(maximum) = thresholds.get(&key).and_then(|v| v.as_f64())
                    && value > maximum
                {
                    failures.push(format!(
                        "categories.{name}.{metric}={value:.3} above maximum {maximum:.3}"
                    ));
                }
            }
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::super::report::{
        BinaryMetrics, LanguageMetrics, RankingMetrics, RebusMetrics, SpamReport,
    };
    use super::*;

    fn report_with(similarity: BinaryMetrics, spam: Option<SpamReport>) -> EvaluationReport {
        EvaluationReport {
            dataset_version: "gate-test".to_string(),
            split: "test".to_string(),
            metrics: similarity,
            rebus: RebusMetrics::default(),
            language: LanguageMetrics::default(),
            ranking: RankingMetrics::default(),
            average_compare_micros: 0.0,
            compare_latency: Default::default(),
            analyze_latency: Default::default(),
            expectations_total: 0,
            expectations_met: 0,
            categories: Default::default(),
            spam,
        }
    }

    #[test]
    fn gates_pass_within_thresholds_and_ignore_unknown_sections() {
        let report = report_with(
            BinaryMetrics {
                f1: 0.95,
                expected_calibration_error: 0.05,
                ..Default::default()
            },
            None,
        );
        let gates = serde_json::json!({
            "similarity": {"f1_min": 0.9, "ece_max": 0.1},
            "unknown_section": {"whatever_min": 99.0},
        });
        assert!(check_gates(&report, &gates).is_empty());
    }

    #[test]
    fn gates_report_min_and_max_violations() {
        let report = report_with(
            BinaryMetrics {
                f1: 0.8,
                expected_calibration_error: 0.2,
                ..Default::default()
            },
            None,
        );
        let gates = serde_json::json!({
            "similarity": {"f1_min": 0.9, "ece_max": 0.1},
        });
        let failures = check_gates(&report, &gates);
        assert_eq!(failures.len(), 2);
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("similarity.f1"))
        );
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("similarity.ece"))
        );
    }

    #[test]
    fn spam_gates_fail_closed_without_metrics() {
        let report = report_with(BinaryMetrics::default(), None);
        let gates = serde_json::json!({"spam": {"f1_min": 0.8}});
        let failures = check_gates(&report, &gates);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("spam"));
    }
}
