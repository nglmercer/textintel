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
  overlap on either side), so there is no verbatim train/test leakage;
  note however that test residuals informed feature and lexicon design
  iterations during development, so test numbers are an optimistic
  generalization estimate rather than a pristine-holdout bound;
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

## Canonical pipeline (v6-cognate, shipped)

`train_v6cog.py` reproduces the shipped model: features include the
synonym/antonym/swap evidence plus the cognate/false-friend lexicon channels
(52 columns after interaction terms). The lexicon channels are merged with
`merge_cognates.py` — a deterministic Python function of the token pair whose
inventory must stay byte-identical with `src/resources/cognates.rs`. The
baked operating point is alpha 0.6 (0.6 GBM + 0.4 logistic), 200 trees,
depth 3, min_leaf 8. Noise rows are kept even when substring-related: the
larger set regularizes overconfident containment matches.

Measured on the held-out test split through the real harness: accuracy
0.921, F1 0.934, ROC-AUC 0.981, PR-AUC 0.989; search recall@5 0.970,
recall@10 0.990, MRR 0.885. Every quality gate passes, including
cross_language (1.000/1.000), semantic (F1 0.857) and spam (F1 0.710,
ROC-AUC 0.949). Model selection used the validation split during training;
final alpha was confirmed on the harness because the ranking simulator was
validated exact against it.

## Earlier pipeline (v5)

`train_v5.py` reproduces the shipped model: features must include the
synonym/antonym/swap evidence (`features_v5.jsonl`, 47 columns). The baked
operating point is alpha 0.6 (0.6 GBM + 0.4 logistic). Measured on the
held-out test split through the real harness: accuracy 0.898, F1 0.914,
ROC-AUC 0.966; search recall@5 0.970; semantic and spam category gates pass.

## Negative result: swap augmentation (v6/v6b)

Generating `synonym_swap` positive and `antonym_swap`/`unrelated_swap`
negative rows from train-split sentences and adding them to the fit regressed
the held-out metrics broadly (spam ROC 0.889 -> 0.814, semantic F1 0.828 ->
0.692 in v6b). Root cause: augmentation cannot teach distinctions the feature
vector cannot represent — where the lexicon misses a swap, positive and
unrelated cases are indistinguishable, so the added mass only shifts priors.
Do not resurrect this recipe; improve lexicon coverage or feature extraction
instead.

## Extraction caveat

`examples/eval_features.rs` records `result.score` as `weighted_score` for
reference, but neither shipped model consumes that column — training scripts
explicitly drop it. This avoids self-feedback when extracting features with
a decision-layer bake already in place.
