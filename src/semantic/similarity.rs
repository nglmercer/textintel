pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let dot: f64 = a
        .iter()
        .zip(b)
        .map(|(left, right)| f64::from(*left) * f64::from(*right))
        .sum();
    let left_norm = a
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt();
    let right_norm = b
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        return 0.0;
    }
    let similarity = dot / (left_norm * right_norm);
    // Total function: hostile vectors (NaN/Inf) score 0 instead of
    // poisoning downstream scoring with NaN.
    if similarity.is_finite() {
        similarity
    } else {
        0.0
    }
}
