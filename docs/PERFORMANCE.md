# Performance and training validation

The optimization pass keeps the shipped similarity and spam artifacts. It
reduces repeated computation and allocation without changing comparison
features or their schema. New similarity training calibrates the bias against
the observed validation class distribution; artifact metadata records this
choice and revisions include `cal-uniform`.

## Measurements

Measurements used Rust 1.90.0, the repository's release profile (LTO, one
codegen unit), and the local CPU backends. Before and after executables were
run sequentially on CPU 0. Training times below average two runs.

| Workload | Before | After | Result |
|---|---:|---:|---|
| Similarity training, 20,000 steps, L2 0.0001, dataset 0.9.0 | 7.19 s | 4.87 s | 32% less time |
| Spam training, corpus 2.0.0 | 5.26 s | 5.14 s | Small difference; no significant gain claimed |
| Unicode Levenshtein with long shared affixes | 195.6 µs | 2.78 µs | About 70× faster for this workload |

The edit-distance microbenchmark ran 10,000 calls on strings with prefix
`"abcé中".repeat(24)`, suffix `"def🏠".repeat(24)`, and differing middle
`old` / `new`. Both implementations return distance 3. This improvement does
not describe arbitrary text comparisons: unrelated texts still require the
quadratic dynamic program.

Criterion runs of `analyze_rebus`, `compare_obfuscated`, and `decode_symbol`
varied considerably between runs. They do not support a reliable general CLI
speedup claim. One-shot and reusable gradient steps also have similar times
on the small bundled dataset; reduced feature extraction and bias calibration
account for the useful training improvement.

## Changes

- Reuse sanitized sample weights and gradient buffers across training steps.
- Precompute linear logits for bias calibration, avoiding discarded weight
  updates, repeated dot products, and unused loss calculations.
- Reuse train/validation feature rows for model metrics; featurize test only
  after training and calibration.
- Fold decoded keys and transliteration views once per comparison and share
  them across decoded-overlap and exact-decoding channels.
- Remove common affixes from Levenshtein's recurrence, keep the shorter
  dimension in rolling rows, and short-circuit equal n-grams and LCS inputs.
- Resolve the existing `nonminimal_bool` Clippy error.

## Model selection

Training used only the train split. Bias and temperature calibration used
validation; the test split did not select parameters or thresholds. Candidate
L2 values were 0.00001, 0.0001, 0.001, and 0.01, each at 20,000 steps.
Balanced calibration failed to improve on the frozen artifact. With uniform
calibration, the candidate with maximum validation F1 was L2 0.001.

| Metric | Shipped v5 | Selected candidate |
|---|---:|---:|
| Validation F1 | 0.8491 | 0.8705 |
| Validation Brier | 0.1281 | 0.1259 |
| Validation calibration error | 0.0832 | 0.0353 |
| Validation ROC-AUC | 0.9079 | 0.8954 |
| Validation PR-AUC | 0.9528 | 0.9485 |
| Held-out test F1 | 0.9144 | 0.8690 |

The candidate trades precision and ranking quality for recall, and does not
improve held-out quality. It was rejected; no shipped weights, decision
thresholds, quality gates, or datasets were changed. Improving validation
calibration alone does not establish that a replacement model is better.

## Verification and reproduction

The all-feature/all-target suite and targeted trainer, Unicode, and CLI checks
passed. The reusable optimizer matches the original reductions exactly, and
cached feature metrics match the public scorer within 1e-12. Uniform bias
calibration recovers a 75% positive prior on a constant-feature validation
batch. Spam retraining produced identical before/after artifacts.

Run from the checkout with the configured Rust toolchain:

```sh
cargo fmt --check
cargo test --locked --all-features --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo bench --locked --bench core -- --sample-size 40 --warm-up-time 1 --measurement-time 4
cargo run --locked --release --bin textintel-train -- similarity data/evaluation --iterations 20000 --l2 0.0001 --output /tmp/similarity-candidate.json
```

Keep candidate artifacts outside `models` until quality has been evaluated.
The existing default and production quality gates still fail with the shipped
model; this pass does not claim that those thresholds have been met. External
transformer weights, live LLM servers, GPU execution, and comparisons against
another product were not evaluated.
