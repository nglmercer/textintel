//! Calibration: temperature scaling with per-class bias, temperature
//! fitting on held-out logits, and proper scoring rules (NLL, Brier, ECE).
//!
//! Calibration always fits on validation/calibration data, never on the
//! final test split. Callers enforce that by construction (the dataset
//! keeps splits separate); these helpers just score and fit what they are
//! given.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::scoring::{softmax_with_temperature, validate_distribution};

/// Negative log-likelihood of one prediction: `-ln p_gold`, clamped to a
/// finite range so a single zero probability cannot poison an aggregate.
pub fn nll_loss(probabilities: &[f64], gold: usize) -> Result<f64, String> {
    validate_distribution(probabilities, None)?;
    if gold >= probabilities.len() {
        return Err(format!(
            "gold index {gold} out of range for {} probabilities",
            probabilities.len()
        ));
    }
    Ok(-probabilities[gold].max(1e-12).ln())
}

/// Multi-class Brier score: mean squared error against the one-hot gold
/// vector. Lower is better; 0 is perfect.
pub fn brier_score(probabilities: &[f64], gold: usize) -> Result<f64, String> {
    validate_distribution(probabilities, None)?;
    if gold >= probabilities.len() {
        return Err(format!(
            "gold index {gold} out of range for {} probabilities",
            probabilities.len()
        ));
    }
    let sum: f64 = probabilities
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let target = if index == gold { 1.0 } else { 0.0 };
            (value - target).powi(2)
        })
        .sum();
    Ok(sum / probabilities.len() as f64)
}

/// Expected calibration error over confidence bins: `Σ_b |conf_b - acc_b| *
/// n_b / n`, where confidence is the max probability. `bins` must be
/// positive; predictions carry `(probabilities, gold)` pairs.
pub fn expected_calibration_error(
    predictions: &[(Vec<f64>, usize)],
    bins: usize,
) -> Result<f64, String> {
    if bins == 0 {
        return Err("ECE needs at least one bin".to_string());
    }
    if predictions.is_empty() {
        return Ok(0.0);
    }
    let mut totals = vec![0usize; bins];
    let mut correct = vec![0usize; bins];
    let mut confidence_sum = vec![0.0; bins];
    for (probabilities, gold) in predictions {
        validate_distribution(probabilities, None)?;
        if *gold >= probabilities.len() {
            return Err(format!(
                "gold index {gold} out of range for {} probabilities",
                probabilities.len()
            ));
        }
        let (predicted, confidence) = probabilities
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(index, value)| (index, *value))
            .unwrap_or((0, 0.0));
        let mut bin = (confidence * bins as f64).floor() as usize;
        if bin >= bins {
            bin = bins - 1;
        }
        totals[bin] += 1;
        confidence_sum[bin] += confidence;
        if predicted == *gold {
            correct[bin] += 1;
        }
    }
    let total = predictions.len() as f64;
    let mut ece = 0.0;
    for bin in 0..bins {
        if totals[bin] == 0 {
            continue;
        }
        let confidence = confidence_sum[bin] / totals[bin] as f64;
        let accuracy = correct[bin] as f64 / totals[bin] as f64;
        ece += (confidence - accuracy).abs() * totals[bin] as f64 / total;
    }
    Ok(ece)
}

/// Temperature scaling: `softmax(logits / temperature)`. The temperature is
/// fit on held-out data (see [`fit_temperature`]).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct TemperatureScaling {
    pub temperature: f64,
}

impl TemperatureScaling {
    pub fn new(temperature: f64) -> Result<Self, String> {
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(format!(
                "temperature {temperature} must be finite and positive"
            ));
        }
        Ok(Self { temperature })
    }

    pub fn identity() -> Self {
        Self { temperature: 1.0 }
    }

    pub fn apply(&self, logits: &[f64]) -> Result<Vec<f64>, String> {
        softmax_with_temperature(logits, self.temperature)
    }
}

/// Temperature scaling plus a per-class bias:
/// `softmax((logits + bias) / temperature)` — equivalently
/// `softmax(logits / t + bias / t)`; the stored bias applies before the
/// temperature division so serialized values stay comparable across
/// temperatures.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TemperatureBias {
    pub temperature: f64,
    #[serde(default)]
    pub biases: BTreeMap<String, f64>,
}

