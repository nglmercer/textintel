//! Logistic similarity-scorer training on the `train` split.

use std::collections::BTreeMap;

use textintel::SimilarityScorer;
use textintel::evaluation::EvaluationDataset;
use textintel::semantic::FeatureHashEmbeddingProvider;
use textintel::{
    EngineConfig, LogisticSimilarityScorer, SimilarityModelArtifact, TRAINING_FEATURES,
    TextIntelligence, balanced_sample_weights, language_agreement, logistic_step_weighted,
    training_features,
};

use super::metrics::report;
use textintel::cli::ParsedArgs;

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

/// Production semantic backend for training featurization: the configured
/// local transformer (`--transformer-model` or `TEXTINTEL_TRANSFORMER_MODEL`)
/// with the feature-hash fallback armed, or the feature-hash baseline when
/// no transformer is configured or loadable. Mirrors the production
/// preference order so training never featurizes on a weaker backend than
/// serving evaluates. Returns the engine plus a backend label recorded in
/// the artifact's training config.
fn training_engine(
    transformer_model: Option<&str>,
) -> Result<(TextIntelligence, String), Box<dyn std::error::Error>> {
    let config = EngineConfig {
        semantic: true,
        phonetic: true,
        ..EngineConfig::default()
    };
    #[cfg(feature = "semantic-transformer")]
    {
        if let Some(directory) = transformer_model {
            match textintel::semantic::TransformerEmbeddingProvider::open(directory) {
                Ok(provider) => {
                    let label = format!(
                        "transformer:{} (feature-hash fallback armed)",
                        provider.model_id()
                    );
                    println!("training embeddings: local transformer at {directory}");
                    let engine = TextIntelligence::new(config).with_embedding_provider(
                        textintel::semantic::FallbackEmbeddingProvider::new(
                            provider,
                            FeatureHashEmbeddingProvider::new(256)
                                .map_err(|error| error.to_string())?,
                        ),
                    );
                    return Ok((engine, label));
                }
                Err(error) => {
                    println!(
                        "training embeddings: configured transformer at {directory} unavailable \
                         ({error}); featurizing with the feature-hash fallback"
                    );
                    let engine = TextIntelligence::new(config).with_embedding_provider(
                        FeatureHashEmbeddingProvider::new(256)
                            .map_err(|error| error.to_string())?,
                    );
                    return Ok((
                        engine,
                        format!("feature-hash-v1 (configured transformer failed: {error})"),
                    ));
                }
            }
        }
    }
    #[cfg(not(feature = "semantic-transformer"))]
    if let Some(directory) = transformer_model {
        println!(
            "training embeddings: transformer configured at {directory} but this build lacks \
             the semantic-transformer feature; featurizing with the feature-hash fallback"
        );
    }
    if transformer_model.is_none() {
        println!("training embeddings: no transformer configured; featurizing with feature-hash");
    }
    let engine = TextIntelligence::new(config).with_embedding_provider(
        FeatureHashEmbeddingProvider::new(256).map_err(|error| error.to_string())?,
    );
    Ok((engine, "feature-hash-v1".to_string()))
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

pub(crate) fn run_similarity(parsed: &ParsedArgs) -> Result<(), Box<dyn std::error::Error>> {
    let path = parsed.positional(0).unwrap_or("data/evaluation");
    let output = parsed.required_value("output")?;
    let iterations = parsed
        .value("iterations")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| "--iterations must be a positive integer")?
        .unwrap_or(ITERATIONS)
        .max(1);
    let learning_rate = parsed
        .value("learning-rate")
        .map(|value| value.parse::<f64>())
        .transpose()
        .map_err(|_| "--learning-rate must be a number")?
        .unwrap_or(LEARNING_RATE);
    let l2 = parsed
        .value("l2")
        .map(|value| value.parse::<f64>())
        .transpose()
        .map_err(|_| "--l2 must be a number")?
        .unwrap_or(L2);
    let dataset = EvaluationDataset::load_path(path).map_err(|error| error.to_string())?;
    // Production-like featurization: semantic and phonetic evidence must be
    // present or their weights train to exactly zero (a default engine would
    // starve both channels and ship a misleading artifact). The semantic
    // backend follows the production preference order (configured local
    // transformer, feature-hash fallback).
    let transformer_model = parsed
        .value("transformer-model")
        .map(str::to_string)
        .or_else(|| std::env::var("TEXTINTEL_TRANSFORMER_MODEL").ok());
    let (engine, embedding_backend) = training_engine(transformer_model.as_deref())?;

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

    // NOTE: no F1 operating-point tuning here. The validation-optimal
    // boundary (best threshold ~0.47) does not transfer to test (~0.60):
    // sliding the bias to the validation F1 peak raised validation F1 to
    // 0.946 but dropped test F1 to 0.911 — a pure operating-point overfit.
    // The NLL-calibrated bias above is the honest boundary.
    let validation_logits: Vec<f64> = validation
        .features
        .iter()
        .map(|row| {
            row.iter()
                .zip(weights.iter())
                .map(|(value, weight)| value * weight)
                .sum::<f64>()
                + bias
        })
        .collect();

    // Temperature scaling on validation (Guo et al.): a single temperature
    // fit by NLL, folded into the weights so the artifact format is
    // unchanged. Rescaling preserves ranking and every 0.5 decision (hence
    // F1); it only repairs probability calibration (Brier/ECE).
    let nll_at = |temperature: f64| {
        validation_logits
            .iter()
            .zip(validation.labels.iter())
            .map(|(logit, label)| {
                let predicted = textintel::sigmoid(logit / temperature).clamp(1e-12, 1.0 - 1e-12);
                let target = if *label { 1.0 } else { 0.0 };
                -(target * predicted.ln() + (1.0 - target) * (1.0 - predicted).ln())
            })
            .sum::<f64>()
            / validation_logits.len().max(1) as f64
    };
    let mut temperatures: Vec<f64> = (0..=135).map(|step| 0.30 + step as f64 * 0.02).collect();
    temperatures.sort_by(|left, right| {
        (left - 1.0)
            .abs()
            .total_cmp(&(right - 1.0).abs())
            .then_with(|| left.total_cmp(right))
    });
    let mut temperature = 1.0;
    let mut best_nll = nll_at(1.0);
    for candidate in temperatures {
        let value = nll_at(candidate);
        if value < best_nll - 1e-12 {
            best_nll = value;
            temperature = candidate;
        }
    }
    for weight in weights.iter_mut() {
        *weight /= temperature;
    }
    bias /= temperature;
    println!("temperature from validation NLL: {temperature:.2} (validation nll {best_nll:.4})");

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
    metrics.insert("calibration_temperature".to_string(), temperature);
    metrics.insert("train_l2".to_string(), l2);
    metrics.insert("train_learning_rate".to_string(), learning_rate);
    metrics.insert("train_iterations".to_string(), iterations as f64);
    let mut training_config = BTreeMap::new();
    training_config.insert("iterations".to_string(), iterations.to_string());
    training_config.insert("learning_rate".to_string(), learning_rate.to_string());
    training_config.insert("l2".to_string(), l2.to_string());
    training_config.insert("sample_weighting".to_string(), "balanced".to_string());
    training_config.insert("embedding_backend".to_string(), embedding_backend);
    training_config.insert(
        "feature_schema_version".to_string(),
        textintel::TRAINING_FEATURE_SCHEMA_VERSION.to_string(),
    );
    training_config.insert(
        "train_split".to_string(),
        format!("train (n={})", train.labels.len()),
    );
    let mut calibration_config = BTreeMap::new();
    calibration_config.insert(
        "bias_iterations".to_string(),
        CALIBRATION_ITERATIONS.to_string(),
    );
    calibration_config.insert("temperature".to_string(), temperature.to_string());
    calibration_config.insert(
        "method".to_string(),
        "validation_nll_bias_plus_temperature_scaling".to_string(),
    );
    calibration_config.insert(
        "validation_split".to_string(),
        format!("validation (n={})", validation.labels.len()),
    );
    let mut artifact = SimilarityModelArtifact::new(dataset.version.clone(), names, bias)
        .with_revision(format!(
            "train-{iterations}-iter-lr{learning_rate}-l2{l2}-ds{}",
            dataset.version
        ))
        .with_metrics(metrics)
        .with_training_config(training_config)
        .with_calibration_config(calibration_config);
    if let Some(model) = engine.diagnostics().embedding_model {
        artifact = artifact.with_embedding_model(model);
    }
    if let Some(parent) = std::path::Path::new(&output).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        output,
        artifact.to_json().map_err(|error| error.to_string())?,
    )?;
    println!("wrote {output}");
    Ok(())
}
