//! Versioned evaluation primitives kept independent from model/training code.
//!
//! The dataset format is backwards compatible: older case objects without
//! `id`, `split`, `difficulty`, `expected`, or `tags` still deserialize, with
//! `split` defaulting to `"test"`.

use std::collections::BTreeMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::core::error::TextIntelError;
use crate::engine::TextIntelligence;

mod metrics;

use metrics::{binary_metrics, language_metrics, ranking_metrics, rebus_metrics};

fn default_dataset_version() -> String {
    "unversioned".to_string()
}

fn default_split() -> String {
    "test".to_string()
}

fn default_difficulty() -> String {
    "medium".to_string()
}

/// Structured expectations for a single evaluation case.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct ExpectedOutput {
    /// Decoded candidates must contain one of these strings (case-insensitive).
    pub decoded_contains: Vec<String>,
    /// Similar pairs should score at or above this threshold.
    pub min_similarity: Option<f64>,
}

/// Primary evaluation categories. Cases predate the field and are inferred
/// from labels (see [`EvaluationCase::primary_category`]); new cases should
/// set `category` explicitly.
pub const EVALUATION_CATEGORIES: &[&str] = &[
    "semantic",
    "cross_language",
    "transliteration",
    "rebus",
    "phonetic",
    "leetspeak",
    "homoglyph",
    "unicode",
    "code_switching",
    "short_text",
    "spam",
    "hard_negatives",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationCase {
    #[serde(default)]
    pub id: String,
    pub a: String,
    pub b: String,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default = "default_split")]
    pub split: String,
    #[serde(default = "default_difficulty")]
    pub difficulty: String,
    /// Primary category (one of [`EVALUATION_CATEGORIES`] or `general`).
    /// Empty on legacy cases, which are inferred from labels.
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub labels: BTreeMap<String, bool>,
    #[serde(default)]
    pub expected: ExpectedOutput,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl EvaluationCase {
    /// Normalized split name (`train`, `validation`, or `test`).
    pub fn normalized_split(&self) -> &str {
        match self.split.as_str() {
            "train" | "training" => "train",
            "validation" | "valid" | "dev" => "validation",
            _ => "test",
        }
    }

    pub fn is_similar(&self) -> bool {
        self.labels.get("similar").copied().unwrap_or(false)
    }

    /// Primary category for per-category metrics. Explicit `category` wins;
    /// legacy cases without one are inferred from labels so old datasets
    /// still slice meaningfully.
    pub fn primary_category(&self) -> &str {
        if !self.category.is_empty() {
            return &self.category;
        }
        for (label, category) in [
            ("rebus", "rebus"),
            ("homoglyph", "homoglyph"),
            ("phonetic", "phonetic"),
            ("semantic", "semantic"),
            ("short", "short_text"),
            ("spam", "spam"),
            ("visual", "unicode"),
            ("symbolic", "rebus"),
            ("obfuscated", "leetspeak"),
        ] {
            if self.labels.get(label).copied().unwrap_or(false) {
                return category;
            }
        }
        "general"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationDataset {
    #[serde(default = "default_dataset_version")]
    pub version: String,
    pub cases: Vec<EvaluationCase>,
}

fn default_report_string() -> String {
    String::new()
}

/// Options controlling which slice of a dataset is evaluated.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluateOptions {
    /// `None` evaluates every case; otherwise only the matching split.
    pub split: Option<String>,
    /// Maximum ranking queries sampled for retrieval metrics.
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

/// One labeled spam-corpus message (`spam` or `ham`/`benign`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpamCorpusItem {
    #[serde(default)]
    pub id: String,
    pub text: String,
    pub label: String,
}

impl SpamCorpusItem {
    pub fn is_spam(&self) -> bool {
        self.label.eq_ignore_ascii_case("spam")
    }
}

/// Versioned held-out spam corpus (see `data/spam/README.md`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpamCorpus {
    #[serde(default = "default_dataset_version")]
    pub version: String,
    pub items: Vec<SpamCorpusItem>,
}

