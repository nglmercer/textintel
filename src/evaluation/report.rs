//! Evaluation reports: metric aggregates plus the options selecting which
//! slice of a dataset they were measured on.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::dataset::SpamCorpus;

fn default_report_string() -> String {
    String::new()
}

/// Options controlling which slice of a dataset is evaluated.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluateOptions {
    /// `None` evaluates every case; otherwise only the matching split.
    pub split: Option<String>,
    /// Maximum distinct queries sampled for retrieval metrics. Queries are
    /// groups of cases sharing one `a` text (first-occurrence order, capped
    /// here); a group counts when it has at least one similar-labelled `b`,
    /// and every similar `b` is a relevant document for its group.
    pub ranking_queries: usize,
    /// Maximum documents indexed for retrieval metrics.
    pub ranking_documents: usize,
    /// Optional held-out spam corpus; when present the report carries spam
    /// classifier metrics so quality gates can enforce them. The CLI
    /// resolves this to the `spam/v2-eval.json` sibling of the dataset
    /// directory (overridable with `--spam-corpus`).
    pub spam_corpus: Option<SpamCorpus>,
}

impl Default for EvaluateOptions {
    fn default() -> Self {
        Self {
            split: None,
            ranking_queries: 100,
            ranking_documents: 500,
            spam_corpus: None,
        }
    }
}

/// Spam classifier metrics over a held-out corpus plus the corpus version
/// they were measured on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SpamReport {
    #[serde(default = "default_report_string")]
    pub corpus_version: String,
    pub metrics: BinaryMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BinaryMetrics {
    pub count: usize,
    pub positives: usize,
    pub negatives: usize,
    #[serde(default)]
    pub accuracy: f64,
    #[serde(default)]
    pub precision: f64,
    #[serde(default)]
    pub recall: f64,
    pub roc_auc: f64,
    pub pr_auc: f64,
    pub f1: f64,
    pub brier: f64,
    pub expected_calibration_error: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RankingMetrics {
    #[serde(default)]
    pub queries: usize,
    #[serde(default)]
    pub recall_at_1: f64,
    #[serde(default)]
    pub recall_at_5: f64,
    #[serde(default)]
    pub recall_at_10: f64,
    #[serde(default)]
    pub mrr: f64,
    #[serde(default)]
    pub ndcg_at_10: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RebusMetrics {
    pub labelled_cases: usize,
    pub top1_accuracy: f64,
    pub top_k_accuracy: f64,
    #[serde(default)]
    pub top3_accuracy: f64,
    #[serde(default)]
    pub top5_accuracy: f64,
    #[serde(default)]
    pub mrr: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LanguageMetrics {
    pub labelled_cases: usize,
    pub top1_accuracy: f64,
    #[serde(default)]
    pub top3_accuracy: f64,
    #[serde(default)]
    pub unknown_precision: f64,
    #[serde(default)]
    pub unknown_recall: f64,
    #[serde(default)]
    pub unknown_support: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LatencyStats {
    #[serde(default)]
    pub count: usize,
    #[serde(default)]
    pub mean_micros: f64,
    #[serde(default)]
    pub p50_micros: f64,
    #[serde(default)]
    pub p95_micros: f64,
    #[serde(default)]
    pub p99_micros: f64,
}

impl LatencyStats {
    pub(super) fn from_micros(mut samples: Vec<f64>) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        samples.sort_by(|left, right| left.total_cmp(right));
        let count = samples.len();
        let mean_micros = samples.iter().sum::<f64>() / count as f64;
        let percentile = |p: f64| {
            let index = ((p * count as f64).ceil() as usize)
                .saturating_sub(1)
                .min(count - 1);
            samples[index]
        };
        Self {
            count,
            mean_micros,
            p50_micros: percentile(0.50),
            p95_micros: percentile(0.95),
            p99_micros: percentile(0.99),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationReport {
    pub dataset_version: String,
    #[serde(default = "default_report_string")]
    pub split: String,
    pub metrics: BinaryMetrics,
    pub rebus: RebusMetrics,
    pub language: LanguageMetrics,
    #[serde(default)]
    pub ranking: RankingMetrics,
    pub average_compare_micros: f64,
    #[serde(default)]
    pub compare_latency: LatencyStats,
    #[serde(default)]
    pub analyze_latency: LatencyStats,
    #[serde(default)]
    pub expectations_total: usize,
    #[serde(default)]
    pub expectations_met: usize,
    /// Per-category binary metrics keyed by
    /// [`EvaluationCase::primary_category`](crate::evaluation::EvaluationCase::primary_category).
    /// Global `metrics` alone can hide a failing slice; gates should pin
    /// the categories that matter (see `data/quality-gates.json`).
    #[serde(default)]
    pub categories: BTreeMap<String, BinaryMetrics>,
    /// Spam classifier metrics over the held-out spam corpus, when
    /// [`EvaluateOptions::spam_corpus`] is set. `None` means spam was not
    /// measured; gates requiring spam fail closed on `None`.
    #[serde(default)]
    pub spam: Option<SpamReport>,
}
