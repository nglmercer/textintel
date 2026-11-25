# Spam calibration training protocol

Trains the baked logistic calibration layer in `src/detection/spam.rs`
(`SPAM_MODEL_WEIGHTS`).

## Protocol

1. Extract features: `cargo run --locked --no-default-features --example
   spam_features -- data/spam/v2-train.json data/spam/v2-eval.json
   tools/train_decision/spam_features.jsonl`
2. Train: `python3 tools/train_spam/train.py
   tools/train_decision/spam_features.jsonl tools/train_spam/model.json`
3. Bake the reported bias/weights into `SPAM_MODEL_WEIGHTS` and update the
   provenance comment with the measured held-out metrics.

## Honesty constraints

- `v2-train.json` is the only training input. `v2-eval.json` is strictly
  held out and must never enter the fit.
- Both files are subsets of the same UCI SMS Spam Collection distribution;
  held-out metrics therefore measure within-distribution generalization.
  Applications with domain-specific traffic should retrain through the same
  protocol or replace the predictor through the `SpamPredictor` boundary.
- Measured on the held-out split: accuracy 0.969, F1 0.968, ROC-AUC 0.998.
