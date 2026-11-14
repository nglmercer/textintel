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
the evaluation API; no training code is included. `data/quality-gates.json`
contains ambitious task-quality targets retained for visibility. The existing
heuristic baseline does not meet every target, particularly semantic and
cross-language similarity and some retrieval recall targets. Removing model
experiments does not establish that those quality gaps have been solved.

Use validation data to choose task thresholds and reserve the test split for
reporting. Heuristic spam scores report `calibrated=false`. Model comparisons
and the old production-model gates are no longer part of the library workflow.
