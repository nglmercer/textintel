# Decision optimization experiments

Completed a bounded optimization session on 2026-09-30. These experiments target typed choices with probability distributions, rather than text generation. There is no direct Jev benchmark or evidence of general Jev parity.

## Results

236 configurations were evaluated: 128 routing interaction heads, 48 pair-matching interaction heads, and 60 routing prototypes. The grids completed in approximately 95 seconds after implementation and compilation. Further identical sweeps would add no evidence, so the remaining session focused on implementation, regression checks, and reproducibility.

| Approach | Held-out examples | Baseline accuracy | Candidate accuracy | Baseline NLL | Candidate NLL | Decision |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Routing interaction | 12 | 75.00% | 83.33% | 1.0134 | 0.8527 | Optional experimental artifact |
| Pair-matching interaction | 482 | 71.99% | 67.43% | 0.5769 | 0.6128 | Rejected for default use |
| Routing prototype | 12 | 75.00% | 33.33% | 1.0134 | 1.5557 | Rejected for default use |

Routing has only 12 training examples before augmentation, six validation examples, and 12 test examples. The one additional test success is preliminary. Prototypes scored perfectly on validation but failed on test, illustrating the danger of a tiny validation set. Pair matching is a specialized choice task derived from the existing similarity pairs. Existing shipped similarity/spam artifacts and default providers remain in place. Existing default/production similarity quality gates remain unresolved.

## Implementation

- Added deterministic local word-hash embeddings and a shared embedding loader. Training can run without downloading transformer weights or compiling transformer features.
- Borrowed indexed batches replace cloned training rows. Regression tests verify identical Adam updates.
- Feature cache keys include SHA256 source contents, encoder metadata, resource/schema identity, and fusion settings.
- Interaction artifacts pin encoder identity, preserve training provenance, and apply a validation-fitted temperature.
- Providers declare the evidence they consume. Interaction heads skip candidate fingerprint analysis; embedding-only heads skip all fingerprint analysis while retaining request and input-length validation. A regression verifies identical fused-head outputs.
- Added normalized centroid/description prototype training as an experimental alternative.
- Earlier similarity training improvements measured 7.19 to 4.87 seconds (32% reduction); see [PERFORMANCE.md](PERFORMANCE.md). Decision latency measurements were variable and are not a reliable speedup claim.

## Reproduce

```sh
cargo build --release --locked --no-default-features --bin textintel --bin textintel-train-decision
python3 tools/optimize_decisions.py --seconds 1200 --output target/decision-search
python3 tools/optimize_decisions.py --task routing --approach prototype --seconds 300 --output target/decision-prototypes
```

The search checks split overlap, keeps the test file empty during selection, and freezes the configuration before loading the held-out test split. Selection ranks validation macro-F1, then NLL, parameter count, and deterministic configuration order. Test results never select a runner-up. Logs and feature caches go under ignored `target/`; compact results are saved in [decision-experiments.json](decision-experiments.json). Final evaluations may add up to six minutes per task beyond the search deadline.

## Experimental routing artifact

`models/decision-routing-wordhash-v1.json` contains the frozen routing configuration: wordhash:128, eight hidden units, learning rate 0.01, seed 19, batch 32, maximum 100 epochs with patience eight, augmented training, state fusion, and rebus disabled. Retraining the frozen configuration preserved weights and temperature exactly. Use it explicitly:

```sh
target/release/textintel eval-decision data/decision --split test --provider interaction --head models/decision-routing-wordhash-v1.json --embeddings wordhash:128 --no-rebus --json
```

The original routing dataset is `data/decision`; the search also writes `target/decision-search/routing/dataset`. This artifact is intended for the small billing/sales/technical routing task, not a general classifier. Source digests and training settings are embedded in the artifact.

## Validation and limits

The full all-feature/all-target regression suite passed with localhost fixture access enabled; the normal sandbox prohibits those fixture binds. Clippy with warnings denied and targeted decision regressions were checked. New pretrained backbone downloads were unavailable because network connectivity failed, so no transformer-quality claim is made. Larger independent task datasets and a direct Jev comparison are needed before production promotion.
