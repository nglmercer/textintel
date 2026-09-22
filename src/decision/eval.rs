//! Decision evaluation: accuracy, macro/micro F1, proper scoring
//! rules, confusion matrices, and risk/coverage curves for selective
//! classification.
//!
//! The runner scores one provider over labeled examples. Provider errors
//! on individual examples count as `skipped` (reported, never silent) so
//! one bad input cannot fail a whole benchmark run; quality gates then
//! decide whether the measured coverage is acceptable.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::core::error::TextIntelError;
use crate::engine::TextIntelligence;

use super::calibration::{brier_score, expected_calibration_error, nll_loss};
use super::dataset::DecisionExample;
use super::provider::DecisionProvider;
use super::types::DecisionRequest;

/// Coverage levels for the risk/coverage curve, highest first.
pub const COVERAGE_LEVELS: &[f64] = &[1.0, 0.95, 0.9, 0.8, 0.7, 0.5];

/// Accuracy at one coverage level: the top-`coverage` fraction by
/// confidence. `threshold` is the lowest accepted confidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CoveragePoint {
    pub coverage: f64,
    pub accuracy: f64,
    pub count: usize,
    pub threshold: f64,
}

/// Aggregate decision-evaluation report for one split.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DecisionEvalReport {
    pub dataset_version: String,
    pub split: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    pub provider: String,
    /// Scored examples (excluding `skipped`).
    pub count: usize,
    /// Examples the provider failed to answer.
    pub skipped: usize,
    pub accuracy: f64,
    pub macro_f1: f64,
    pub micro_f1: f64,
    pub nll: f64,
    pub brier: f64,
    pub ece: f64,
    /// `gold -> predicted -> count`.
    pub confusion: BTreeMap<String, BTreeMap<String, usize>>,
    pub coverage: Vec<CoveragePoint>,
    /// Mean per-example latency in microseconds (analysis + decision).
    pub mean_latency_micros: f64,
}

struct ScoredExample {
    gold: String,
    predicted: String,
    confidence: f64,
    probabilities: Vec<f64>,
    gold_index: usize,
    latency_micros: f64,
}

fn f1_scores(scored: &[ScoredExample]) -> (f64, f64) {
    let mut labels = BTreeSet::new();
    for example in scored {
        labels.insert(example.gold.clone());
        labels.insert(example.predicted.clone());
    }
    if labels.is_empty() {
        return (0.0, 0.0);
    }
    let mut macro_sum = 0.0;
    let mut micro_tp = 0usize;
    let mut micro_fp = 0usize;
    let mut micro_fn = 0usize;
    for label in &labels {
        let mut true_positive = 0usize;
        let mut false_positive = 0usize;
        let mut false_negative = 0usize;
        for example in scored {
            match (example.predicted == *label, example.gold == *label) {
                (true, true) => true_positive += 1,
                (true, false) => false_positive += 1,
                (false, true) => false_negative += 1,
                (false, false) => {}
            }
        }
        micro_tp += true_positive;
        micro_fp += false_positive;
        micro_fn += false_negative;
        let denominator = 2 * true_positive + false_positive + false_negative;
        macro_sum += if denominator == 0 {
            0.0
        } else {
            2.0 * true_positive as f64 / denominator as f64
        };
    }
    let macro_f1 = macro_sum / labels.len() as f64;
    let micro_denominator = 2 * micro_tp + micro_fp + micro_fn;
    let micro_f1 = if micro_denominator == 0 {
        0.0
    } else {
        2.0 * micro_tp as f64 / micro_denominator as f64
    };
    (macro_f1, micro_f1)
}

fn coverage_curve(scored: &[ScoredExample]) -> Vec<CoveragePoint> {
    if scored.is_empty() {
        return COVERAGE_LEVELS
            .iter()
            .map(|coverage| CoveragePoint {
                coverage: *coverage,
                accuracy: 0.0,
                count: 0,
                threshold: 0.0,
            })
            .collect();
    }
    let mut ranked: Vec<&ScoredExample> = scored.iter().collect();
    ranked.sort_by(|left, right| right.confidence.total_cmp(&left.confidence));
    COVERAGE_LEVELS
        .iter()
        .map(|coverage| {
            let count = ((*coverage * ranked.len() as f64).round() as usize).clamp(1, ranked.len());
            let head = &ranked[..count];
            let correct = head
                .iter()
                .filter(|example| example.predicted == example.gold)
                .count();
            CoveragePoint {
                coverage: *coverage,
                accuracy: correct as f64 / count as f64,
                count,
                threshold: head
                    .iter()
                    .map(|example| example.confidence)
                    .fold(f64::INFINITY, f64::min),
            }
        })
        .collect()
}

