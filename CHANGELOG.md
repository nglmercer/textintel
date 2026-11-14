# Changelog

All notable changes to this project are documented in this file. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions
match `Cargo.toml`, `textintel::API_VERSION`, and the `--version` /
`schema-version` CLI output.

## [Unreleased]

### Changed

- Focus the crate on deterministic text analysis, comparison, decoding,
  provider boundaries, and storage. The default dependency set is lightweight.
- The local preset enables caches and deterministic providers without reading
  model directories or model environment variables.

### Removed

- Experimental decision APIs, transformer/HTTP model backends, generation,
  model catalogs, learned scorers, trained artifacts, and training/evaluation
  programs dedicated to models. Their public APIs and CLI flags are removed.
- Model-only fixtures, benchmarks, datasets, and speculative research reports.
  Essential external references remain in `docs/EXPERIMENTS.md`.

### Fixed

- Comparison batch limits count pairs while analysis stays chunked.
- Embedding chunks enforce their output count and search result requests
  respect the configured candidate budget.
- Currency amounts retain decimal digits, ISO dates follow calendar rules,
  and URL prefix scanning avoids repeated suffix copies.
- JSON rename failures preserve the previous snapshot. Failed ANN updates
  preserve records; persistent-write rollback restores usable ANN capacity.
- espeak output is drained concurrently with bounded reads, avoiding pipe
  deadlocks while retaining the subprocess timeout.

## [0.2.0] - 2026-09-16

First tagged release: local-first multilingual text intelligence as a Rust
library plus the `textintel` CLI.

### Added

- `TextIntelligence` engine: `analyze`, `compare`, `decode`, `detect_spam`,
  duplicate detection, pattern registration and matching, batch APIs, and an
  indexed in-memory document store with `find_similar`.
- Independent evidence channels in `MessageFingerprint`: Unicode
  normalization, segmentation, language candidates, lexical/character/visual
  similarity, symbol readings, rebus decoding, phonetic views, semantic
  views, transliteration, and obfuscation features.
- `EngineBuilder` with the `production_local()` preset: embedded resource
  packs, trained similarity/spam models when present, bounded
  revision-aware caches, and honest `degraded` fallback notes surfaced
  through `diagnostics()` and `provider_capabilities()`.
- Provider traits with injectable defaults: embeddings (null, feature-hash,
  static, cached), G2P (null, rule-based, optional espeak-ng),
  language detection, lexicon, lemmatizer, symbols, abbreviations,
  transliteration, reranker, spam predictor, and similarity scorer.
- Versioned JSON persistence for fingerprints (`JsonFileStore`, schema v2
  with migration) and patterns (`save_patterns_to` / `load_patterns_from`),
  plus the `VectorStore` boundary and optional `redb` / HNSW backends.
- Trained artifacts: logistic similarity scorer (`models/similarity-v4.json`,
  feature schema 8, dataset 0.7.0) and calibrated spam predictors
  (`models/spam-v1.json`, `models/spam-v2.json`); `textintel-train` CLI to
  retrain both from `data/evaluation/`.
- `textintel` CLI: `analyze`, `explain`, `decode`, `compare`, `duplicate`,
  `spam`, `batch`, `resources`, `diagnostics`, `provider-info`,
  `schema-version`, `eval`, `index`, and `search`, with `--json` output
  covered by the API-version contract.
- Seed resources for 16 languages (word packs, symbol readings,
  abbreviations) with schema validation, provenance metadata, and enforced
  loader limits; documented in `resources/README.md`.
- Versioned evaluation set (`data/evaluation/`, 0.7.0, 1902 cases over
  train/validation/test) with default and production quality gates.
- Runnable examples: `basic`, `production_local`, `patterns_spam`,
  `persistent_store`, plus feature-gated `espeak` and
  `transformer_embeddings`.

### Security

- Bounded inputs everywhere (`EngineConfig`, `CacheLimits`,
  `ResourceLimits`); non-finite model parameters rejected at load; user
  text is never fetched as a URL and never executed as a shell command.

[0.2.0]: https://github.com/nglmercer/textintel/releases/tag/v0.2.0
