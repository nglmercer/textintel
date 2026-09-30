//! Deterministic logistic optimization with reusable batch workspaces.

pub fn sigmoid(logit: f64) -> f64 {
    (1.0 / (1.0 + (-logit.clamp(-60.0, 60.0)).exp())).clamp(0.0, 1.0)
}

/// A fixed training batch with sanitized sample weights and reusable gradient
/// storage. Construct once and reuse across epochs to avoid per-step allocations.
/// Sample order and floating-point reductions match the one-shot helpers.
pub struct LogisticTrainingBatch<'a> {
    features: &'a [Vec<f64>],
    labels: &'a [bool],
    sample_weights: Vec<f64>,
    total: f64,
    gradient: Vec<f64>,
}

impl<'a> LogisticTrainingBatch<'a> {
    pub fn new(features: &'a [Vec<f64>], labels: &'a [bool]) -> Self {
        Self::weighted(features, labels, &vec![1.0; features.len()])
    }

    pub fn weighted(features: &'a [Vec<f64>], labels: &'a [bool], sample_weights: &[f64]) -> Self {
        let sample_weights: Vec<f64> = sample_weights
            .iter()
            .map(|value| {
                if value.is_finite() && *value > 0.0 {
                    *value
                } else {
                    0.0
                }
            })
            .collect();
        let total = sample_weights.iter().sum();
        Self {
            features,
            labels,
            sample_weights,
            total,
            gradient: Vec::new(),
        }
    }

    /// One full-batch L2-regularized gradient step, returning mean loss.
    /// Non-finite/negative sample weights are zero; an all-zero batch is a no-op.
    pub fn step(
        &mut self,
        weights: &mut [f64],
        bias: &mut f64,
        learning_rate: f64,
        l2: f64,
    ) -> f64 {
        if self.total <= 0.0 {
            return 0.0;
        }
        self.gradient.resize(weights.len(), 0.0);
        self.gradient.fill(0.0);
        let mut loss = 0.0;
        let mut bias_gradient = 0.0;
        for ((row, label), weight) in self
            .features
            .iter()
            .zip(self.labels)
            .zip(&self.sample_weights)
        {
            let logit = row
                .iter()
                .zip(weights.iter())
                .map(|(value, weight)| value * weight)
                .sum::<f64>()
                + *bias;
            let predicted = sigmoid(logit).clamp(1e-12, 1.0 - 1e-12);
            let target = if *label { 1.0 } else { 0.0 };
            loss += weight * -(target * predicted.ln() + (1.0 - target) * (1.0 - predicted).ln());
            let error = weight * (predicted - target);
            for (index, value) in row.iter().enumerate() {
                self.gradient[index] += error * value;
            }
            bias_gradient += error;
        }
        for (index, weight) in weights.iter_mut().enumerate() {
            *weight -= learning_rate * (self.gradient[index] / self.total + l2 * *weight);
        }
        *bias -= learning_rate * bias_gradient / self.total;
        loss / self.total
    }

    /// Fit only the intercept with frozen feature weights. Dot products are
    /// computed once; no discarded weight updates or loss logs per iteration.
    pub fn calibrate_bias(
        &self,
        weights: &[f64],
        bias: &mut f64,
        learning_rate: f64,
        iterations: usize,
    ) {
        if self.total <= 0.0 {
            return;
        }
        let logits: Vec<f64> = self
            .features
            .iter()
            .map(|row| {
                row.iter()
                    .zip(weights)
                    .map(|(value, weight)| value * weight)
                    .sum()
            })
            .collect();
        for _ in 0..iterations {
            let mut gradient = 0.0;
            for ((logit, label), weight) in logits.iter().zip(self.labels).zip(&self.sample_weights)
            {
                let predicted = sigmoid(logit + *bias).clamp(1e-12, 1.0 - 1e-12);
                let target = if *label { 1.0 } else { 0.0 };
                gradient += weight * (predicted - target);
            }
            *bias -= learning_rate * gradient / self.total;
        }
    }
}

/// One-shot uniform full-batch step. Reuse [`LogisticTrainingBatch`] for training loops.
pub fn logistic_step(
    features: &[Vec<f64>],
    labels: &[bool],
    weights: &mut [f64],
    bias: &mut f64,
    learning_rate: f64,
    l2: f64,
) -> f64 {
    LogisticTrainingBatch::new(features, labels).step(weights, bias, learning_rate, l2)
}

/// One-shot weighted step. Invalid sample weights are treated as zero.
pub fn logistic_step_weighted(
    features: &[Vec<f64>],
    labels: &[bool],
    weights: &mut [f64],
    bias: &mut f64,
    learning_rate: f64,
    l2: f64,
    sample_weights: &[f64],
) -> f64 {
    LogisticTrainingBatch::weighted(features, labels, sample_weights).step(
        weights,
        bias,
        learning_rate,
        l2,
    )
}