/// Score `provider` over `examples`, preparing every request through
/// `engine` (state + candidate analysis). Provider failures and
/// answer-validation failures count as `skipped`.
pub fn evaluate_decisions(
    engine: &TextIntelligence,
    provider: &dyn DecisionProvider,
    dataset_version: &str,
    split: &str,
    examples: &[DecisionExample],
) -> Result<DecisionEvalReport, TextIntelError> {
    evaluate_decisions_with_jobs(engine, provider, dataset_version, split, examples, 1)
}

/// [`evaluate_decisions`] with worker threads. Examples are scored on
/// `jobs` threads sharing one engine (both engine and provider are
/// `Sync`), then aggregated in example order, so every metric except
/// `mean_latency_micros` is identical to the sequential run. Latencies
/// under `jobs > 1` reflect shared-machine contention, not
/// single-query latency — use the `decision` benchmarks for that.
/// `jobs` below 2 runs the plain sequential path.
pub fn evaluate_decisions_with_jobs(
    engine: &TextIntelligence,
    provider: &dyn DecisionProvider,
    dataset_version: &str,
    split: &str,
    examples: &[DecisionExample],
    jobs: usize,
) -> Result<DecisionEvalReport, TextIntelError> {
    if jobs < 2 || examples.len() < 2 {
        return assemble_report(
            dataset_version,
            split,
            &provider.capabilities().provider,
            score_sequential(engine, provider, examples),
        );
    }
    let workers = jobs.min(examples.len());
    let chunk = examples.len().div_ceil(workers);
    let mut outcomes: Vec<(usize, Result<ScoredExample, String>)> =
        Vec::with_capacity(examples.len());
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for (chunk_index, piece) in examples.chunks(chunk).enumerate() {
            let offset = chunk_index * chunk;
            handles.push(scope.spawn(move || {
                piece
                    .iter()
                    .enumerate()
                    .map(|(index, example)| {
                        (offset + index, score_one_example(engine, provider, example))
                    })
                    .collect::<Vec<_>>()
            }));
        }
        for handle in handles {
            match handle.join() {
                Ok(scored) => outcomes.extend(scored),
                Err(_) => {
                    // A panicking worker must not hang the run; its
                    // examples count as failed below via the length gap.
                }
            }
        }
    });
    outcomes.sort_by_key(|(index, _)| *index);
    assemble_report(
        dataset_version,
        split,
        &provider.capabilities().provider,
        outcomes_with_gaps_filled(outcomes, examples.len()),
    )
}

/// Restore example order after parallel scoring, treating examples a
/// dead worker never returned as failures (skipped, never silent).
fn outcomes_with_gaps_filled(
    mut outcomes: Vec<(usize, Result<ScoredExample, String>)>,
    total: usize,
) -> Vec<Result<ScoredExample, String>> {
    outcomes.sort_by_key(|(index, _)| *index);
    let mut filled = Vec::with_capacity(total);
    let mut cursor = 0usize;
    for (index, outcome) in outcomes {
        while cursor < index.min(total) {
            filled.push(Err("worker failed to return this example".to_string()));
            cursor += 1;
        }
        if index < total {
            filled.push(outcome);
            cursor = index + 1;
        }
    }
    while cursor < total {
        filled.push(Err("worker failed to return this example".to_string()));
        cursor += 1;
    }
    filled
}

fn score_sequential(
    engine: &TextIntelligence,
    provider: &dyn DecisionProvider,
    examples: &[DecisionExample],
) -> Vec<Result<ScoredExample, String>> {
    examples
        .iter()
        .map(|example| score_one_example(engine, provider, example))
        .collect()
}

fn score_one_example(
    engine: &TextIntelligence,
    provider: &dyn DecisionProvider,
    example: &DecisionExample,
) -> Result<ScoredExample, String> {
    let started = Instant::now();
    let mut request = DecisionRequest::new(example.state.clone(), example.question.clone());
    if let Some(task) = &example.task {
        request = request.with_task(task.clone());
    }
    engine
        .prepare_decision_request(&mut request)
        .map_err(|error| error.to_string())?;
    let response = provider
        .decide(&request)
        .map_err(|error| error.to_string())?;
    response
        .validate_against(&request)
        .map_err(|error| format!("invalid provider answer: {error}"))?;
    let gold_index = example
        .gold_index()?
        .ok_or_else(|| format!("unknown gold label {:?} for question", example.gold))?;
    Ok(ScoredExample {
        gold: example.gold.clone(),
        predicted: response.answer.predicted_label(),
        confidence: response.confidence(),
        probabilities: response.answer.probabilities_in_order(),
        gold_index,
        latency_micros: started.elapsed().as_micros() as f64,
    })
}

