//! Scoring math: softmax invariants, expected scores, entropy/margin,
//! and energy-based OOD scores.

use textintel::decision::{
    energy_score, entropy, expected_score, is_ood_by_energy, margin, max_probability, softmax,
    softmax_with_temperature, validate_distribution,
};

fn assert_distribution(values: &[f64]) {
    assert!(values.iter().all(|value| value.is_finite()));
    assert!(values.iter().all(|value| (0.0..=1.0).contains(value)));
    let sum: f64 = values.iter().sum();
    assert!((sum - 1.0).abs() < 1e-9, "softmax sums to {sum}");
}

#[test]
fn softmax_is_a_valid_distribution() {
    assert_distribution(&softmax(&[2.0, 1.0, 0.1]).expect("softmax"));
    assert_distribution(&softmax(&[1000.0, 999.0, 998.0]).expect("large logits"));
    assert_distribution(&softmax(&[-1000.0, -1001.0]).expect("negative logits"));
    assert_distribution(&softmax(&[0.0]).expect("single logit"));
    // Argmax preserved.
    let probabilities = softmax(&[1.0, 5.0, 2.0]).expect("softmax");
    assert!(probabilities[1] > probabilities[0] && probabilities[1] > probabilities[2]);
}

#[test]
fn softmax_rejects_invalid_inputs() {
    assert!(softmax(&[]).is_err());
    assert!(softmax(&[1.0, f64::NAN]).is_err());
    assert!(softmax(&[1.0, f64::INFINITY]).is_err());
    assert!(softmax_with_temperature(&[1.0, 2.0], 0.0).is_err());
    assert!(softmax_with_temperature(&[1.0, 2.0], -1.0).is_err());
    assert!(softmax_with_temperature(&[1.0, 2.0], f64::NAN).is_err());
}

#[test]
fn temperature_controls_sharpness() {
    let cold = softmax_with_temperature(&[2.0, 1.0], 0.1).expect("cold");
    let hot = softmax_with_temperature(&[2.0, 1.0], 10.0).expect("hot");
    assert!(cold[0] > 0.99, "cold softmax is peaked");
    assert!((hot[0] - 0.5).abs() < 0.05, "hot softmax is flat");
}

#[test]
fn expected_score_weights_by_index() {
    assert!((expected_score(&[1.0, 0.0, 0.0]).expect("score") - 0.0).abs() < 1e-12);
    assert!((expected_score(&[0.0, 0.0, 1.0]).expect("score") - 2.0).abs() < 1e-12);
    assert!((expected_score(&[0.5, 0.5]).expect("score") - 0.5).abs() < 1e-12);
    assert!(expected_score(&[0.5, 0.6]).is_err());
}

#[test]
fn entropy_and_margin_measure_uncertainty() {
    let flat = entropy(&[0.25, 0.25, 0.25, 0.25]).expect("entropy");
    let peaked = entropy(&[0.97, 0.01, 0.01, 0.01]).expect("entropy");
    assert!(flat > peaked);
    assert!(
        (flat - 4.0f64.ln()).abs() < 1e-9,
        "uniform entropy is ln(4)"
    );
    assert!((margin(&[0.7, 0.2, 0.1]).expect("margin") - 0.5).abs() < 1e-9);
    assert!((max_probability(&[0.7, 0.2, 0.1]).expect("max") - 0.7).abs() < 1e-9);
    assert!(entropy(&[]).is_err());
}

#[test]
fn energy_orders_confidence() {
    // Peaked logits carry lower energy than flat ones at the same scale.
    let peaked = energy_score(&[5.0, 0.0, 0.0], 1.0).expect("energy");
    let flat = energy_score(&[1.0, 1.0, 1.0], 1.0).expect("energy");
    assert!(peaked < flat, "peaked energy {peaked} vs flat {flat}");
    assert!(energy_score(&[], 1.0).is_err());
    assert!(energy_score(&[1.0], 0.0).is_err());
    assert!(is_ood_by_energy(&[1.0, 1.0], 1.0, f64::NAN).is_err());
    // Peaked energy ≈ -5: above a very low threshold (OOD), below a high one.
    assert!(is_ood_by_energy(&[5.0, 0.0], 1.0, -100.0).expect("ood"));
    assert!(!is_ood_by_energy(&[5.0, 0.0], 1.0, 100.0).expect("ood"));
}

#[test]
fn distribution_validation_is_strict() {
    assert!(validate_distribution(&[0.5, 0.5], Some(2)).is_ok());
    assert!(validate_distribution(&[0.5, 0.5], Some(3)).is_err());
    assert!(validate_distribution(&[], None).is_err());
    assert!(validate_distribution(&[0.5, f64::NAN], None).is_err());
    assert!(validate_distribution(&[1.2, -0.2], None).is_err());
    assert!(validate_distribution(&[0.5, 0.4], None).is_err());
}