/// Balanced sample weights: positives and negatives each carry half the total
/// mass, so a skewed training split cannot drag the operating point. Either
/// class may be empty (all mass goes to the other side).
pub fn balanced_sample_weights(labels: &[bool]) -> Vec<f64> {
    let positive_count = labels.iter().filter(|label| **label).count();
    let negative_count = labels.len().saturating_sub(positive_count);
    let positives = positive_count.max(1) as f64;
    let negatives = negative_count.max(1) as f64;
    labels
        .iter()
        .map(|label| {
            if *label {
                0.5 / positives
            } else {
                0.5 / negatives
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_step(
        features: &[Vec<f64>],
        labels: &[bool],
        weights: &mut [f64],
        bias: &mut f64,
        learning_rate: f64,
        l2: f64,
        sample_weights: &[f64],
    ) -> f64 {
        let clean: Vec<f64> = sample_weights
            .iter()
            .map(|value| {
                if value.is_finite() && *value > 0.0 {
                    *value
                } else {
                    0.0
                }
            })
            .collect();
        let total: f64 = clean.iter().sum();
        if total <= 0.0 {
            return 0.0;
        }
        let mut loss = 0.0;
        let mut gradient = vec![0.0; weights.len()];
        let mut bias_gradient = 0.0;
        for ((row, label), weight) in features.iter().zip(labels.iter()).zip(clean.iter()) {
            let logit = row
                .iter()
                .zip(weights.iter())
                .map(|(value, weight)| value * weight)
                .sum::<f64>()
                + *bias;
            let predicted = sigmoid(logit).clamp(1e-12, 1.0 - 1e-12);
            let target = if *label { 1.0 } else { 0.0 };
            loss += weight * -(target * predicted.ln() + (1.0 - target) * (1.0 - predicted).ln());
            let error = weight * (predicted - target);
            for (index, value) in row.iter().enumerate() {
                gradient[index] += error * value;
            }
            bias_gradient += error;
        }
        for (index, weight) in weights.iter_mut().enumerate() {
            *weight -= learning_rate * (gradient[index] / total + l2 * *weight);
        }
        *bias -= learning_rate * bias_gradient / total;
        loss / total
    }

    #[test]
    fn reusable_batch_matches_original_reductions_and_bias_calibration() {
        let features = vec![
            vec![0.9, 0.1],
            vec![0.8, 0.2],
            vec![0.2, 0.8],
            vec![0.1, 0.9],
        ];
        let labels = vec![true, false, true, false];
        for sample_weights in [
            vec![1.0; 4],
            vec![0.5, 0.25, 0.125, 0.125],
            vec![f64::NAN, -1.0, 0.5, f64::INFINITY],
            vec![0.0; 4],
        ] {
            let (mut expected_weights, mut actual_weights) = (vec![0.1, -0.2], vec![0.1, -0.2]);
            let (mut expected_bias, mut actual_bias) = (0.3, 0.3);
            let mut batch = LogisticTrainingBatch::weighted(&features, &labels, &sample_weights);
            for _ in 0..100 {
                let expected_loss = reference_step(
                    &features,
                    &labels,
                    &mut expected_weights,
                    &mut expected_bias,
                    0.2,
                    1e-3,
                    &sample_weights,
                );
                let actual_loss = batch.step(&mut actual_weights, &mut actual_bias, 0.2, 1e-3);
                assert_eq!(actual_loss, expected_loss);
                assert_eq!(actual_weights, expected_weights);
                assert_eq!(actual_bias, expected_bias);
            }
            for _ in 0..50 {
                let mut scratch = expected_weights.clone();
                reference_step(
                    &features,
                    &labels,
                    &mut scratch,
                    &mut expected_bias,
                    0.2,
                    0.0,
                    &sample_weights,
                );
            }
            batch.calibrate_bias(&actual_weights, &mut actual_bias, 0.2, 50);
            assert_eq!(actual_bias, expected_bias);
        }
    }
    #[test]
    fn gradient_steps_reduce_separable_loss() {
        // Two clusters on one feature: repeated steps must drive loss down
        // and orient the weight positively.
        let features = vec![
            vec![0.9, 0.1],
            vec![0.8, 0.2],
            vec![0.2, 0.8],
            vec![0.1, 0.9],
        ];
        let labels = vec![true, true, false, false];
        let mut weights = vec![0.0, 0.0];
        let mut bias = 0.0;
        let mut previous = f64::INFINITY;
        for _ in 0..200 {
            let loss = logistic_step(&features, &labels, &mut weights, &mut bias, 0.5, 1e-4);
            assert!(loss <= previous + 1e-12, "loss must not increase");
            previous = loss;
        }
        assert!(previous < 0.5);
        assert!(weights[0] > 0.0);
        assert!(weights[1] < 0.0);
    }

    #[test]
    fn weighted_step_matches_uniform_and_balances_classes() {
        let features = vec![vec![1.0], vec![1.0], vec![0.0], vec![0.0]];
        let labels = vec![true, true, false, false];
        let mut plain_weights = vec![0.0];
        let mut plain_bias = 0.0;
        let plain_loss = logistic_step(
            &features,
            &labels,
            &mut plain_weights,
            &mut plain_bias,
            0.5,
            0.0,
        );
        let mut weighted_weights = vec![0.0];
        let mut weighted_bias = 0.0;
        let weighted_loss = logistic_step_weighted(
            &features,
            &labels,
            &mut weighted_weights,
            &mut weighted_bias,
            0.5,
            0.0,
            &[1.0, 1.0, 1.0, 1.0],
        );
        assert_eq!(plain_loss, weighted_loss);
        assert_eq!(plain_weights, weighted_weights);
        assert_eq!(plain_bias, weighted_bias);

        // Nine negatives against one positive: balanced weights give the lone
        // positive half the gradient mass instead of one tenth.
        let skewed = vec![
            true, false, false, false, false, false, false, false, false, false,
        ];
        let balanced = balanced_sample_weights(&skewed);
        assert_eq!(balanced[0], 0.5);
        assert!((balanced[1..].iter().sum::<f64>() - 0.5).abs() < 1e-12);

        // Hostile weights degrade to zero instead of NaN.
        let mut untouched_weights = vec![0.25];
        let mut untouched_bias = 0.5;
        let loss = logistic_step_weighted(
            &features,
            &labels,
            &mut untouched_weights,
            &mut untouched_bias,
            0.5,
            0.0,
            &[f64::NAN, f64::INFINITY, -1.0, 0.0],
        );
        assert_eq!(loss, 0.0);
        assert_eq!(untouched_weights, vec![0.25]);
        assert_eq!(untouched_bias, 0.5);
    }
}
