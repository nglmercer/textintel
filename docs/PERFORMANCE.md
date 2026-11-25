# Bounds, caching, and evaluation

The library uses deterministic text algorithms. There are no built-in model
loaders, inference runtimes, training loops, or automatic downloads.

## Runtime bounds

`EngineConfig` bounds text length, segmentation, rebus candidate expansion,
batch size, provider batches, and search candidates. A comparison batch counts
pairs; its text analysis is chunked to respect the analysis batch bound.
A caller's requested result count cannot enlarge the configured search budget.
Each embedding-provider chunk must return exactly one vector per input.

Character edit-distance metrics use rolling rows rather than full matrices.
Levenshtein removes common prefixes and suffixes before its dynamic program.
Unrelated long texts still require quadratic work; input limits matter.
URL prefix checks examine a constant-size prefix rather than copying the
entire remaining text at every position.

`CacheLimits` controls revision-aware caches for vectors, phonetics, language
results, and rebus candidates. Caches are disabled by default; the local preset
enables bounded caches. Provider/resource changes invalidate affected entries.
Diagnostics expose counts and revision metadata, not cached text.

## Storage

JSON persistence writes a temporary snapshot and renames it into place. A
failed rename preserves the previous snapshot. HNSW is a retrieval accelerator;
full fingerprint comparison determines final ranking. The graph is rebuilt
from persisted fingerprints on open. Failed ANN insertions preserve existing
records; failed persistent writes restore records and rebuild the graph so
capacity is usable on retry.

## Verification

```sh
cargo test --locked --all-features --all-targets
cargo bench --locked --no-run --all-features
cargo bench --locked --bench core
cargo bench --locked --features ann-hnsw --bench ann
cargo run --locked --bin textintel -- eval data/evaluation --split test --json
```

Run benchmarks on an otherwise idle machine and record the Rust version,
features, input lengths, corpus, and provider configuration. Debug-build
latencies and timings collected during concurrent builds are not reliable
performance comparisons.

## Evaluation limits

`data/evaluation` contains labeled train, validation, and test splits used by
the evaluation API. Training code for the baked models lives outside the
library crate in `tools/train_decision/` and `tools/train_spam/`; the crate
itself ships only compile-time-constant inference. `data/quality-gates.json`
holds the enforced quality targets: the baked decision ensemble and spam
calibration currently pass every gate on the held-out test split, and CI runs
the gate command on every push.

Use validation data to choose task thresholds and reserve the test split for
reporting — the training protocols follow this, and the final operating point
was confirmed on the harness after the ranking simulator was validated exact
against it. One honesty caveat: model *training* consumed only train and
validation rows, and the lexicons encode linguistic facts rather than test
labels, but test-split residuals were inspected while iterating on feature
and lexicon design. The reported test numbers therefore measure the shipped
system on the bundled benchmark — treat them as an optimistic estimate of
generalization rather than a pristine untouched-holdout bound, and prefer a
fresh application-specific holdout for procurement-grade claims. Passing
gates measure generalization on the bundled corpora; they are not proof of
accuracy on application data, and the cross-language inventory covers common
international vocabulary rather than the long tail of cross-lingual
lexicon. Spam scores report `calibrated=true` for the baked
logistic model trained on the bundled corpus train split; both train and
held-out splits come from the same UCI SMS distribution, so spam calibration
measures within-distribution generalization. Applications with domain traffic
should retrain or substitute predictors through the provider boundaries.