fn assemble_report(
    dataset_version: &str,
    split: &str,
    provider_name: &str,
    outcomes: Vec<Result<ScoredExample, String>>,
) -> Result<DecisionEvalReport, TextIntelError> {
    let mut scored = Vec::new();
    let mut skipped = 0usize;
    let mut confusion: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for outcome in outcomes {
        match outcome {
            Ok(record) => {
                confusion
                    .entry(record.gold.clone())
                    .or_default()
                    .entry(record.predicted.clone())
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
                scored.push(record);
            }
            Err(_) => skipped += 1,
        }
    }
    let count = scored.len();
    let correct = scored
        .iter()
        .filter(|example| example.predicted == example.gold)
        .count();
    let accuracy = if count == 0 {
        0.0
    } else {
        correct as f64 / count as f64
    };
    let (macro_f1, micro_f1) = f1_scores(&scored);
    let mut nll_sum = 0.0;
    let mut brier_sum = 0.0;
    let mut calibrations = Vec::with_capacity(count);
    for example in &scored {
        // Records passed answer validation, so these cannot fail; fall
        // back to worst-case scores rather than failing the run.
        nll_sum += nll_loss(&example.probabilities, example.gold_index)
            .unwrap_or(f64::MAX / count.max(1) as f64);
        brier_sum += brier_score(&example.probabilities, example.gold_index).unwrap_or(1.0);
        calibrations.push((example.probabilities.clone(), example.gold_index));
    }
    let coverage = coverage_curve(&scored);
    // Sanity anchor: at full coverage the curve head equals overall accuracy.
    debug_assert!(
        scored.is_empty()
            || (coverage.first().map(|point| point.accuracy).unwrap_or(0.0) - accuracy).abs()
                < 1e-9
    );
    Ok(DecisionEvalReport {
        dataset_version: dataset_version.to_string(),
        split: split.to_string(),
        task: None,
        provider: provider_name.to_string(),
        count,
        skipped,
        accuracy,
        macro_f1,
        micro_f1,
        nll: if count == 0 {
            0.0
        } else {
            nll_sum / count as f64
        },
        brier: if count == 0 {
            0.0
        } else {
            brier_sum / count as f64
        },
        ece: expected_calibration_error(&calibrations, 10).unwrap_or(1.0),
        confusion,
        coverage,
        mean_latency_micros: if count == 0 {
            0.0
        } else {
            scored
                .iter()
                .map(|example| example.latency_micros)
                .sum::<f64>()
                / count as f64
        },
    })
}

/// Check a decision report against a gates file. Supported shape:
///
/// ```json
/// {
///   "decision": {"accuracy_min": 0.8, "macro_f1_min": 0.75, "ece_max": 0.1},
///   "coverage_90": {"accuracy_min": 0.9}
/// }
/// ```
///
/// Unknown sections and metrics are ignored (forwards compatible, like
/// [`crate::evaluation::check_gates`]). Coverage sections address the
/// curve point by name (`coverage_90` → 0.9 coverage); a missing curve
/// point fails only when the section sets `count_min > 0`.
pub fn check_decision_gates(report: &DecisionEvalReport, gates: &serde_json::Value) -> Vec<String> {
    let mut failures = Vec::new();
    let Some(sections) = gates.as_object() else {
        return failures;
    };
    let observed = [
        ("accuracy", report.accuracy),
        ("macro_f1", report.macro_f1),
        ("micro_f1", report.micro_f1),
        ("nll", report.nll),
        ("brier", report.brier),
        ("ece", report.ece),
    ];
    for (metric, value) in observed {
        let section = sections.get("decision");
        let key = format!("{metric}_min");
        if let Some(minimum) = section
            .and_then(|section| section.get(&key))
            .and_then(serde_json::Value::as_f64)
            && value < minimum
        {
            failures.push(format!(
                "decision.{metric}={value:.3} below minimum {minimum:.3}"
            ));
        }
        let key = format!("{metric}_max");
        if let Some(maximum) = section
            .and_then(|section| section.get(&key))
            .and_then(serde_json::Value::as_f64)
            && value > maximum
        {
            failures.push(format!(
                "decision.{metric}={value:.3} above maximum {maximum:.3}"
            ));
        }
    }
    for (name, thresholds) in sections
        .iter()
        .filter(|(name, _)| name.starts_with("coverage_"))
    {
        let Some(thresholds) = thresholds.as_object() else {
            continue;
        };
        let wanted: Option<f64> = name
            .strip_prefix("coverage_")
            .and_then(|suffix| suffix.parse::<f64>().ok())
            .map(|percent| percent / 100.0);
        let Some(wanted) = wanted else { continue };
        let point = report
            .coverage
            .iter()
            .find(|point| (point.coverage - wanted).abs() < 1e-9);
        if let Some(count_min) = thresholds.get("count_min").and_then(|value| value.as_u64()) {
            let count = point.map(|point| point.count as u64).unwrap_or(0);
            if count < count_min {
                failures.push(format!("{name}.count={count} below minimum {count_min}"));
                continue;
            }
        }
        let Some(point) = point else { continue };
        let key = "accuracy_min";
        if let Some(minimum) = thresholds.get(key).and_then(|value| value.as_f64())
            && point.accuracy < minimum
        {
            failures.push(format!(
                "{name}.accuracy={:.3} below minimum {minimum:.3}",
                point.accuracy
            ));
        }
    }
    failures
}
