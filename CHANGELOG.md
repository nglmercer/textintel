# Changelog

All notable changes to this project are documented in this file. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions
match `Cargo.toml`, `textintel::API_VERSION`, and the `--version` /
`schema-version` CLI output.

## [Unreleased]

### Added

- Baked decision ensemble for comparison scoring with default weights:
  gradient-boosted trees blended with a logistic layer over the explainable
  evidence vector (`src/comparison/decision.rs`). Unlike the removed neural
  infrastructure, inference is compile-time-baked, deterministic,
  allocation-free and dependency-free; every input feature remains visible in
  `evidence`/`explanations`. Custom `SimilarityWeights` retain the plain
  weighted combination. Training protocol and reproducibility notes in
  `tools/train_decision/README.md`. Measured on the held-out test split:
  accuracy 0.921, F1 0.934, ROC-AUC 0.981; search recall@5 0.970, MRR 0.885;
  all quality gates pass.
- Baked synonym/antonym lexicon and cross-lingual cognate/false-friend
  inventory with explainable `ComparisonResult` channels (`swap_synonym`,
  `swap_antonym`, `multi_swap_synonym_coverage`, `multi_swap_antonym`,
  `cognate_match`, `false_friend`), separating paraphrase substitutions from
  meaning flips and true cognates from byte-identical false friends.
- Trained logistic spam calibration replacing the hand-weighted heuristic sum
  (`src/detection/spam.rs`). `SpamResult.calibrated` is now `true`; measured
  on the strictly held-out corpus split: accuracy 0.969, F1 0.968 (was 0.0),
  ROC-AUC 0.998. Retraining protocol in `tools/train_spam/README.md`; custom
  `SpamPredictor` substitution unchanged.

- Optional complete typed-value gold and separate semantic evaluation metrics,
  preserving exact signs, currency identities, time fields and decimal precision.
  Value failures remain failures under both spoken comparison gates. A separate
  structural regression corpus exercises the twenty-language core.
- Experimental typed speech parsing and rendering for twenty language
  conventions, with explicit interpretation, date/decimal/currency policies,
  exact quantities, calendar forms, fractions, ranges, scientific notation,
  ordinals, durations, and diagnostics for unresolved readings.
- Add bounded alternate readings for standalone German exact-hour clocks and
  Arabic integer currency counts 3–10 with reviewed masculine unit nouns,
  preserving source metadata, value, sign and currency naming. Decimal/register
  mixtures remain unresolved; default rendering and upstream gold are unchanged.
- Separate literal and reviewed-variant speech evaluation, preserving original
  corpus gold and reporting contradictory references and named comparison rules.
- Opt-in full localized currency names through `CurrencyNaming::Full` and
  `--currency-naming full`, retaining currency identity in minor-only amounts.
- Opt-in normalization of coherent redundant clock periods, such as `13:10pm`,
  with strict parsing retained by default and canonical semantic values.
- Opt-in literal reading of impossible calendar combinations through
  `InvalidDatePolicy::ReadComponents`. Invalid-date values and diagnostics
  remain visible even when speech generation or literal word matching succeeds.
- Retained evaluation diagnostics, including supported literal readings, and
  bounded monetary reading/layout variants backed by unchanged typed values.
- Seven attributed language profiles and shared localized symbol readings,
  with opt-in completion for custom resource loaders.
- A reproducible optional eSpeak runtime audit covering the twenty language
  families, registered aliases, and explicit Japanese/Arabic input controls.
- Exact subsecond clock IR and reading across the twenty-language core,
  retaining every fraction digit; comma fractions require explicit Time intent.

### Changed

- `EngineConfig::default()` enables the deterministic rule-based phonetic and
  feature-hash semantic channels; both are dependency-free and available in
  `--no-default-features` builds. Phonetic evidence is suppressed for
  disjoint-script pairs, fixing the inversion where true cross-script
  transliterations scored below same-script lookalikes.
- Focus the crate on deterministic text analysis, comparison, decoding,
  provider boundaries, and storage. The default dependency set is lightweight.
- The local preset enables caches and deterministic providers without reading
  model directories or model environment variables.
