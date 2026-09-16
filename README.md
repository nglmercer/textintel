# textintel

`textintel` is a local-first Rust library for multilingual text intelligence.
It keeps independent evidence channels in a `MessageFingerprint` instead of
forcing a single interpretation of a message.

The Rust crate is the sole implementation and exposes a typed API.

## Quick start

```rust
use textintel::TextIntelligence;

let engine = TextIntelligence::default();
let fingerprint = engine.analyze("Fra🏠do")?;
let comparison = engine.compare("Fra🏠do", "fracasado")?;
let candidates = engine.decode("salU2")?;

assert_eq!(fingerprint.raw, "Fra🏠do");
assert!(candidates.iter().any(|candidate| candidate.text.eq_ignore_ascii_case("saludos")));
# Ok::<(), textintel::TextIntelError>(())
```

The default engine requires no model download and provides:

- Unicode normalization, invisible/bidi/combining-character checks, scripts,
  confusable skeletons, and homoglyph similarity.
- Unicode-aware segmentation, language candidates, token/word n-grams,
  lightweight lemmatization, stop-word extraction, Jaccard, TF-IDF, MinHash,
  and SimHash.
- Character metrics: Levenshtein, Damerau-Levenshtein, Jaro, Jaro-Winkler,
  n-gram overlap, LCS, and configurable combined similarity.
- Multi-reading symbols, numbers, currency, and math tokens, bounded
  beam-search rebus decoding, uncertainty-preserving spoken candidates, and
  obfuscation features.
- Deterministic local character n-gram language detection with top-k
  probabilities, script hints, and segment-level code-switch evidence.
- A deterministic rule-based G2P fallback, phoneme edit/feature distance, and
  optional local, HTTP, embedding, and reranker providers.
- Pattern registration, calibrated feature-based spam signals, selectable
  duplicate modes, an indexed in-memory store, versioned JSON persistence, and
  a `VectorStore` boundary for larger backends.
- Versioned pattern persistence: registered patterns snapshot to a schemaed
  file and reload with re-analyzed examples (`save_patterns_to` /
  `load_patterns_from`). Stored fingerprints migrate across schema versions
  through an explicit version-checked path instead of failing or guessing.
- Trained artifacts: an interpretable logistic similarity scorer
  (`models/similarity-v1.json`, see `tools/train_similarity.rs`) and a
  calibrated spam predictor (`models/spam-v1.json`).

Optional production backends (off by default, no automatic network access):

- `semantic-candle`: real local multilingual embeddings via the Candle
  runtime (explicit local model path).
- `semantic-transformer`: contextual BERT-family sentence embeddings on the
  CPU from explicit local files (`config.json`, `vocab.txt`,
  `model.safetensors`), with batching, dimension validation, normalization,
  and truncation reporting. No new dependencies beyond Candle.
- `phonetic-espeak`: production G2P backed by a local `espeak-ng` binary,
  with the rule-based provider as deterministic fallback. Supports voice
  detection (`installed_voices`), per-call timeouts, syllables, recovered
  primary stress, articulatory features, and discounted confidence for
  unmapped languages.
- `ann-hnsw`: approximate nearest-neighbor retrieval over embedding indexes
  plus `benches/ann.rs` recall/latency benchmarks.
- `persist-redb`: embedded persistent storage with the same migration
  guarantees as the JSON store.
- `semantic-http`: explicit remote embedding adapter (only when configured).

## Modes: default, production-local, remote

```text
Default:
fast, deterministic, local, model-free.

Production local:
embedded resource packs + trained models + espeak-ng G2P when installed
+ local embedding baseline. Graceful fallback with degradation notes.

Remote:
explicit HTTP providers only, never automatic.
```

```rust
use textintel::TextIntelligence;

// Lightweight and deterministic (unchanged default).
let engine = TextIntelligence::default();

// Local production preset: never touches the network, never panics when an
// optional dependency (espeak-ng, model files) is missing. Check
// `engine.diagnostics().degraded` to see what fell back.
let engine = TextIntelligence::production_local()?;

// Ergonomic builder with the same preset plus explicit configuration.
let engine = TextIntelligence::builder()
    .production_local()
    .trained_similarity_model("models/similarity-v1.json")
    .trained_spam_model("models/spam-v1.json")
    .build()?;
# Ok::<(), textintel::TextIntelError>(())
```

Providers report quality tiers through `engine.provider_capabilities()`:
`RuleBasedG2PProvider` and feature-hash embeddings are `Basic`,
`EspeakNgG2PProvider` and transformer/HTTP embeddings are `Production`,
and unconfigured backends report `Unavailable` instead of pretending to
work. `engine.diagnostics()` adds API/schema versions, the loaded resource
manifest (source, license, revision per pack), and the degraded list.

## Providers and configuration

Expensive or remote functionality is explicit. Implement one of the provider
traits and inject it with a builder method:

