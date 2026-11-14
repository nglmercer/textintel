# Library API

The public Rust API is re-exported from `textintel`. The generated API reference
(`cargo doc --no-deps --all-features`) describes individual types and methods.

## Construction and analysis

```rust
use textintel::{EngineConfig, TextIntelligence};

let engine = TextIntelligence::try_new(EngineConfig::default())?;
let fingerprint = engine.analyze("Fra🏠do")?;
let batch = engine.analyze_batch(&["hola".into(), "hello".into()])?;
assert_eq!(batch.len(), 2);
# Ok::<(), textintel::TextIntelError>(())
```

`new` panics for invalid programmer-supplied configuration; `try_new` and
`builder().build()` return errors. `MessageFingerprint` keeps raw text,
normalized views, tokens, language candidates, symbols, phonetic evidence,
rebus candidates, entity mentions, obfuscation signals, and optional vectors.
`analyze_with_timing` reports per-stage timings without embedding user text in
the timing structure.

## Comparison and decoding

- `compare`, `compare_batch`, and `compare_fingerprints` combine available
  evidence channels. Missing channels are omitted and weights renormalized.
- `duplicate` and `duplicate_with_mode` apply an explicit threshold to a
  duplicate mode. Modes include combined, near-exact, lexical, semantic,
  phonetic, decoded, and visual comparison.
- `decode` and `decode_with_languages` return bounded, ranked rebus candidates
  with scores and transformation evidence. Ambiguous readings are preserved.
- `SimilarityProfile` selects deterministic channel weights; an application
  can also implement `SimilarityScorer`.

Scores depend on the configured providers and resources. They are not a
universal probability of equivalent meaning.

## Patterns, spam signals, and search

`add_pattern` registers examples. `add_pattern_with_options` also accepts
negative examples, a threshold, language filters, tags, and selected channels.
Use `match_patterns` and `remove_pattern` to query/manage the registry.
`save_patterns_to` and `load_patterns_from` persist definitions and re-analyze
examples on load.

`detect_spam` returns heuristic features, labels, reasons, and an uncalibrated
score. It does not infer intent from a trained classifier.

`add_document`, `remove_document`, and `document_count` manage the configured
store. `find_similar` returns ranked results with retrieval metadata;
`find_duplicates` applies a duplicate threshold. Requested results remain
bounded by `max_search_candidates`.

`MemoryStore` is the default. `JsonFileStore` supports versioned persistence;
`RedbStore` is available with `persist-redb`. `ann-hnsw` adds HNSW retrieval for
supplied vectors. ANN graphs rebuild from stored fingerprints rather than
being persisted as implementation-specific graph files. Stored fingerprints
pass through explicit schema migration before use.

## Provider interfaces

Applications can implement and inject these interfaces:

| Interface | Purpose |
|---|---|
| `EmbeddingProvider` | Supply vectors for explicit text inputs |
| `G2PProvider` | Supply phonetic readings |
| `LanguageDetectionProvider` | Supply language candidates |
| `LemmatizerProvider` | Supply language-aware lemmas |
| `LexiconProvider` | Supply word, frequency, and stop-word lookups |
| `SymbolKnowledgeProvider` | Supply symbol readings and concepts |
| `AbbreviationProvider` | Supply language-scoped expansions |
| `TransliterationProvider` | Supply converted script views |
| `EntityProvider` | Supply entity mentions |
| `SimilarityScorer` | Supply application-specific comparison scores |
| `SpamPredictor` | Supply application-specific spam judgments |
| `RerankerProvider` | Reorder retrieved candidates |
| `VectorStore` | Supply searchable fingerprint storage |

The library includes null/static vector providers and a deterministic
`FeatureHashEmbeddingProvider`. Vector metadata identifies compatible spaces
and dimensions; it does not load or execute a model. Provider implementations
must preserve input order and report their actual capabilities. Vector counts,
dimensions, and non-finite values are validated before fingerprints use them.

