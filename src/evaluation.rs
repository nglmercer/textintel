//! Versioned evaluation primitives kept independent from model/training code.
//!
//! The dataset format is backwards compatible: older case objects without
//! `id`, `split`, `difficulty`, `expected`, or `tags` still deserialize, with
//! `split` defaulting to `"test"`.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::core::error::TextIntelError;
use crate::engine::TextIntelligence;

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
}

impl Default for EvaluateOptions {
    fn default() -> Self {
        Self {
            split: None,
            ranking_queries: 100,
            ranking_documents: 500,
        }
    }
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
    let true_negative = negatives.saturating_sub(false_positive);
    let f1 = if 2 * true_positive + false_positive + false_negative == 0 {
        0.0
    } else {
        2.0 * true_positive as f64 / (2 * true_positive + false_positive + false_negative) as f64
    };
    let accuracy = if samples.is_empty() {
        0.0
    } else {
        (true_positive + true_negative) as f64 / samples.len() as f64
    };
    let precision = ratio(true_positive, true_positive + false_positive);
    let recall = ratio(true_positive, positives);
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
        accuracy,
        precision,
        recall,
        roc_auc,
        pr_auc,
        f1,
        brier,
        expected_calibration_error: calibration_bins,
    }
}

fn rebus_metrics(
    engine: &TextIntelligence,
    cases: &[&EvaluationCase],
) -> Result<RebusMetrics, TextIntelError> {
    let mut labelled = 0usize;
    let mut top1 = 0usize;
    let mut top3 = 0usize;
    let mut top5 = 0usize;
    let mut reciprocal_sum = 0.0;
    for case in cases {
        if !case.labels.get("rebus").copied().unwrap_or(false) {
            continue;
        }
        labelled += 1;
        let candidates = engine.decode(&case.a)?;
        let mut rank: Option<usize> = None;
        for (index, candidate) in candidates.iter().enumerate() {
            let matches_b = same_text(&candidate.text, &case.b);
            let matches_expected = case
                .expected
                .decoded_contains
                .iter()
                .any(|expected| same_text(&candidate.text, expected));
            if matches_b || matches_expected {
                rank = Some(index + 1);
                break;
            }
        }
        match rank {
            Some(1) => {
                top1 += 1;
                top3 += 1;
                top5 += 1;
                reciprocal_sum += 1.0;
            }
            Some(2..=3) => {
                top3 += 1;
                top5 += 1;
                reciprocal_sum += 1.0 / rank.unwrap_or(1) as f64;
            }
            Some(4..=5) => {
                top5 += 1;
                reciprocal_sum += 1.0 / rank.unwrap_or(1) as f64;
            }
            Some(other) => {
                reciprocal_sum += 1.0 / other as f64;
            }
            None => {}
        }
    }
    Ok(RebusMetrics {
        labelled_cases: labelled,
        top1_accuracy: ratio(top1, labelled),
        top_k_accuracy: ratio(top5, labelled),
        top3_accuracy: ratio(top3, labelled),
        top5_accuracy: ratio(top5, labelled),
        mrr: if labelled == 0 {
            0.0
        } else {
            reciprocal_sum / labelled as f64
        },
    })
}

