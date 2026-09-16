# Changelog

All notable changes to this project are documented in this file. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions
match `Cargo.toml`, `textintel::API_VERSION`, and the `--version` /
`schema-version` CLI output.

## [Unreleased]

### Added

- Rule-based entity evidence provider (`rule_based_entities`) with bounded
  spans, wired into fingerprints and similarity comparison.
- Contextual semantic similarity features with compatibility-gated
  transliteration, plus entity-aware comparison signals.
- Multi-channel candidate union for retrieval with a semantic ANN channel
  (bounded per-channel budgets, no unbounded expansion).
- `models/similarity-v5.json`: retrained logistic similarity scorer
  (revision `train-20000-iter-lr0.2-l20.0001-ds0.8.0`, feature schema 9,
  dataset 0.8.0), selected on validation only.
- Evaluation dataset 0.8.0: expanded `train` (1121) and `validation`
  (415); `test` cases unchanged (482).
- Training follows the production semantic preference order:
  `textintel-train similarity` accepts `--transformer-model <dir>` (or
  `TEXTINTEL_TRANSFORMER_MODEL`) and records the embedding backend in the
  artifact's training config; without a configured transformer it featurizes
  with the feature-hash fallback and says so.
- Production diagnostics report retrieval candidate budgets
  (`EngineDiagnostics::candidate_budgets`).
- Evaluation dataset 0.9.0: expanded `train` (1229) and `validation`
  (469) with low-overlap paraphrases, cross-language paraphrases and
  cognates, false cognates, transliteration false friends, entity
  substitutions, single-word ambiguity, semantic hard negatives, and
  mixed-language paraphrases; `test` cases byte-identical (482).
- Regression suites: transformer semantic-evidence routing
  (`tests/transformer_semantics.rs`), store-path recall
  (`tests/search_recall.rs`), diagnostics budgets and no-raw-text
  guarantees.

### Fixed

- Retrieval evaluation grouped duplicate queries with graded relevance:
  cases sharing one query text used to occupy forced distinct ranks, which
  capped even a perfect ranker at MRR ~0.34. Production search now measures
  the rank of the first relevant variant (MRR 0.88 on the held-out split).

### Changed

- Internal modularization with no public API changes: the engine split
  into builder/analyzer/comparison/search/patterns/diagnostics modules,
  evaluation split into dataset/report/gates modules (quality gates moved
  from the CLI into the library with unit tests), the core engine
  coverage suite split by behavior area, and `src/entities.rs` split into
  `src/entities/` by scanner responsibility (provider, normalize, url,
  email, mention, numeric, currency, datetime, names).
- CI quality job additionally runs the production evaluation gate
  (`eval --production --gates data/quality-gates-production.json`).

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
