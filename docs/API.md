# textintel API reference

This is the narrative companion to the rustdoc (`cargo doc --open -p
textintel`). Every signature below mirrors `src/`; when in doubt the source
and rustdoc win.

Library version, JSON contract version, and fingerprint schema version are
available at runtime:

```rust
assert_eq!(textintel::API_VERSION, "0.2.0");
assert_eq!(textintel::FINGERPRINT_SCHEMA_VERSION, 2);
```

## Engine

`textintel::TextIntelligence` is the entry point. Three ways to build one:

| Constructor | Behavior |
|---|---|
| `TextIntelligence::default()` | Model-free, deterministic, offline. |
| `TextIntelligence::new(config)` / `try_new(config)` | Custom `EngineConfig` limits; `try_new` validates. |
| `TextIntelligence::production_local()` | Local preset: resource packs, trained models when present, bounded caches; never touches the network. Check `diagnostics().degraded` for fallbacks. |
| `TextIntelligence::builder()` | `EngineBuilder` for explicit providers, stores, and model paths; add `.production_local()` for the preset plus overrides. |

### Analysis

```rust
use textintel::TextIntelligence;

let engine = TextIntelligence::default();
let fingerprint = engine.analyze("Fra🏠do")?;          // full evidence channels
let batch = engine.analyze_batch(&["a".to_string()])?; // bounded by max_batch_size
let (timed, timings) = engine.analyze_with_timing("Fra🏠do")?; // StageTimings: durations only
# Ok::<(), textintel::TextIntelError>(())
```

`MessageFingerprint` keeps the `raw` message plus independent views
(normalized forms, segments, language candidates, n-grams, visual,
symbolic, rebus, phonetic, semantic, transliteration, and obfuscation
features). Helpers: `top_language()`, `decoded_texts()`.

### Comparison and duplicates

```rust
use textintel::{DuplicateMode, TextIntelligence};

let engine = TextIntelligence::default();
let comparison = engine.compare("c0mpr4 ah0r4", "compra ahora")?;
let pairs = engine.compare_batch(&[("a".to_string(), "b".to_string())])?;
let direct = engine.compare_fingerprints(&engine.analyze("a")?, &engine.analyze("b")?);
let duplicate = engine.duplicate("a", "a", 0.85)?;
let decoded_only = engine.duplicate_with_mode("a", "a", 0.85, DuplicateMode::Decoded)?;
# Ok::<(), textintel::TextIntelError>(())
```

`ComparisonResult` carries per-channel `Option` scores plus `score` and
`explanations`; missing channels are skipped and remaining weights
renormalized, never scored as zero. `DuplicateMode` (`Combined` default,
`NearExact`, `Lexical`, `Semantic`, `Phonetic`, `Decoded`, `Visual`)
selects the evidence slice.

### Rebus decoding

```rust
use textintel::TextIntelligence;

let engine = TextIntelligence::default();
let candidates = engine.decode("salU2")?;
let scoped = engine.decode_with_languages("salU2", Some(&["es".to_string()]), Some(5))?;
# Ok::<(), textintel::TextIntelError>(())
```

`DecodedCandidate { text, score, transformations, language, .. }` records
each derivation step; `Transformation::explain()` renders it for the
`explain` CLI command.

### Patterns and spam

```rust
use textintel::TextIntelligence;

let engine = TextIntelligence::default();
engine.add_pattern("promo", vec!["win a free prize".to_string()])?;
let matches = engine.match_patterns("win a free prize today")?;
let spam = engine.detect_spam("win a free prize today, claim now")?;
assert!(engine.remove_pattern("promo")?);
# Ok::<(), textintel::TextIntelError>(())
```

Patterns persist across restarts with versioned files:

```rust
use textintel::TextIntelligence;

let engine = TextIntelligence::default();
engine.add_pattern("promo", vec!["win a free prize".to_string()])?;
let path = std::env::temp_dir().join("textintel-api-patterns.json");
engine.save_patterns_to(&path)?;
let count = TextIntelligence::default().load_patterns_from(&path)?;
assert_eq!(count, 1);
std::fs::remove_file(&path).ok();
# Ok::<(), textintel::TextIntelError>(())
```

### Retrieval

```rust
use textintel::TextIntelligence;

let engine = TextIntelligence::default();
engine.add_document("doc-1", "compra ahora, oferta limitada")?;
assert_eq!(engine.document_count()?, 1);
let hits: Vec<_> = engine.find_similar("oferta de compra", 5)?;
let duplicates = engine.find_duplicates("compra ahora", 0.85)?;
assert!(engine.remove_document("doc-1")?);
# Ok::<(), textintel::TextIntelError>(())
```

`with_store` accepts any `VectorStore`; `with_json_store(path)` opens the
versioned file store (schema v2, migrates v1 records on load). Optional
features add `RedbStore` (`persist-redb`) and HNSW ANN
(`HnswVectorIndex`, `ann-hnsw`).

### Introspection

