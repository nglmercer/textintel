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
beam width, candidate count, symbol readings, recursion, and document count
are bounded by `EngineConfig`.

Optional feature names are available for packaging integrations:
`lang-profile`, `semantic-local`, `semantic-http`, `phonetic-ipa`, `persist`,
`semantic`, `phonetic`, `ml`, and `all`. The default build stays local and
lightweight; `semantic-http` is the explicit remote-provider adapter and
`semantic-local` provides the deterministic feature-hash embedding baseline.

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
cargo run -- evaluate data/evaluation.json
cargo run -- index store.json id-1 "a message"
cargo run -- search store.json "similar message" 5
```

## Verification

```text
cargo fmt --all -- --check
cargo test --locked --all-features --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo doc --locked --no-deps
```

The integration suite covers the plan's MVP examples plus provider injection,
batch APIs, persistence, multilingual segmentation, IPA features, patterns,
spam, duplicate detection, and indexed search behavior. A versioned seed
evaluation set with ranking and calibration metrics is available at
`data/evaluation.json`; the repository also includes fuzz targets for Unicode,
tokenization, and rebus parsing under `fuzz/`.
