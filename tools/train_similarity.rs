//! Train an interpretable logistic similarity scorer from the evaluation set.
//!
//! Usage:
//!
//! ```text
//! textintel-train similarity data/evaluation.json --output models/similarity-v1.json
//! ```
//!
//! The tool trains on the `train` split only, calibrates the bias on the
//! `validation` split, and reports held-out metrics on `test` without
//! training on it. The default engine configuration is used so the artifact
//! matches production scoring conditions.

use std::collections::BTreeMap;

use textintel::evaluation::EvaluationDataset;
use textintel::SimilarityScorer;
use textintel::{
    language_agreement, logistic_step, training_features, EngineConfig, LogisticSimilarityScorer,
    SimilarityModelArtifact, TextIntelligence, TRAINING_FEATURES,
};

const ITERATIONS: usize = 20000;
const CALIBRATION_ITERATIONS: usize = 500;
const LEARNING_RATE: f64 = 0.2;
const L2: f64 = 1e-3;

fn usage() -> &'static str {
    "Usage:\n  textintel-train similarity <evaluation.json> --output <artifact.json> [--iterations N] [--learning-rate F] [--l2 F]"
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

struct SplitData {
    features: Vec<Vec<f64>>,
    labels: Vec<bool>,
}

fn featurize(
    engine: &TextIntelligence,
    dataset: &EvaluationDataset,
    split: &str,
) -> Result<SplitData, Box<dyn std::error::Error>> {
    let cases = dataset.filter_split(split);
    let mut features = Vec::with_capacity(cases.len());
    let mut labels = Vec::with_capacity(cases.len());
    for case in cases {
        let left = engine.analyze(&case.a)?;
        let right = engine.analyze(&case.b)?;
        let comparison = engine.compare_fingerprints(&left, &right);
        let agreement = language_agreement(&left, &right);
        let map = training_features(&comparison, agreement);
        features.push(
            TRAINING_FEATURES
                .iter()
                .map(|name| map.get(*name).copied().unwrap_or(0.0))
                .collect(),
        );
        labels.push(case.is_similar());
    }
    Ok(SplitData { features, labels })
}

fn metrics_at(
    scorer: &LogisticSimilarityScorer,
    engine: &TextIntelligence,
    dataset: &EvaluationDataset,
    split: &str,
) -> Result<BTreeMap<String, f64>, Box<dyn std::error::Error>> {
    // Held-out measurement through the public scorer interface: re-analyze
    // each pair and score with the trained weights.
    let cases = dataset.filter_split(split);
    let mut scores = Vec::with_capacity(cases.len());
    for case in cases {
        let left = engine.analyze(&case.a)?;
        let right = engine.analyze(&case.b)?;
        scores.push((scorer.score(&left, &right).score, case.is_similar()));
    }
    Ok(report(&scores))
}

fn roc_auc(samples: &[(f64, bool)]) -> f64 {
    let positives = samples.iter().filter(|(_, label)| *label).count() as f64;
    let negatives = samples.len() as f64 - positives;
    if positives == 0.0 || negatives == 0.0 {
        return 0.0;
    }
    let mut concordant = 0.0;
    for (positive, _) in samples.iter().filter(|(_, label)| *label) {
        for (negative, _) in samples.iter().filter(|(_, label)| !*label) {
            if positive > negative {
                concordant += 1.0;
            } else if positive == negative {
                concordant += 0.5;
            }
        }
    }
    concordant / (positives * negatives)
}

