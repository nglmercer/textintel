//! Versioned evaluation primitives kept independent from model/training code.
//!
//! The dataset format is backwards compatible: older case objects without
//! `id`, `split`, `difficulty`, `expected`, or `tags` still deserialize, with
//! `split` defaulting to `"test"`.
//!
//! One module per responsibility: [`dataset`] parses cases and corpora,
//! [`metrics`] scores them, [`gates`] checks threshold files, and [`report`]
//! carries the aggregates. The runners below (`evaluate`, …) coordinate the
//! pipeline; per-slice scoring stays in [`metrics`].

pub mod dataset;
pub mod gates;
pub mod report;

mod metrics;

pub use dataset::{
    EvaluationCase, EvaluationDataset, ExpectedOutput, SpamCorpus, SpamCorpusItem,
    EVALUATION_CATEGORIES,
};
pub use gates::check_gates;
pub use report::{
    BinaryMetrics, EvaluateOptions, EvaluationReport, LanguageMetrics, LatencyStats,
    RankingMetrics, RebusMetrics, SpamReport,
};

use std::collections::BTreeMap;
use std::time::Instant;

use crate::core::error::TextIntelError;
use crate::engine::TextIntelligence;

use metrics::{binary_metrics, language_metrics, ranking_metrics, rebus_metrics};
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
