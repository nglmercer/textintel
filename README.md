# textintel

A Rust library for multilingual text analysis, comparison, rebus decoding,
pattern matching, and local search. Analysis keeps independent evidence in a
`MessageFingerprint`, including the original text and the transformations used.
The default engine is deterministic, offline, and requires no model files.

Requires Rust 1.90 or later.

```rust
use textintel::TextIntelligence;

let engine = TextIntelligence::default();
let fingerprint = engine.analyze("Fra🏠do")?;
let comparison = engine.compare("c0mpr4 ah0r4", "compra ahora")?;
let candidates = engine.decode("salU2")?;

assert_eq!(fingerprint.raw, "Fra🏠do");
assert!(candidates.iter().any(|c| c.text.eq_ignore_ascii_case("saludos")));
# Ok::<(), textintel::TextIntelError>(())
```

## Library capabilities

- Unicode normalization, script detection, invisible/bidi checks, confusable
  skeletons, and visual similarity.
- Unicode-aware tokenization, lexical and character metrics, n-grams,
  deterministic language detection, transliteration, and rule-based phonetics.
- Multi-reading symbols, emoji and number evidence, bounded rebus decoding,
  and transformation spans.
- Rule-based entities, configurable pattern matching, explainable spam
  signals, and duplicate detection.
- Indexed in-memory search, versioned JSON persistence, optional redb and
  HNSW storage, and revision-aware caches.
- Provider interfaces for application-supplied embeddings, language tools,
  scoring, reranking, and storage.

Heuristic scores are evidence, not calibrated predictions. Resource packs are
starter dictionaries and shared speech resources; they are not complete dictionaries.
Embedding vectors are optional. The included feature-hash vectorizer is a
simple deterministic baseline, not a contextual encoder.

Concrete model inference, generation, decision models, training tools, and
trained artifacts have been removed from the library. Previous experiments
remain useful as [references](docs/EXPERIMENTS.md), not supported backends.
Applications can implement the provider interfaces in separate integrations.

## Text preparation for TTS

Use the versioned normalizer for speech preparation, then split the spoken text into bounded fragments:

```rust
use textintel::speech::{normalization as tn, segmentation};

let options = tn::NormalizeOptions::resolve(
    tn::NormalizationMode::Auto,
    "es",
    Some("es-PE"),
    tn::NormalizeLimits::default(),
    true,
)?;
let result = tn::normalize("Tengo 12 libros.", &options)?;
assert_eq!(result.spoken, "Tengo doce libros.");
let fragments = segmentation::segment_text(
    &result.spoken,
    &segmentation::SegmentPolicy::default(),
);
assert_eq!(fragments[0].text, result.spoken);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`speech::normalization` provides `tn-v3` rules and preparation identity,
with language-specific coverage and ambiguity warnings. Both speech APIs cover
the twenty base languages through shared numeric, calendar, lexical and Unicode
primitives. The typed API and CLI expose additional explicit interpretation
policies and diagnostics. Inspect unresolved readings before synthesis.
See [speech APIs and evaluation](docs/SPEECH.md) and
[measured coverage and remaining gaps](docs/COVERAGE.md).

## Configuration

Use `try_new` for configuration read from files or user input:

```rust
use textintel::{EngineConfig, TextIntelligence};

let engine = TextIntelligence::try_new(EngineConfig {
    max_input_length: 4096,
    max_batch_size: 32,
    max_search_candidates: 100,
    ..Default::default()
})?;
# Ok::<(), textintel::TextIntelError>(())
```

`TextIntelligence::production_local()` enables bounded caches, rule-based
phonetics, and deterministic feature-hash vectors. With the `phonetic-espeak`
feature it uses an installed `espeak-ng` binary when available. It does not
inspect model directories or model environment variables. Inspect
`engine.diagnostics()` for the actual providers and storage capabilities.
The [eSpeak runtime audit](docs/PHONETIC-COVERAGE.md) exercises the twenty language
families and their aliases, with documented input limits and no claim of native
pronunciation accuracy.

Providers and resource packs are explicit:

```rust
use textintel::{FeatureHashEmbeddingProvider, ResourceLoader, TextIntelligence};

let engine = TextIntelligence::builder()
    .production_local()
    .resources(ResourceLoader::from_resource_root("resources")?)
    .semantic_provider(FeatureHashEmbeddingProvider::default())
    .build()?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Features and CLI

The default build uses the lightweight local stack. Optional backends:

| Feature | Purpose |
|---|---|
| `phonetic-espeak` | Local `espeak-ng` phonetics with a subprocess timeout |
| `ann-hnsw` | HNSW retrieval over application-supplied vectors |
| `persist-redb` | Embedded persistent storage |
| `production-local` / `all` | Include the optional local backends above |

`--no-default-features` remains available. JSON persistence, provider
interfaces, feature hashing, and deterministic text utilities are core APIs.
The compatibility flags `lang-profile`, `semantic-local`, `phonetic-ipa`,
`semantic`, `phonetic`, `persist`, and `production-local-lite` remain available.

```sh
cargo build --locked
cargo run --locked --bin textintel -- analyze 'Fra🏠do' --json
cargo run --locked --bin textintel -- compare 'c0mpr4 ah0r4' 'compra ahora' --json
cargo run --locked --bin textintel -- decode 'salU2' --languages es,en
cargo run --locked --bin textintel -- diagnostics
cargo run --locked --bin textintel -- --help
```

The CLI also supports heuristic spam signals, duplicate checks, JSONL batch
analysis, resource inspection, dataset evaluation, and JSON-store indexing
and search. Run `<command> --help` for its accepted flags.

## Documentation and verification

- [API and extension points](docs/API.md)
- [Speech preparation and its experimental coverage](docs/SPEECH.md)
- [Resource-pack format](resources/README.md)
- [Bounds, caching, and benchmarks](docs/PERFORMANCE.md)
- [Changelog](CHANGELOG.md)

```sh
cargo fmt --check
cargo test --locked --no-default-features --all-targets
cargo test --locked --all-features --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo doc --locked --no-deps --all-features
cargo bench --locked --no-run --all-features
```

Evaluation data and optional quality targets live under `data/`. Some targets
currently exceed the heuristic baseline; they must not be interpreted as
verified library accuracy. See [evaluation limits](docs/PERFORMANCE.md).

MIT — see [LICENSE-MIT](LICENSE-MIT).
