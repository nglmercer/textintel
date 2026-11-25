# Decision-layer training tools

The default comparison readout is a baked ensemble — 0.6 gradient-boosted
trees + 0.4 logistic regression — over the explainable feature vector that
`src/comparison/scorer.rs` already exposes on `ComparisonResult`. This
directory holds the training and rendering pipeline; the baked inference code
is generated into `src/comparison/decision.rs` and never edited by hand.

## Pipeline

1. **Feature extraction** — `examples/eval_features.rs` dumps one JSONL row
   per evaluation case (plus optional synthetic pair files) with every raw
   channel/feature value:

   ```sh
   cargo run --locked --no-default-features --example eval_features -- \
     data/evaluation - tools/train_decision/noise_pairs.jsonl \
     > tools/train_decision/features_v2.jsonl
   ```

2. **Training** — pure-numpy trainers (`train.py` logistic, `gbm.py`
   gradient boosting; no sklearn dependency). Training rows are the
   evaluation `train` split plus the noise-negative augmentation; the
   `validation` split selects models; the `test` split is only ever used by
   held-out verification runs. Noise rows with substring containment or
   near-identity are dropped because their negative label is unreliable.

3. **Rendering** — `render_gbm_rust.py` writes `src/comparison/decision.rs`
   with the tree ensemble, logistic weights, feature extraction, and blend
   baked in as plain arrays.

4. **Verification** — the CI gate command scores the held-out test split:

   ```sh
   cargo run --locked --no-default-features --bin textintel -- \
     eval data/evaluation --split test --gates data/quality-gates.json
   ```

   `rank_sim.py` replicates the harness retrieval probe offline over
   extracted query/document feature rows, so blend weights can be swept
   without rebuilding Rust; its alpha=0.6 numbers reproduce the harness
   exactly.

## Augmentation protocol

`noise_pairs.jsonl` holds 2100 synthetic negatives: random pairings of
unrelated texts across scripts and lengths. They are labelled negative and
assigned to the train split. Verified properties:

- no noise `a`/`b` text appears verbatim anywhere in the test split (0/2100
  overlap on either side), so the held-out claim is intact;
- pairs with substring containment or near-identity are excluded at training
  time rather than trusted as negatives.

## Known limits

The ensemble reprices existing evidence; it cannot create evidence the
channels do not carry. Two gate categories remain out of reach for that
reason — see `docs/COVERAGE.md` for the measured analysis:

- `cross_language` requires translation-identity evidence (e.g. `red` the
  English color vs `red` the Spanish network — identical strings, negative
  by label) and true cross-lingual semantics;
- `spam` adversarial negatives are minimal-pair meaning swaps
  (`won`/`lost`, `expires`/`renewed`) where positives are synonym swaps
  (`needed`/`required`); separating them requires lexical-semantic resources
  (synonym/antonym knowledge) that surface and feature-hash channels do not
  contain.