```rust
let engine = TextIntelligence::new(textintel::EngineConfig {
    semantic: true,
    phonetic: true,
    ..Default::default()
})
.with_embedding_provider(my_embedding_provider)
.with_g2p_provider(my_g2p_provider);
```

Provider failures are returned as `TextIntelError::Provider`; input length,
segments, beam width, symbol readings, decoded candidates, branches,
recursion, batch size, search candidates, reranker candidates, cache entries,
and model token length are bounded by `EngineConfig`, `CacheLimits`, and the
provider constructors. Non-finite model parameters are rejected at load;
user text is never fetched as a URL and never executed as a shell command.

Optional feature names are available for packaging integrations:
`lang-profile`, `semantic-local`, `semantic-candle`, `semantic-http`,
`phonetic-ipa`, `phonetic-espeak`, `ann-hnsw`, `persist`, `persist-redb`,
`semantic`, `phonetic`, `ml`, `production-local-lite`, `production-local`,
`semantic-transformer`, and `all`. The default build stays local and
lightweight; `semantic-http` is the explicit remote-provider adapter and
`semantic-local` provides the deterministic feature-hash embedding baseline.
`production-local` is the full local stack (transformer embeddings, espeak-ng
phonetics, HNSW retrieval, redb persistence; no network access);
`production-local-lite` is the same preset without the heavy native
dependencies.

Provider capability and model metadata are exposed so callers can record the
backend, revision, dimensions, language coverage, and local/remote status used
for each result. `SimilarityProfile` and `SimilarityScorer` allow calibrated
task-specific scoring without changing the independent evidence channels.

## Language and symbol resources

The default repository resources contain versioned seed packs for 16 languages:
Arabic, Chinese, Dutch, English, French, German, Hindi, Indonesian, Italian,
Japanese, Korean, Polish, Portuguese, Russian, Spanish, and Turkish. These are
starter resources, not complete dictionaries. The loader indexes every JSON
pack found recursively, so larger licensed dictionaries can be added without
hardcoding words in Rust:

```rust
let resources = textintel::ResourceLoader::from_resource_root("resources")?;
let engine = textintel::TextIntelligence::default().with_resources(resources);
```

The initial English pack includes the requested examples `this is a example`
and `a good example`. See [`resources/README.md`](resources/README.md) for the
pack schema, compact `words` / `stop_words` forms, source-aware lookup records,
revision/hash fields, and loader limits. Index keys are deterministic: numeric
runs sort first using natural numeric order, followed by Unicode letters and
then other characters. This is only an ordering rule; matching still preserves
the exact language and Unicode form needed for reliable tracking.

Symbols are modularized into neutral Unicode metadata and per-language reading
packs. Adding another language JSON pack automatically extends symbol coverage
without changing Rust code. Provenance is retained for each merged record, and
resource byte, entry, symbol, and reading limits are enforced before indexing.

## CLI

```text
cargo run -- analyze "Fra🏠do" --json
cargo run -- decode "salU2"
cargo run -- compare "c0mpr4 ah0r4" "compra ahora" --json
cargo run -- spam "ganaste un premio https://example.test"
cargo run -- batch messages.jsonl
cargo run -- resources resources
cargo run -- diagnostics
cargo run -- evaluate data/evaluation/
cargo run -- index store.json id-1 "a message"
cargo run -- search store.json "similar message" 5
```

## Examples

```text
cargo run --example basic
cargo run --example production_local
cargo run --example persistent_store
cargo run --features semantic-transformer --example transformer_embeddings
cargo run --features phonetic-espeak --example espeak
cargo run --example patterns_spam
```

## Verification

Before every commit run:

```text
cargo fmt --check
cargo test --locked --no-default-features --all-targets
cargo test --locked --all-features --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps --all-features
cargo bench --locked --no-run --all-features
cargo run --locked --all-features --bin textintel -- eval data/evaluation/ --split test --gates data/quality-gates.json
```

Then the production preset separately:

```text
cargo run --locked --all-features --bin textintel -- eval data/evaluation/ --split test --production --gates data/quality-gates-production.json
```

The integration suite covers the plan's MVP examples plus provider injection,
batch APIs, persistence, multilingual segmentation, IPA features, patterns,
spam, duplicate detection, and indexed search behavior. A versioned seed
evaluation set (currently `0.5.0`, 1314 cases over train/validation/test)
with ranking and calibration metrics is available at
`data/evaluation/`; `data/quality-gates.json` pins the default-engine
bars and `data/quality-gates-production.json` the stronger production bars,
both calibrated on the held-out test split.

CI runs fmt, tests on Ubuntu/Windows/macOS, quality gates, coverage, clippy,
MSRV, and a nightly fuzz job. The repository includes fuzz targets for
Unicode, tokenization, and rebus parsing under `fuzz/` with a curated seed
corpus in `fuzz/corpus/`; run them locally with
`cd fuzz && cargo +nightly fuzz run <target> corpus/<target> -- -max_total_time=60`
(requires a nightly toolchain plus `cargo install cargo-fuzz`).