- Speech preparation uses `tn-v3` cache identities. Both entry points reuse
  protected-region scanning, Unicode digit/calendar primitives, locale metadata,
  pronunciation dictionaries, and number composition. Established versioned
  formatting remains behind explicit adapters.
- Regional number/date/currency conventions share one locale registry;
  scientific, duration, fraction and range grammars share compatibility profiles.
- Versioned classification and typed augmentation share one immutable source
  protection map instead of rescanning code and tags independently.

### Removed

- Experimental decision APIs, transformer/HTTP model backends, generation,
  model catalogs, learned scorers, trained artifacts, and training/evaluation
  programs dedicated to models. Their public APIs and CLI flags are removed.
- Model-only fixtures, benchmarks, datasets, and speculative research reports.
  Essential external references remain in `docs/EXPERIMENTS.md`.

### Fixed

- Shared sentence boundaries recognize Arabic, Urdu and Indic terminators,
  retaining mixed punctuation runs, closers, protected text and bounded chunks.
- Arabic euro one/dual readings reuse the shared count agreement for standalone
  integer major units and existing cent composition, with full-name paradigms
  still explicitly bounded.

- Clock seconds use shared quantity agreement in English, Spanish, Portuguese
  and German; Portuguese clock hours retain feminine agreement. Implicit Arabic
  one/dual unit patterns no longer erase explicit decimal precision.
- Arabic feminine counting and construct duals share one number core for units
  and currencies. INR/PKR/IDR/TRY gain reviewed integer paradigms; unresolved
  noun-order and decimal constructions remain unsupported.
- Explicit AM/PM clocks retain their time value in all twenty languages, using
  the shared 24-hour fallback where no spoken period is rendered. Missing
  minutes remain absent, and compact date/time projection retains extra labels
  and period metadata in temporal IR.
- Vietnamese written day labels are consumed once, German clock readings use
  Uhr rather than duration nouns, and English 24-hour whole hours avoid o'clock.
- Arabic ruble and Swiss-franc amounts reuse reviewed count/qualifier paradigms;
  unsupported full currency names remain explicit. Russian exact-hour variants
  work in both zero-minute directions without dropping a source AM/PM marker.
- Japanese phonetic dates and clocks share counter readings across speech
  styles, including calendar-day exceptions and clocks without written minutes.
- Comparison batch limits count pairs while analysis stays chunked.
- Embedding chunks enforce their output count and search result requests
  respect the configured candidate budget.
- Currency amounts retain decimal digits, ISO dates follow calendar rules,
  and URL prefix scanning avoids repeated suffix copies.
- JSON rename failures preserve the previous snapshot. Failed ANN updates
  preserve records; persistent-write rollback restores usable ANN capacity.
- espeak output is drained concurrently with bounded reads, avoiding pipe
  deadlocks while retaining the subprocess timeout.
- Native combining marks remain attached to words, entity numeric/date spans
  recognize native decimal digits with original UTF-8 offsets, and unsupported
  G2P languages no longer inherit English/Spanish confidence.
- Scaled measures retain their entire quantity under explicit hints; Japanese
  and Korean date defaults use year/month/day. Russian explicitly written
  US-dollar qualifiers and Spanish feminine clock hours are retained correctly.
- Signed fractions keep their sign in the numeric representation. Scalar hints
  reject durations, conflicting Arabic separators preserve input, and phone/file
  shapes are protected before numeric rewriting. Spanish half-unit agreement and
  English fraction rendering retain values when changing reading style.
- AM/PM fields require twelve-hour values across all twenty languages, and era
  year zero is rejected in both parsing and public-document rendering.
- Oversized fraction and duration counts preserve their input instead of
  reaching a numeric renderer outside its supported range.
- eSpeak resolves regional language tags, reads the real voice inventory, and
  excludes recognized language switches and ZWJ controls from phoneme tokens.
  `for_voice` now honors explicit overrides, with capabilities and confidence
  reflecting the selected voice family.
- eSpeak receives an option terminator before source text, so leading `-`
  forms are read as data instead of altering the subprocess options.

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