fn report(samples: &[(f64, bool)]) -> BTreeMap<String, f64> {
    let positives = samples.iter().filter(|(_, label)| *label).count() as f64;
    let total = samples.len() as f64;
    let mut true_positives = 0.0;
    let mut predicted_positive = 0u32;
    for (score, label) in samples {
        if *score >= 0.5 {
            predicted_positive += 1;
            if *label {
                true_positives += 1.0;
            }
        }
    }
    let false_positives = predicted_positive as f64 - true_positives;
    let accuracy = (true_positives + (total - positives - false_positives)) / total.max(1.0);
    let precision = if predicted_positive == 0 {
        0.0
    } else {
        true_positives / predicted_positive as f64
    };
    let recall = if positives == 0.0 {
        0.0
    } else {
        true_positives / positives
    };
    let f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    let brier = samples
        .iter()
        .map(|(score, label)| {
            let target = if *label { 1.0 } else { 0.0 };
            (score - target).powi(2)
        })
        .sum::<f64>()
        / total.max(1.0);
    // Best operating threshold by Youden's J, for information only: the
    // scorer always emits probabilities and callers keep their own cutoff.
    let mut thresholds: Vec<f64> = samples.iter().map(|(score, _)| *score).collect();
    thresholds.sort_by(|left, right| left.total_cmp(right));
    let mut best_threshold = 0.5;
    let mut best_j = f64::NEG_INFINITY;
    for threshold in thresholds {
        let (mut tp, mut fp) = (0.0, 0.0);
        for (score, label) in samples {
            if *score >= threshold {
                if *label {
                    tp += 1.0;
                } else {
                    fp += 1.0;
                }
            }
        }
        let tpr = if positives == 0.0 {
            0.0
        } else {
            tp / positives
        };
        let fpr = if total - positives == 0.0 {
            0.0
        } else {
            fp / (total - positives)
        };
        if tpr - fpr > best_j {
            best_j = tpr - fpr;
            best_threshold = threshold;
        }
    }
    let mut report = BTreeMap::new();
    report.insert("accuracy".to_string(), accuracy);
    report.insert("precision".to_string(), precision);
    report.insert("recall".to_string(), recall);
    report.insert("f1".to_string(), f1);
    report.insert("brier".to_string(), brier);
    report.insert("roc_auc".to_string(), roc_auc(samples));
    report.insert("best_threshold".to_string(), best_threshold);
    report
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("similarity") {
        eprintln!("{}", usage());
        std::process::exit(2);
    }
    let path = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("data/evaluation.json");
    let output = flag_value(&args, "--output")
        .ok_or("similarity training requires --output <artifact.json>")?;
    let iterations = flag_value(&args, "--iterations")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| "--iterations must be a positive integer")?
        .unwrap_or(ITERATIONS)
        .max(1);
    let learning_rate = flag_value(&args, "--learning-rate")
        .map(|value| value.parse::<f64>())
        .transpose()
        .map_err(|_| "--learning-rate must be a number")?
        .unwrap_or(LEARNING_RATE);
    let l2 = flag_value(&args, "--l2")
        .map(|value| value.parse::<f64>())
        .transpose()
        .map_err(|_| "--l2 must be a number")?
        .unwrap_or(L2);
    let source = std::fs::read_to_string(path)?;
    let dataset = EvaluationDataset::from_json(&source)?;
    let engine = TextIntelligence::new(EngineConfig::default());

    println!("featurizing train split...");
    let train = featurize(&engine, &dataset, "train")?;
    println!("featurizing validation split...");
    let validation = featurize(&engine, &dataset, "validation")?;
    println!(
        "train cases: {} (positives: {})",
        train.labels.len(),
        train.labels.iter().filter(|label| **label).count()
    );

    let mut weights = vec![0.0; TRAINING_FEATURES.len()];
    let mut bias = 0.0;
    let mut loss = f64::INFINITY;
    for _ in 0..iterations {
        loss = logistic_step(
            &train.features,
            &train.labels,
            &mut weights,
            &mut bias,
            learning_rate,
            l2,
        );
    }
    println!("train loss after {iterations} iterations: {loss:.4}");

    // Calibration on validation: freeze weights, fit the bias only. The
    // scratch copy absorbs the weight update and is discarded.
    for _ in 0..CALIBRATION_ITERATIONS {
        let mut scratch = weights.clone();
        logistic_step(
            &validation.features,
            &validation.labels,
            &mut scratch,
            &mut bias,
            learning_rate,
            0.0,
        );
    }
    println!("bias after validation calibration: {bias:.4}");

    let names: BTreeMap<String, f64> = TRAINING_FEATURES
        .iter()
        .zip(weights.iter())
        .map(|(name, weight)| ((*name).to_string(), *weight))
        .collect();
    let scorer = LogisticSimilarityScorer::new(names.clone(), bias);
    let mut metrics = BTreeMap::new();
    for split in ["train", "validation", "test"] {
        let split_metrics = metrics_at(&scorer, &engine, &dataset, split)?;
        println!(
            "{split}: accuracy={:.3} f1={:.3} brier={:.3} roc_auc={:.3} best_threshold={:.3}",
            split_metrics["accuracy"],
            split_metrics["f1"],
            split_metrics["brier"],
            split_metrics["roc_auc"],
            split_metrics["best_threshold"],
        );
        for (key, value) in split_metrics {
            metrics.insert(format!("{split}_{key}"), value);
        }
    }
    let artifact = SimilarityModelArtifact::new(dataset.version.clone(), names, bias)
        .with_revision(format!("train-{iterations}-iter"))
        .with_metrics(metrics);
    if let Some(parent) = std::path::Path::new(&output).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(
        &output,
        artifact.to_json().map_err(|error| error.to_string())?,
    )?;
    println!("wrote {output}");
    Ok(())
}