- `engine.config()` — active `EngineConfig`.
- `engine.diagnostics()` — providers, model revisions, resource manifest,
  store/ANN/cache status, and the `degraded` fallback list.
- `engine.provider_capabilities()` — per-provider `ProviderCapabilities`
  with `Basic` / `Production` / `Unavailable` quality tiers.
- `engine.health_check()` — fails fast on invalid configuration.
- `engine.resource_manifest()` — loaded pack provenance.

## Providers

Expensive or remote behavior is explicit: implement a provider trait and
inject it. Engine methods take `self` (`with_*`); the builder mirrors them:

| Concern | Trait | Built-in backends |
|---|---|---|
| Embeddings | `EmbeddingProvider` | `Null` (default), `FeatureHash` (local baseline), `Static`, `Cached`, `Candle` (`semantic-candle`), `Transformer` (`semantic-transformer`), `Http` (`semantic-http`) |
| G2P | `G2PProvider` | `Null` (default), `RuleBased`, `Cached`, `EspeakNg` (`phonetic-espeak`) |
| Language | `LanguageDetectionProvider` | n-gram detector over resource packs, `Cached`, profile |
| Lexicon / lemmatizer | `LexiconProvider` / `LemmatizerProvider` | `DefaultLexiconProvider` from resource packs |
| Symbols / abbreviations | `SymbolKnowledgeProvider` / `AbbreviationProvider` | `DefaultSymbolKnowledge`, resource packs |
| Transliteration | `TransliterationProvider` | `RuleBasedTransliterationProvider` |
| Reranker | `RerankerProvider` | `ChannelScoreReranker` from `RerankerModelArtifact` |
| Spam | `SpamPredictor` | `HeuristicSpamPredictor`, `TrainedSpamPredictor` |
| Similarity | `SimilarityScorer` | `LogisticSimilarityScorer` from `SimilarityModelArtifact` |
| Stores | `VectorStore` | `MemoryStore`, `JsonFileStore`, `RedbStore` (`persist-redb`) |

Provider failures surface as `TextIntelError::Provider`; unconfigured
backends report `Unavailable` instead of pretending to work.

## Configuration and errors

Key `EngineConfig` knobs (all validated, no panics): `max_input_length`
(8192), `max_segments`, `beam_width`, `max_candidates`,
`max_symbol_readings`, `max_recursion`, `max_documents`, `max_batch_size`,
`max_search_candidates`, `max_decoded_branches`, `semantic` / `phonetic`
channel flags, `similarity_weights`, `rebus_weights`, `language_hints`,
and `cache: CacheLimits`.

`TextIntelError` variants: `InputTooLong`, `TooManySegments`,
`InvalidConfiguration`, `Provider`, `Storage`, `Serialization`.
`ResourceError` covers pack loading (`ResourceLoader::from_resource_root`,
`ResourceLimits`).

## Features

Default build is dependency-light and offline. Opt-in features:
`lang-profile`, `semantic-local`, `semantic-candle`,
`semantic-transformer`, `semantic-http`, `phonetic-ipa`,
`phonetic-espeak`, `ann-hnsw`, `persist`, `persist-redb`, `semantic`,
`phonetic`, `ml`, `production-local-lite`, `production-local`, `all`.
`semantic-http` is the only remote adapter and is never enabled
implicitly. Minimum supported Rust version is 1.71 (see `rust-version` in
`Cargo.toml` and the MSRV CI job).

## CLI contract

The `textintel` binary mirrors the library: `analyze`, `explain`,
`decode`, `compare`, `duplicate`, `spam`, `batch`, `resources`,
`diagnostics`, `provider-info`, `schema-version`, `eval` (`evaluate`
alias), `index`, `search`. Global flags: `--json`, `--production`,
`--resource-root <dir>`, `--model-path <dir>`, `--language <code>`.
Every `--json` payload follows `API_VERSION` (see `schema-version`) and
evolves additively within a major version. `--version` prints the crate
version.

## Trained artifacts

- `models/similarity-v4.json` — logistic similarity scorer, feature
  schema 8, trained on evaluation dataset 0.7.0 (`train` split, bias
  calibrated on `validation`, metrics reported on held-out `test`).
  Retrain: `cargo run --bin textintel-train -- similarity data/evaluation
  --output models/similarity-v4.json`.
- `models/spam-v1.json` / `models/spam-v2.json` — calibrated logistic
  spam predictors (v2 preferred). Retrain: `cargo run --bin
  textintel-train -- spam --output <artifact.json>`.
- Loading is strict: a present-but-invalid artifact fails loudly; only a
  missing preset path falls back (with a `degraded` note).

## Evaluation

`data/evaluation/` (version 0.7.0: 1045 train / 375 validation / 482 test
cases) plus `data/quality-gates.json` (default engine) and
`data/quality-gates-production.json` (production preset). Run:

```text
cargo run --bin textintel -- eval data/evaluation/ --split test --gates data/quality-gates.json
cargo run --bin textintel -- eval data/evaluation/ --split test --production --gates data/quality-gates-production.json
```