fn language_metrics(
    engine: &TextIntelligence,
    cases: &[&EvaluationCase],
) -> Result<LanguageMetrics, TextIntelError> {
    let mut labelled = 0usize;
    let mut top1 = 0usize;
    let mut top3 = 0usize;
    let mut unknown_predicted = 0usize;
    let mut unknown_correct = 0usize;
    let mut unknown_support = 0usize;
    for case in cases {
        if case.languages.is_empty() {
            continue;
        }
        labelled += 1;
        let detected = engine.analyze(&case.a)?;
        let predicted: Vec<String> = detected
            .language_candidates
            .iter()
            .map(|candidate| candidate.language.clone())
            .collect();
        let expected_unknown = case
            .languages
            .iter()
            .any(|language| language.eq_ignore_ascii_case("unknown"));
        if expected_unknown {
            unknown_support += 1;
        }
        let predicted_unknown = predicted
            .first()
            .is_some_and(|language| language.eq_ignore_ascii_case("unknown"));
        if predicted_unknown {
            unknown_predicted += 1;
            if expected_unknown {
                unknown_correct += 1;
            }
        }
        let matches = |candidate: &str| {
            case.languages
                .iter()
                .any(|expected| expected.eq_ignore_ascii_case(candidate))
        };
        if predicted.first().is_some_and(|language| matches(language)) {
            top1 += 1;
        }
        if predicted.iter().take(3).any(|language| matches(language)) {
            top3 += 1;
        }
    }
    Ok(LanguageMetrics {
        labelled_cases: labelled,
        top1_accuracy: ratio(top1, labelled),
        top3_accuracy: ratio(top3, labelled),
        unknown_precision: ratio(unknown_correct, unknown_predicted),
        unknown_recall: ratio(unknown_correct, unknown_support),
        unknown_support,
    })
}

fn ranking_metrics(
    engine: &TextIntelligence,
    cases: &[&EvaluationCase],
    options: &EvaluateOptions,
) -> Result<RankingMetrics, TextIntelError> {
    // Retrieval probe: every distinct `b` text is a document and a sample of
    // `a` texts are queries. A query is relevant to its own paired document
    // when the case is labelled similar.
    let mut documents: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    for case in cases {
        if documents.len() >= options.ranking_documents {
            break;
        }
        if seen.insert(case.b.clone()) {
            documents.push(case.b.clone());
        }
    }
    // Every query's own target must be indexed or recall is meaningless.
    let queries: Vec<&&EvaluationCase> = cases.iter().take(options.ranking_queries).collect();
    if queries.is_empty() || documents.is_empty() {
        return Ok(RankingMetrics::default());
    }
    // `analyze_batch` enforces `max_batch_size`: index large corpora in chunks.
    let batch_size = engine.config().max_batch_size.max(1);
    let mut doc_fingerprints = Vec::with_capacity(documents.len());
    for chunk in documents.chunks(batch_size) {
        doc_fingerprints.extend(engine.analyze_batch(chunk)?);
    }
    let mut recall_1 = 0usize;
    let mut recall_5 = 0usize;
    let mut recall_10 = 0usize;
    let mut reciprocal_sum = 0.0;
    let mut ndcg_sum = 0.0;
    let mut relevant_queries = 0usize;
    for query_case in queries {
        let query = engine.analyze(&query_case.a)?;
        let mut scored = doc_fingerprints
            .iter()
            .enumerate()
            .map(|(index, document)| (index, engine.compare_fingerprints(&query, document).score))
            .collect::<Vec<_>>();
        // Deterministic tie-breaking keeps search reproducible.
        scored.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| documents[left.0].cmp(&documents[right.0]))
        });
        let rank = scored
            .iter()
            .position(|(index, _)| same_text(&documents[*index], &query_case.b));
        if !query_case.is_similar() {
            continue;
        }
        relevant_queries += 1;
        match rank {
            Some(0) => {
                recall_1 += 1;
                recall_5 += 1;
                recall_10 += 1;
                reciprocal_sum += 1.0;
                ndcg_sum += 1.0;
            }
            Some(position) => {
                if position < 5 {
                    recall_5 += 1;
                }
                if position < 10 {
                    recall_10 += 1;
                }
                reciprocal_sum += 1.0 / (position + 1) as f64;
                if position < 10 {
                    ndcg_sum += 1.0 / ((position + 2) as f64).log2();
                }
            }
            None => {}
        }
    }
    Ok(RankingMetrics {
        queries: relevant_queries,
        recall_at_1: ratio(recall_1, relevant_queries),
        recall_at_5: ratio(recall_5, relevant_queries),
        recall_at_10: ratio(recall_10, relevant_queries),
        mrr: if relevant_queries == 0 {
            0.0
        } else {
            reciprocal_sum / relevant_queries as f64
        },
        ndcg_at_10: if relevant_queries == 0 {
            0.0
        } else {
            ndcg_sum / relevant_queries as f64
        },
    })
}