`production_local()` enables local deterministic providers and bounded caches.
It never loads model files or consults model-path environment variables.
Optional espeak uses an installed local binary with bounded output and timeout.

## Speech normalization and segmentation

The recommended normalization API is `speech::normalization::normalize` with
`NormalizeOptions::resolve` or `resolve_with_style`. It provides
versioned `tn-v2.1` rules, including `precise`, `natural`, and `digits`
styles, explicit locales, resource budgets, protected spans, and warnings.
`NormalizedText` contains `spoken`, ordered original byte-offset `changes`,
`warnings`, and language-specific `coverage`. `SpeechText::prepare` retains
the same stable preparation identity. `language_coverage()` exposes the
supported language/locale and feature matrix; this is a rules coverage
declaration, not measured linguistic accuracy.

Normalize the complete input before calling
`speech::segmentation::segment_text(&spoken, &SegmentPolicy::default())`.
Fragments carry `index`, `total`, and `text`; the default limit is 200 Unicode
scalar values, configurable with `SegmentPolicy::bounded(max_chars)`.
Segmentation can canonicalize whitespace at boundaries. It prefers protected
sentence boundaries and whitespace, then approximate grapheme boundaries;
a cluster exceeding the hard character limit can be split. No audio or model
runtime is involved.

The separate `speech::{parse_for_speech, normalize_for_speech,
verbalize_document}` APIs are **experimental**, with explicit typed spans for
20 base-language reading conventions. `SpeechOptions` selects the language,
date order, decimal separator, and budgets. `SpeechDocument` retains the
source text and byte offsets. `SpeechNormalization.readings` marks unsupported
or ambiguous spans with `supported: false` and a reason, preserves their
source, and sets `fully_supported: false`. That flag reports implementation
support; it does not certify pronunciation quality. Plain text is preserved
without similarity normalization, case folding, or rebus decoding.

The versioned normalizer preserves ambiguous forms and reports warnings;
languages without rules pass through with `none` coverage. In both APIs,
callers should inspect diagnostics and choose whether to preserve, review, or
reject unresolved input before synthesis. The experimental parser never
silently selects USD for a bare `$` or guesses ambiguous slash-date order.

The current speech CLI uses the experimental typed APIs:

```sh
textintel parse-speech 'El 2024-11-13 pagué S/ 12.50.' --locale es --json
textintel normalize-speech '03/04/2024' --locale es --date-order dmy --json
textintel eval-speech data/speech/evaluation.json --json
textintel eval-speech data/speech/reference-nemo.json --json
```

`eval-speech` returns a nonzero status when annotated expectations fail;
failures are expected on the external NeMo reference corpus. These commands
do not invoke the versioned rule-based normalizer. See [speech coverage,
corpus policy, and API examples](SPEECH.md).

## Resources, configuration, and errors

Load custom packs with `ResourceLoader::from_resource_root` and attach them
with `with_resources` or `EngineBuilder::resources`. See the
[resource format](../resources/README.md) for schema and provenance rules.

`EngineConfig`, `CacheLimits`, `SimilarityWeights`, `RebusWeights`, and
`ResourceLimits` bound work and resource usage. Failures use `TextIntelError`;
application-provider failures carry `ProviderError`. Inspect
`provider_capabilities`, `diagnostics`, and `resource_manifest` for the actual
configuration, revisions, cache counts, and storage capabilities.

## CLI and evaluation

`textintel --help` lists the supported library commands. Commands accept
`--json` for structured output; `schema-version` reports API/fingerprint
versions. Run `<command> --help` for command-specific options. Model-path,
training, generation, and decision-model commands have been removed.

The `evaluation` module loads labeled datasets, scores comparisons, language
identification, decoding, and retrieval, and checks optional quality targets.
See [evaluation limits](PERFORMANCE.md#evaluation-limits) before interpreting
those targets as guarantees.
