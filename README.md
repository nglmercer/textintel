# textintel

`textintel` is a local-first Rust library for multilingual text intelligence.
It keeps independent evidence channels in a `MessageFingerprint` instead of
forcing a single interpretation of a message.

The Rust crate is the primary implementation. The original Python MVP remains
in `textintel/` as a migration reference while downstream users move to the
typed Rust API.

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
- Multi-reading symbols and numbers, bounded beam-search rebus decoding,
  uncertainty-preserving spoken candidates, and obfuscation features.
- A deterministic rule-based G2P fallback, phoneme edit/feature distance, and
  optional semantic embedding and reranker providers.
- Pattern registration, spam signals, duplicate checks, an in-memory search
  store, and a `VectorStore` boundary for larger backends.

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
`semantic`, `phonetic`, `ml`, and `all`. The core intentionally does not pull
large model runtimes into the default build.

## CLI

```text
cargo run -- analyze "Fra🏠do" --json
cargo run -- decode "salU2"
cargo run -- compare "c0mpr4 ah0r4" "compra ahora" --json
cargo run -- spam "ganaste un premio https://example.test"
```

## Verification

```text
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-features
```

The integration suite covers the plan's MVP examples plus provider injection,
patterns, spam, duplicate detection, and search behavior. A small positive /
hard-negative evaluation set is available at `data/evaluation.json`.