impl SpamCorpus {
    pub fn from_json(source: &str) -> Result<Self, TextIntelError> {
        serde_json::from_str(source).map_err(TextIntelError::from)
    }

    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, TextIntelError> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path).map_err(|error| {
            TextIntelError::InvalidConfiguration(format!("cannot read {}: {error}", path.display()))
        })?;
        let corpus = Self::from_json(&source)?;
        if corpus.items.is_empty() {
            return Err(TextIntelError::InvalidConfiguration(format!(
                "spam corpus {} has no items",
                path.display()
            )));
        }
        Ok(corpus)
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
    fn from_micros(mut samples: Vec<f64>) -> Self {
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
    /// Per-category binary metrics keyed by [`EvaluationCase::primary_category`].
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

    /// Load a modular dataset directory (`train.json`, `validation.json`,
    /// `test.json`, each a [`EvaluationDataset`] document). Part versions
    /// must agree and every case must carry its part's split; concatenation
    /// preserves within-split order so split filtering is unaffected.
    pub fn from_dir(path: impl AsRef<std::path::Path>) -> Result<Self, TextIntelError> {
        let root = path.as_ref();
        let mut version: Option<String> = None;
        let mut cases = Vec::new();
        for split in ["train", "validation", "test"] {
            let file = root.join(format!("{split}.json"));
            let source = std::fs::read_to_string(&file).map_err(|error| {
                TextIntelError::InvalidConfiguration(format!(
                    "cannot read {}: {error}",
                    file.display()
                ))
            })?;
            let part = Self::from_json(&source)?;
            match &version {
                None => version = Some(part.version.clone()),
                Some(expected) if expected == &part.version => {}
                Some(expected) => {
                    return Err(TextIntelError::InvalidConfiguration(format!(
                        "dataset part {} has version {}, expected {expected}",
                        file.display(),
                        part.version
                    )));
                }
            }
            for case in part.cases {
                if case.normalized_split() != split {
                    return Err(TextIntelError::InvalidConfiguration(format!(
                        "case {} carries split {:?}, expected {split:?} in {}",
                        case.id,
                        case.split,
                        file.display()
                    )));
                }
                cases.push(case);
            }
        }
        Ok(Self {
            version: version.unwrap_or_else(default_dataset_version),
            cases,
        })
    }

    /// Load a dataset from a directory (see [`EvaluationDataset::from_dir`])
    /// or a single JSON document (see [`EvaluationDataset::from_json`]).
    pub fn load_path(path: impl AsRef<std::path::Path>) -> Result<Self, TextIntelError> {
        let path = path.as_ref();
        if path.is_dir() {
            return Self::from_dir(path);
        }
        let source = std::fs::read_to_string(path).map_err(|error| {
            TextIntelError::InvalidConfiguration(format!("cannot read {}: {error}", path.display()))
        })?;
        Self::from_json(&source)
    }

    /// Cases belonging to `split` (`train`, `validation`, or `test`).
    pub fn filter_split(&self, split: &str) -> Vec<&EvaluationCase> {
        let wanted = match split {
            "train" | "training" => "train",
            "validation" | "valid" | "dev" => "validation",
            _ => "test",
        };
        self.cases
            .iter()
            .filter(|case| case.normalized_split() == wanted)
            .collect()
    }

    pub fn train(&self) -> Vec<&EvaluationCase> {
        self.filter_split("train")
    }

    pub fn validation(&self) -> Vec<&EvaluationCase> {
        self.filter_split("validation")
    }

    pub fn test(&self) -> Vec<&EvaluationCase> {
        self.filter_split("test")
    }

    /// Count cases per split, returned as `(train, validation, test)`.
    pub fn split_counts(&self) -> (usize, usize, usize) {
        (
            self.train().len(),
            self.validation().len(),
            self.test().len(),
        )
    }
}

/// Evaluate every case in the dataset.
pub fn evaluate(
    engine: &TextIntelligence,
    dataset: &EvaluationDataset,
) -> Result<EvaluationReport, TextIntelError> {
    evaluate_with_options(engine, dataset, &EvaluateOptions::default())
}

/// Evaluate a single named split (`train`, `validation`, or `test`).
pub fn evaluate_on_split(
    engine: &TextIntelligence,
    dataset: &EvaluationDataset,
    split: &str,
) -> Result<EvaluationReport, TextIntelError> {
    evaluate_with_options(
        engine,
        dataset,
        &EvaluateOptions {
            split: Some(split.to_string()),
            ..EvaluateOptions::default()
        },
    )
}

