//! Candidate scoring math: softmax, expected scores, distribution
//! diagnostics (entropy, margin), and energy-based out-of-distribution
//! scores. Pure functions over caller-supplied logits — no model access.

use super::types::PROBABILITY_SUM_TOLERANCE;

/// Numerically stable softmax. Rejects empty and non-finite inputs; the
/// output is finite, within `[0, 1]`, and sums to 1.
pub fn softmax(logits: &[f64]) -> Result<Vec<f64>, String> {
    softmax_with_temperature(logits, 1.0)
}

/// Softmax with temperature scaling: `softmax(logits / temperature)`.
/// Temperature must be finite and positive.
pub fn softmax_with_temperature(logits: &[f64], temperature: f64) -> Result<Vec<f64>, String> {
    if logits.is_empty() {
        return Err("softmax needs at least one logit".to_string());
    }
    if !temperature.is_finite() || temperature <= 0.0 {
        return Err(format!(
            "softmax temperature {temperature} must be finite and positive"
        ));
    }
    if logits.iter().any(|value| !value.is_finite()) {
        return Err("softmax logits must be finite".to_string());
    }
    let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let scaled: Vec<f64> = logits
        .iter()
        .map(|value| (value - max) / temperature)
        .collect();
    // Underflow guard: when every logit is far below the max (extreme
    // temperature), `exp` rounds all terms to 0. Fall back to uniform so
    // scoring stays total instead of dividing by zero.
    let mut sum: f64 = scaled.iter().map(|value| value.exp()).sum();
    if sum == 0.0 {
        let uniform = 1.0 / logits.len() as f64;
        return Ok(vec![uniform; logits.len()]);
    }
    if !sum.is_finite() {
        sum = f64::MAX;
    }
    Ok(scaled.iter().map(|value| value.exp() / sum).collect())
}

/// Expected ordered score `Σ p_i * i` for a validated distribution.
pub fn expected_score(probabilities: &[f64]) -> Result<f64, String> {
    validate_distribution(probabilities, None)?;
    Ok(probabilities
        .iter()
        .enumerate()
        .map(|(index, value)| index as f64 * value)
        .sum())
}

/// Shannon entropy in nats. High entropy means a flat, uncertain
/// distribution; low entropy means a peaked one. Validated input only.
pub fn entropy(probabilities: &[f64]) -> Result<f64, String> {
    validate_distribution(probabilities, None)?;
    Ok(-probabilities
        .iter()
        .filter(|value| **value > 0.0)
        .map(|value| value * value.ln())
        .sum::<f64>())
}

/// Top-1 / top-2 probability gap. A small margin means the winner barely
/// beat its closest alternative — a useful escalation signal alongside raw
/// confidence.
pub fn margin(probabilities: &[f64]) -> Result<f64, String> {
    validate_distribution(probabilities, None)?;
    let mut sorted = probabilities.to_vec();
    sorted.sort_by(|left, right| right.total_cmp(left));
    match sorted.as_slice() {
        [] => Err("margin needs at least one probability".to_string()),
        [only] => Ok(*only),
        [first, second, ..] => Ok(first - second),
    }
}

/// Maximum probability of a validated distribution.
pub fn max_probability(probabilities: &[f64]) -> Result<f64, String> {
    validate_distribution(probabilities, None)?;
    Ok(probabilities.iter().copied().fold(0.0, f64::max))
}

/// Free-energy OOD score `-t * log Σ exp(logit_i / t)`: lower energy means
/// the input looks more in-distribution. Computed from raw logits (before
/// softmax), since softmax discards the magnitude information energy needs.
pub fn energy_score(logits: &[f64], temperature: f64) -> Result<f64, String> {
    if logits.is_empty() {
        return Err("energy score needs at least one logit".to_string());
    }
    if !temperature.is_finite() || temperature <= 0.0 {
        return Err(format!(
            "energy temperature {temperature} must be finite and positive"
        ));
    }
    if logits.iter().any(|value| !value.is_finite()) {
        return Err("energy logits must be finite".to_string());
    }
    let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let sum: f64 = logits
        .iter()
        .map(|value| ((value - max) / temperature).exp())
        .sum();
    Ok(-temperature * (max / temperature + sum.ln()))
}

/// Energy-threshold OOD verdict: `energy > threshold` reads as
/// out-of-distribution. Thresholds are fit on held-out data, never on test.
pub fn is_ood_by_energy(logits: &[f64], temperature: f64, threshold: f64) -> Result<bool, String> {
    if !threshold.is_finite() {
        return Err("energy threshold must be finite".to_string());
    }
    Ok(energy_score(logits, temperature)? > threshold)
}

/// Validate a probability distribution: optionally the expected length,
/// finite values within `[0, 1]`, summing to ≈ 1.
pub fn validate_distribution(
    probabilities: &[f64],
    expected_len: Option<usize>,
) -> Result<(), String> {
    if let Some(expected) = expected_len
        && probabilities.len() != expected
    {
        return Err(format!(
            "distribution has {} values, expected {expected}",
            probabilities.len()
        ));
    }
    if probabilities.is_empty() {
        return Err("distribution must not be empty".to_string());
    }
    if probabilities.iter().any(|value| !value.is_finite()) {
        return Err("distribution values must be finite".to_string());
    }
    if probabilities
        .iter()
        .any(|value| !(0.0..=1.0).contains(value))
    {
        return Err("distribution values must be within [0.0, 1.0]".to_string());
    }
    let sum: f64 = probabilities.iter().sum();
    if (sum - 1.0).abs() > PROBABILITY_SUM_TOLERANCE {
        return Err(format!("distribution sums to {sum}, expected ≈ 1.0"));
    }
    Ok(())
}
