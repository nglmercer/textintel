//! Versioned evaluation primitives kept independent from model/training code.

use std::collections::BTreeMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::core::error::TextIntelError;
use crate::engine::TextIntelligence;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationCase {
    pub a: String,
    pub b: String,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub labels: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationDataset {
    #[serde(default = "default_dataset_version")]
    pub version: String,
    pub cases: Vec<EvaluationCase>,
}

fn default_dataset_version() -> String {
    "unversioned".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BinaryMetrics {
    pub count: usize,
    pub positives: usize,
    pub negatives: usize,
    pub roc_auc: f64,
    pub pr_auc: f64,
    pub f1: f64,
    pub brier: f64,
    pub expected_calibration_error: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RebusMetrics {
    pub labelled_cases: usize,
    pub top1_accuracy: f64,
    pub top_k_accuracy: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LanguageMetrics {
    pub labelled_cases: usize,
    pub top1_accuracy: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationReport {
    pub dataset_version: String,
    pub metrics: BinaryMetrics,
    pub rebus: RebusMetrics,
    pub language: LanguageMetrics,
    pub average_compare_micros: f64,
}

impl EvaluationDataset {
    pub fn from_json(source: &str) -> Result<Self, TextIntelError> {
        if let Ok(dataset) = serde_json::from_str::<Self>(source) {
            return Ok(dataset);
        }
        let cases = serde_json::from_str(source)?;
        Ok(Self {
            version: "unversioned".to_string(),
            cases,
        })
    }
}

pub fn evaluate(
    engine: &TextIntelligence,
    dataset: &EvaluationDataset,
) -> Result<EvaluationReport, TextIntelError> {
    if dataset.cases.is_empty() {
        return Ok(EvaluationReport {
            dataset_version: dataset.version.clone(),
            metrics: BinaryMetrics {
                count: 0,
                positives: 0,
                negatives: 0,
                roc_auc: 0.0,
                pr_auc: 0.0,
                f1: 0.0,
                brier: 0.0,
                expected_calibration_error: 0.0,
            },
            rebus: RebusMetrics {
                labelled_cases: 0,
                top1_accuracy: 0.0,
                top_k_accuracy: 0.0,
            },
            language: LanguageMetrics {
                labelled_cases: 0,
                top1_accuracy: 0.0,
            },
            average_compare_micros: 0.0,
        });
    }
    let pairs = dataset
        .cases
        .iter()
        .map(|case| (case.a.clone(), case.b.clone()))
        .collect::<Vec<_>>();
    let started = Instant::now();
    let comparisons = engine.compare_batch(&pairs)?;
    let elapsed = started.elapsed().as_secs_f64() * 1_000_000.0;
    let samples = dataset
        .cases
        .iter()
        .zip(comparisons.iter())
        .map(|(case, result)| {
            (
                result.score,
                case.labels.get("similar").copied().unwrap_or(false),
            )
        })
        .collect::<Vec<_>>();
    let metrics = binary_metrics(&samples);

    let mut rebus_labelled = 0usize;
    let mut rebus_top1 = 0usize;
    let mut rebus_topk = 0usize;
    let mut language_labelled = 0usize;
    let mut language_top1 = 0usize;
    for case in &dataset.cases {
        if case.labels.get("rebus").copied().unwrap_or(false) {
            rebus_labelled += 1;
            let candidates = engine.decode(&case.a)?;
            if candidates
                .first()
                .is_some_and(|candidate| same_text(&candidate.text, &case.b))
            {
                rebus_top1 += 1;
            }
            if candidates
                .iter()
                .take(5)
                .any(|candidate| same_text(&candidate.text, &case.b))
            {
                rebus_topk += 1;
            }
        }
        if !case.languages.is_empty() {
            language_labelled += 1;
            let detected = engine.analyze(&case.a)?;
            if detected.top_language().is_some_and(|language| {
                case.languages
                    .iter()
                    .any(|expected| expected.eq_ignore_ascii_case(language))
            }) {
                language_top1 += 1;
            }
        }
    }
    Ok(EvaluationReport {
        dataset_version: dataset.version.clone(),
        metrics,
        rebus: RebusMetrics {
            labelled_cases: rebus_labelled,
            top1_accuracy: ratio(rebus_top1, rebus_labelled),
            top_k_accuracy: ratio(rebus_topk, rebus_labelled),
        },
        language: LanguageMetrics {
            labelled_cases: language_labelled,
            top1_accuracy: ratio(language_top1, language_labelled),
        },
        average_compare_micros: elapsed / dataset.cases.len() as f64,
    })
}

fn same_text(left: &str, right: &str) -> bool {
    crate::normalization::unicode::casefold_text(left).trim()
        == crate::normalization::unicode::casefold_text(right).trim()
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn binary_metrics(samples: &[(f64, bool)]) -> BinaryMetrics {
    let positives = samples.iter().filter(|(_, label)| *label).count();
    let negatives = samples.len().saturating_sub(positives);
    let mut ranked = samples.to_vec();
    ranked.sort_by(|left, right| right.0.total_cmp(&left.0));
    let roc_auc = if positives == 0 || negatives == 0 {
        0.0
    } else {
        let concordant = samples
            .iter()
            .filter(|(_, label)| *label)
            .map(|(positive, _)| {
                samples
                    .iter()
                    .filter(|(_, label)| !*label)
                    .map(|(negative, _)| {
                        if positive > negative {
                            1.0
                        } else if positive == negative {
                            0.5
                        } else {
                            0.0
                        }
                    })
                    .sum::<f64>()
            })
            .sum::<f64>();
        concordant / (positives * negatives) as f64
    };
    let mut true_positives = 0usize;
    let mut false_positives = 0usize;
    let mut pr_auc = 0.0;
    let mut previous_recall = 0.0;
    for (_, label) in &ranked {
        if *label {
            true_positives += 1;
        } else {
            false_positives += 1;
        }
        let recall = ratio(true_positives, positives);
        let precision = ratio(true_positives, true_positives + false_positives);
        pr_auc += precision * (recall - previous_recall).max(0.0);
        previous_recall = recall;
    }
    let threshold = 0.5;
    let predicted_positive = samples
        .iter()
        .filter(|(score, _)| *score >= threshold)
        .count();
    let true_positive = samples
        .iter()
        .filter(|(score, label)| *score >= threshold && *label)
        .count();
    let false_positive = predicted_positive.saturating_sub(true_positive);
    let false_negative = positives.saturating_sub(true_positive);
    let f1 = if 2 * true_positive + false_positive + false_negative == 0 {
        0.0
    } else {
        2.0 * true_positive as f64 / (2 * true_positive + false_positive + false_negative) as f64
    };
    let brier = samples
        .iter()
        .map(|(score, label)| {
            let expected = if *label { 1.0 } else { 0.0 };
            (score - expected).powi(2)
        })
        .sum::<f64>()
        / samples.len().max(1) as f64;
    let calibration_bins = (0..10)
        .map(|bin| {
            let lower = bin as f64 / 10.0;
            let upper = (bin + 1) as f64 / 10.0;
            let values = samples
                .iter()
                .filter(|(score, _)| {
                    *score >= lower && (*score < upper || (bin == 9 && *score <= upper))
                })
                .collect::<Vec<_>>();
            if values.is_empty() {
                0.0
            } else {
                let confidence =
                    values.iter().map(|(score, _)| *score).sum::<f64>() / values.len() as f64;
                let accuracy =
                    values.iter().filter(|(_, label)| *label).count() as f64 / values.len() as f64;
                (confidence - accuracy).abs() * values.len() as f64 / samples.len() as f64
            }
        })
        .sum();
    BinaryMetrics {
        count: samples.len(),
        positives,
        negatives,
        roc_auc,
        pr_auc,
        f1,
        brier,
        expected_calibration_error: calibration_bins,
    }
}