pub fn evaluate_with_options(
    engine: &TextIntelligence,
    dataset: &EvaluationDataset,
    options: &EvaluateOptions,
) -> Result<EvaluationReport, TextIntelError> {
    let split_label = options.split.clone().unwrap_or_default();
    let cases: Vec<&EvaluationCase> = match &options.split {
        Some(split) => dataset.filter_split(split),
        None => dataset.cases.iter().collect(),
    };
    if cases.is_empty() {
        return Ok(EvaluationReport {
            dataset_version: dataset.version.clone(),
            split: split_label,
            metrics: BinaryMetrics::default(),
            rebus: RebusMetrics::default(),
            language: LanguageMetrics::default(),
            ranking: RankingMetrics::default(),
            average_compare_micros: 0.0,
            compare_latency: LatencyStats::default(),
            analyze_latency: LatencyStats::default(),
            expectations_total: 0,
            expectations_met: 0,
            categories: BTreeMap::new(),
            spam: None,
        });
    }

    // Chunked pairwise comparison: `analyze_batch` enforces `max_batch_size`
    // on input texts, and each pair expands to two texts.
    let pairs_per_chunk = (engine.config().max_batch_size / 2).max(1);
    let mut scores = Vec::with_capacity(cases.len());
    let mut compare_samples = Vec::with_capacity(cases.len());
    let mut analyze_samples = Vec::new();
    for chunk in cases.chunks(pairs_per_chunk) {
        let pairs = chunk
            .iter()
            .map(|case| (case.a.clone(), case.b.clone()))
            .collect::<Vec<_>>();
        let analyze_started = Instant::now();
        let fingerprints = engine.analyze_batch(
            &pairs
                .iter()
                .flat_map(|(left, right)| [left.clone(), right.clone()])
                .collect::<Vec<_>>(),
        )?;
        let analyze_elapsed = analyze_started.elapsed().as_secs_f64() * 1_000_000.0;
        analyze_samples.push(analyze_elapsed / (chunk.len() * 2).max(1) as f64);
        for (index, _) in chunk.iter().enumerate() {
            let started = Instant::now();
            let result =
                engine.compare_fingerprints(&fingerprints[index * 2], &fingerprints[index * 2 + 1]);
            compare_samples.push(started.elapsed().as_secs_f64() * 1_000_000.0);
            scores.push(result.score);
        }
    }
    let samples = cases
        .iter()
        .zip(scores.iter())
        .map(|(case, score)| (*score, case.is_similar()))
        .collect::<Vec<_>>();
    let metrics = binary_metrics(&samples);
    let mut grouped: BTreeMap<String, Vec<(f64, bool)>> = BTreeMap::new();
    for (case, score) in cases.iter().zip(scores.iter()) {
        grouped
            .entry(case.primary_category().to_string())
            .or_default()
            .push((*score, case.is_similar()));
    }
    let categories = grouped
        .into_iter()
        .map(|(name, group)| (name, binary_metrics(&group)))
        .collect::<BTreeMap<_, _>>();

    let (expectations_total, expectations_met) =
        cases
            .iter()
            .zip(scores.iter())
            .fold((0usize, 0usize), |(total, met), (case, score)| {
                if let Some(minimum) = case.expected.min_similarity {
                    let satisfied = if case.is_similar() {
                        *score >= minimum
                    } else {
                        *score < minimum
                    };
                    (total + 1, met + usize::from(satisfied))
                } else {
                    (total, met)
                }
            });

    let rebus = rebus_metrics(engine, &cases)?;
    let language = language_metrics(engine, &cases)?;
    let ranking = ranking_metrics(engine, &cases, options)?;
    let spam = match &options.spam_corpus {
        Some(corpus) => {
            let mut samples = Vec::with_capacity(corpus.items.len());
            for item in &corpus.items {
                let result = engine.detect_spam(&item.text)?;
                samples.push((result.probability, item.is_spam()));
            }
            Some(SpamReport {
                corpus_version: corpus.version.clone(),
                metrics: binary_metrics(&samples),
            })
        }
        None => None,
    };

    let compare_latency = LatencyStats::from_micros(compare_samples);
    let analyze_latency = LatencyStats::from_micros(analyze_samples);
    Ok(EvaluationReport {
        dataset_version: dataset.version.clone(),
        split: split_label,
        metrics,
        rebus,
        language,
        ranking,
        average_compare_micros: compare_latency.mean_micros,
        compare_latency,
        analyze_latency,
        expectations_total,
        expectations_met,
        categories,
        spam,
    })
}
