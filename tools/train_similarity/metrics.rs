//! Shared ranking/calibration metrics for both trainers.

use std::collections::BTreeMap;

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

pub(crate) fn report(samples: &[(f64, bool)]) -> BTreeMap<String, f64> {
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
    report.insert("pr_auc".to_string(), pr_auc(samples));
    report.insert("ece".to_string(), expected_calibration_error(samples));
    report.insert("best_threshold".to_string(), best_threshold);
    report
}

fn pr_auc(samples: &[(f64, bool)]) -> f64 {
    let positives = samples.iter().filter(|(_, label)| *label).count();
    if positives == 0 || samples.is_empty() {
        return 0.0;
    }
    let mut ranked = samples.to_vec();
    ranked.sort_by(|left, right| right.0.total_cmp(&left.0));
    let mut area = 0.0;
    let mut hits = 0;
    let mut previous_recall = 0.0;
    for (index, (_, label)) in ranked.iter().enumerate() {
        let seen = index + 1;
        if *label {
            hits += 1;
        }
        let precision = hits as f64 / seen as f64;
        let recall = hits as f64 / positives as f64;
        area += precision * (recall - previous_recall);
        previous_recall = recall;
    }
    area.clamp(0.0, 1.0)
}

fn expected_calibration_error(samples: &[(f64, bool)]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let bins = 10;
    let mut totals = vec![0u32; bins];
    let mut hits = vec![0u32; bins];
    let mut confidence = vec![0.0; bins];
    for (score, label) in samples {
        let bin = ((score.clamp(0.0, 1.0) * bins as f64) as usize).min(bins - 1);
        totals[bin] += 1;
        confidence[bin] += score.clamp(0.0, 1.0);
        if *label {
            hits[bin] += 1;
        }
    }
    let mut error = 0.0;
    for bin in 0..bins {
        if totals[bin] == 0 {
            continue;
        }
        let accuracy = hits[bin] as f64 / totals[bin] as f64;
        let mean_confidence = confidence[bin] / totals[bin] as f64;
        error += totals[bin] as f64 / samples.len() as f64 * (accuracy - mean_confidence).abs();
    }
    error.clamp(0.0, 1.0)
}

/// Accuracy/F1/Brier/ROC for a fixed weight vector (shared by the similarity
/// and spam trainers).
pub(crate) fn logistic_report(
    weights: &[f64],
    bias: f64,
    features: &[Vec<f64>],
    labels: &[bool],
) -> BTreeMap<String, f64> {
    use textintel::sigmoid;

    let samples: Vec<(f64, bool)> = features
        .iter()
        .zip(labels.iter())
        .map(|(row, label)| {
            let logit = row
                .iter()
                .zip(weights.iter())
                .map(|(value, weight)| value * weight)
                .sum::<f64>()
                + bias;
            (sigmoid(logit), *label)
        })
        .collect();
    report(&samples)
}
