//! Calibration: temperature/bias application, temperature fitting, and
//! proper scoring rules (NLL, Brier, ECE).

use std::collections::BTreeMap;

use textintel::decision::{
    CalibrationSample, TaskCalibration, TemperatureBias, TemperatureScaling, brier_score,
    expected_calibration_error, fit_temperature, nll_loss, softmax,
};

#[test]
fn temperature_scaling_matches_softmax() {
    let scaling = TemperatureScaling::new(0.5).expect("temperature");
    assert_eq!(
        scaling.apply(&[2.0, 1.0]).expect("apply"),
        textintel::decision::softmax_with_temperature(&[2.0, 1.0], 0.5).expect("softmax")
    );
    assert!(TemperatureScaling::new(0.0).is_err());
    assert!(TemperatureScaling::new(-2.0).is_err());
    assert!(TemperatureScaling::new(f64::NAN).is_err());
    let identity = TemperatureScaling::identity();
    assert_eq!(
        identity.apply(&[1.0, 2.0]).expect("apply"),
        softmax(&[1.0, 2.0]).expect("softmax")
    );
}

#[test]
fn class_bias_shifts_winners() {
    let plain = TemperatureBias::identity();
    let logits = BTreeMap::from([("a".to_string(), 1.0), ("b".to_string(), 1.0)]);
    let even = plain.apply(&logits).expect("apply");
    assert!((even["a"] - 0.5).abs() < 1e-9);

    let biased =
        TemperatureBias::new(1.0, BTreeMap::from([("a".to_string(), 2.0)])).expect("calibration");
    let shifted = biased.apply(&logits).expect("apply");
    assert!(shifted["a"] > 0.8, "bias favors a: {shifted:?}");
    let sum: f64 = shifted.values().sum();
    assert!((sum - 1.0).abs() < 1e-9);

    // Unknown bias entries are ignored; missing ones read 0.
    let extra = TemperatureBias::new(1.0, BTreeMap::from([("zzz".to_string(), 99.0)]))
        .expect("calibration");
    let unchanged = extra.apply(&logits).expect("apply");
    assert!((unchanged["a"] - 0.5).abs() < 1e-9);

    assert!(TemperatureBias::new(1.0, BTreeMap::from([("a".to_string(), f64::NAN)])).is_err());
    assert!(plain.apply(&BTreeMap::new()).is_err());
}

#[test]
fn temperature_fitting_reduces_nll() {
    // Overconfident-but-wrong-heavy samples: the raw logits are too sharp
    // for their accuracy, so fitting must cool them (t > 1) and lower NLL.
    let samples: Vec<CalibrationSample> = (0..40)
        .map(|index| {
            // 70% correct at |logit| 3: overconfident, needs t > 1.
            let gold = if index % 10 < 7 { 0 } else { 1 };
            CalibrationSample::new(vec![3.0, 0.0], gold).expect("sample")
        })
        .collect();
    let fit = fit_temperature(&samples).expect("fit");
    assert!(
        fit.temperature > 1.0,
        "cooling expected, got {}",
        fit.temperature
    );
    assert!(
        fit.nll_after < fit.nll_before,
        "NLL {} should beat {}",
        fit.nll_after,
        fit.nll_before
    );
    // Deterministic: same samples, same temperature.
    let again = fit_temperature(&samples).expect("fit");
    assert_eq!(fit, again);
    assert!(fit_temperature(&[]).is_err());
}

#[test]
fn scoring_rules_reward_good_predictions() {
    let perfect_nll = nll_loss(&[0.0, 1.0], 1).expect("nll");
    let poor_nll = nll_loss(&[0.9, 0.1], 1).expect("nll");
    assert!(perfect_nll < poor_nll);

    let perfect_brier = brier_score(&[1.0, 0.0], 0).expect("brier");
    assert!((perfect_brier - 0.0).abs() < 1e-12);
    let worst_brier = brier_score(&[0.0, 1.0], 0).expect("brier");
    assert!(worst_brier > 0.5);

    // Perfectly confident and correct: ECE is 0.
    let perfect: Vec<(Vec<f64>, usize)> = vec![
        (vec![1.0, 0.0], 0),
        (vec![0.0, 1.0], 1),
        (vec![0.9, 0.1], 0),
    ];
    let ece = expected_calibration_error(&perfect, 10).expect("ece");
    assert!(ece < 0.05, "calibrated predictions have low ECE: {ece}");

    // Confident but always wrong: ECE is large.
    let wrong: Vec<(Vec<f64>, usize)> = vec![(vec![0.95, 0.05], 1), (vec![0.05, 0.95], 0)];
    let bad_ece = expected_calibration_error(&wrong, 10).expect("ece");
    assert!(
        bad_ece > 0.5,
        "miscalibrated predictions have high ECE: {bad_ece}"
    );

    assert!(nll_loss(&[0.5, 0.5], 7).is_err());
    assert!(brier_score(&[0.5, 0.5], 7).is_err());
    assert!(expected_calibration_error(&perfect, 0).is_err());
    assert_eq!(expected_calibration_error(&[], 10).expect("empty"), 0.0);
}

#[test]
fn task_calibration_validates_bounds() {
    assert!(TaskCalibration::new("routing", 0.81, 0.9).is_ok());
    assert!(TaskCalibration::new("", 0.81, 0.9).is_err());
    assert!(TaskCalibration::new("routing", 0.0, 0.9).is_err());
    assert!(TaskCalibration::new("routing", 0.81, 1.5).is_err());
    assert!(TaskCalibration::new("routing", 0.81, f64::NAN).is_err());
    let task = TaskCalibration::new("routing", 0.81, 0.9).expect("task");
    let json = serde_json::to_string(&task).expect("serialize");
    let parsed: TaskCalibration = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed, task);
}
