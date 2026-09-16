//! Logistic similarity-scorer training on the `train` split.

use std::collections::BTreeMap;

use textintel::evaluation::EvaluationDataset;
use textintel::semantic::FeatureHashEmbeddingProvider;
use textintel::SimilarityScorer;
use textintel::{
    balanced_sample_weights, language_agreement, logistic_step_weighted, training_features,
    EngineConfig, LogisticSimilarityScorer, SimilarityModelArtifact, TextIntelligence,
    TRAINING_FEATURES,
};

use super::flag_value;
use super::metrics::report;

const ITERATIONS: usize = 20000;
const CALIBRATION_ITERATIONS: usize = 500;
const LEARNING_RATE: f64 = 0.2;
const L2: f64 = 1e-3;

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
    let mut semantic_present = 0usize;
    let mut phonetic_present = 0usize;
    for case in cases {
        let left = engine.analyze(&case.a)?;
        let right = engine.analyze(&case.b)?;
        let comparison = engine.compare_fingerprints(&left, &right);
        semantic_present += usize::from(comparison.semantic.is_some());
        phonetic_present += usize::from(comparison.phonetic.is_some());
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
    println!(
        "{split}: semantic present in {semantic_present}/{} pairs, phonetic in {phonetic_present}/{}",
        features.len(),
        features.len()
    );
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

pub(crate) fn run_similarity(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = args.get(1).map(String::as_str).unwrap_or("data/evaluation");
    let output = flag_value(args, "--output")
        .ok_or("similarity training requires --output <artifact.json>")?;
    let iterations = flag_value(args, "--iterations")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| "--iterations must be a positive integer")?
        .unwrap_or(ITERATIONS)
        .max(1);
    let learning_rate = flag_value(args, "--learning-rate")
        .map(|value| value.parse::<f64>())
        .transpose()
        .map_err(|_| "--learning-rate must be a number")?
        .unwrap_or(LEARNING_RATE);
    let l2 = flag_value(args, "--l2")
        .map(|value| value.parse::<f64>())
        .transpose()
        .map_err(|_| "--l2 must be a number")?
        .unwrap_or(L2);
    let dataset = EvaluationDataset::load_path(path).map_err(|error| error.to_string())?;
    // Production-like featurization: semantic and phonetic evidence must be
    // present or their weights train to exactly zero (a default engine would
    // starve both channels and ship a misleading artifact).
    let engine = TextIntelligence::new(EngineConfig {
        semantic: true,
        phonetic: true,
        ..EngineConfig::default()
    })
    .with_embedding_provider(
        FeatureHashEmbeddingProvider::new(256).map_err(|error| error.to_string())?,
    );

    println!("featurizing train split...");
    let train = featurize(&engine, &dataset, "train")?;
    println!("featurizing validation split...");
    let validation = featurize(&engine, &dataset, "validation")?;
    println!(
        "train cases: {} (positives: {})",
        train.labels.len(),
        train.labels.iter().filter(|label| **label).count()
    );

    // Class-balanced loss: the training split is ~85% positive while held-out
    // slices are closer to balanced, so uniform weighting would drag the
    // operating point toward always-positive (recall ~0.98, poor precision).
    let train_sample_weights = balanced_sample_weights(&train.labels);
    let validation_sample_weights = balanced_sample_weights(&validation.labels);
    let mut weights = vec![0.0; TRAINING_FEATURES.len()];
    let mut bias = 0.0;
    let mut loss = f64::INFINITY;
    for _ in 0..iterations {
        loss = logistic_step_weighted(
            &train.features,
            &train.labels,
            &mut weights,
            &mut bias,
            learning_rate,
            l2,
            &train_sample_weights,
        );
    }
    println!("balanced train loss after {iterations} iterations: {loss:.4}");

    // Calibration on validation: freeze weights, fit the bias only. The
    // scratch copy absorbs the weight update and is discarded.
    for _ in 0..CALIBRATION_ITERATIONS {
        let mut scratch = weights.clone();
        logistic_step_weighted(
            &validation.features,
            &validation.labels,
            &mut scratch,
            &mut bias,
            learning_rate,
            0.0,
            &validation_sample_weights,
        );
    }
    println!("bias after validation calibration: {bias:.4}");

    let names: BTreeMap<String, f64> = TRAINING_FEATURES
        .iter()
        .zip(weights.iter())
        .map(|(name, weight)| ((*name).to_string(), *weight))
        .collect();
    println!("weights:");
    for name in TRAINING_FEATURES {
        println!("  {name} = {:.4}", names.get(*name).copied().unwrap_or(0.0));
    }
    let scorer = LogisticSimilarityScorer::new(names.clone(), bias);
    let mut metrics = BTreeMap::new();
    for split in ["train", "validation", "test"] {
        let split_metrics = metrics_at(&scorer, &engine, &dataset, split)?;
        println!(
            "{split}: accuracy={:.3} f1={:.3} brier={:.3} roc_auc={:.3} pr_auc={:.3} ece={:.3} best_threshold={:.3}",
            split_metrics["accuracy"],
            split_metrics["f1"],
            split_metrics["brier"],
            split_metrics["roc_auc"],
            split_metrics["pr_auc"],
            split_metrics["ece"],
            split_metrics["best_threshold"],
        );
        for (key, value) in split_metrics {
            metrics.insert(format!("{split}_{key}"), value);
        }
    }
    let artifact = SimilarityModelArtifact::new(dataset.version.clone(), names, bias)
        .with_revision(format!("train-{iterations}-iter-ds{}", dataset.version))
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