impl TemperatureBias {
    pub fn new(temperature: f64, biases: BTreeMap<String, f64>) -> Result<Self, String> {
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(format!(
                "temperature {temperature} must be finite and positive"
            ));
        }
        if biases.values().any(|value| !value.is_finite()) {
            return Err("calibration biases must be finite".to_string());
        }
        Ok(Self {
            temperature,
            biases,
        })
    }

    pub fn identity() -> Self {
        Self {
            temperature: 1.0,
            biases: BTreeMap::new(),
        }
    }

    /// Calibrate a label-aligned logit map. Labels missing from `biases`
    /// get a zero bias; unknown bias entries are ignored.
    pub fn apply(&self, logits: &BTreeMap<String, f64>) -> Result<BTreeMap<String, f64>, String> {
        if logits.is_empty() {
            return Err("calibration needs at least one logit".to_string());
        }
        if logits.values().any(|value| !value.is_finite()) {
            return Err("calibration logits must be finite".to_string());
        }
        let labels: Vec<&String> = logits.keys().collect();
        let shifted: Vec<f64> = labels
            .iter()
            .map(|label| logits[*label] + self.biases.get(*label).copied().unwrap_or(0.0))
            .collect();
        let probabilities = softmax_with_temperature(&shifted, self.temperature)?;
        Ok(labels
            .into_iter()
            .zip(probabilities)
            .map(|(label, value)| (label.clone(), value))
            .collect())
    }

    /// Calibrate an order-aligned logit vector with order-aligned biases
    /// (`biases[i]` adjusts `logits[i]`; missing entries read 0.0).
    pub fn apply_ordered(&self, labels: &[String], logits: &[f64]) -> Result<Vec<f64>, String> {
        if labels.len() != logits.len() {
            return Err(format!(
                "calibration got {} labels for {} logits",
                labels.len(),
                logits.len()
            ));
        }
        let map: BTreeMap<String, f64> =
            labels.iter().cloned().zip(logits.iter().copied()).collect();
        let calibrated = self.apply(&map)?;
        Ok(labels.iter().map(|label| calibrated[label]).collect())
    }
}

/// One held-out sample for temperature fitting: raw logits plus the gold
/// class index.
#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationSample {
    pub logits: Vec<f64>,
    pub gold: usize,
}

impl CalibrationSample {
    pub fn new(logits: Vec<f64>, gold: usize) -> Result<Self, String> {
        if logits.is_empty() {
            return Err("calibration sample needs at least one logit".to_string());
        }
        if logits.iter().any(|value| !value.is_finite()) {
            return Err("calibration sample logits must be finite".to_string());
        }
        if gold >= logits.len() {
            return Err(format!(
                "gold index {gold} out of range for {} logits",
                logits.len()
            ));
        }
        Ok(Self { logits, gold })
    }
}

/// Fit result: winning temperature plus mean NLL before/after on the same
/// held-out samples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TemperatureFit {
    pub temperature: f64,
    pub nll_before: f64,
    pub nll_after: f64,
}

fn mean_nll(samples: &[CalibrationSample], temperature: f64) -> Result<f64, String> {
    let mut sum = 0.0;
    for sample in samples {
        let probabilities = softmax_with_temperature(&sample.logits, temperature)?;
        sum += nll_loss(&probabilities, sample.gold)?;
    }
    Ok(sum / samples.len().max(1) as f64)
}

/// Fit the temperature by grid search over a log-spaced grid
/// (`0.05..=5.0`, 60 points), minimizing mean NLL on held-out samples.
/// Deterministic: fixed grid, ties keep the smaller temperature.
pub fn fit_temperature(samples: &[CalibrationSample]) -> Result<TemperatureFit, String> {
    if samples.is_empty() {
        return Err("temperature fitting needs at least one sample".to_string());
    }
    let nll_before = mean_nll(samples, 1.0)?;
    let mut best_temperature = 1.0;
    let mut best_nll = nll_before;
    for index in 0..60 {
        // Log-spaced from ln(0.05) to ln(5.0).
        let temperature = (0.05f64.ln() + (5.0f64.ln() - 0.05f64.ln()) * index as f64 / 59.0).exp();
        let nll = mean_nll(samples, temperature)?;
        if nll < best_nll {
            best_nll = nll;
            best_temperature = temperature;
        }
    }
    Ok(TemperatureFit {
        temperature: best_temperature,
        nll_before,
        nll_after: best_nll,
    })
}

/// Task-specific calibration record: temperature plus the accept threshold
/// for selective classification. One universal threshold is usually wrong;
/// each task stores its own.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskCalibration {
    pub task: String,
    pub temperature: f64,
    pub accept_threshold: f64,
}

impl TaskCalibration {
    pub fn new(
        task: impl Into<String>,
        temperature: f64,
        accept_threshold: f64,
    ) -> Result<Self, String> {
        let task = task.into();
        if task.trim().is_empty() {
            return Err("task calibration needs a non-empty task name".to_string());
        }
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(format!(
                "task temperature {temperature} must be finite and positive"
            ));
        }
        if !accept_threshold.is_finite() || !(0.0..=1.0).contains(&accept_threshold) {
            return Err(format!(
                "accept threshold {accept_threshold} must be finite and within [0.0, 1.0]"
            ));
        }
        Ok(Self {
            task,
            temperature,
            accept_threshold,
        })
    }
}
