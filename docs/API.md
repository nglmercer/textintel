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

`detect_spam` returns explainable features, labels, reasons, and a calibrated
probability from the baked logistic model trained on the bundled v2 spam corpus
train split. Applications can substitute their own `SpamPredictor`.

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
It resolves regional tags through their curated language family; this does not
select a certified regional accent. `EspeakNgG2PProvider::for_voice` explicitly
overrides voice selection, and the instance advertises only that voice's known
family. Tool metadata is removed before IPA tokenization, and a foreign-language
switch reduces its uncalibrated confidence marker. The
[twenty-language runtime audit](PHONETIC-COVERAGE.md) records actual execution
and the backend's Japanese/Arabic input limits separately from native accuracy.

## Speech normalization and segmentation

The recommended normalization API is `speech::normalization::normalize` with
`NormalizeOptions::resolve` or `resolve_with_style`. It provides
versioned `tn-v4` rules, including `precise`, `natural`, and `digits`
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
verbalize_document, verbalize_document_with_options}` APIs are **experimental**,
with explicit typed spans for twenty base-language reading conventions.
`SpeechOptions` selects date order (including year/month/day), decimal separator,
`ReadingStyle`, phonetic/orthographic `NumberNotation`, and input/span budgets.
`clock_policy` defaults to `ClockPolicy::Strict`: explicit AM/PM hours must be
1–12. Opt-in `ClockPolicy::NormalizeRedundantPeriod` accepts coherent redundant
24-hour notation, converting 13–23 PM to 1–11 PM and 0 AM to 12 AM. Contradictory
periods and out-of-range hours remain unresolved. This is an explicit TTS source
convention, not a claim that nonstandard notation is universally invalid.
Serialized policy values are `strict` and `normalize_redundant_period`.
Rendering always validates canonical clock fields; the tolerant source policy
does not allow a caller-constructed AM/PM clock with hour 13.
Natural rendering retains a `TemporalValue::Clock` with hour 1 and `pm` for
`13:10pm`; Precise may project the same time to `SpeechValue::Time` at 13:10.
The 1–12 constraint applies to clock fields carrying AM/PM, not 24-hour `Time`.
`TemporalValue::FractionalClock { clock, fraction }` retains a complete boxed
clock and 1–128 canonical fraction digits as a string. It is a `time` span and
cannot project to compact `Time`; nested wrappers, incomplete inner clocks and
malformed precision are rejected. Dot fractions are recognized automatically;
comma fractions require `SpeechInterpretation::Time`. Fractional seconds use
literal Precise number reading while the surrounding clock keeps the selected
style and notation. Complete timestamps use the separate
`TemporalValue::Timestamp` with the explicit `SpeechInterpretation::DateTime`
hint. The parser accepts RFC 3339's fixed date/time/offset form, including
lowercase `t`/`z`, up to 128 fractional digits, and keeps the source wall
time without timezone conversion. Four-digit year zero is valid in this
timestamp grammar. `TimestampOffset::UnknownLocal` preserves
the `-00:00` distinction from UTC or known numeric offsets. Malformed or
over-limit timestamps and IXDTF annotations stay atomic and unresolved.
Positive leap seconds are accepted against the bundled IERS/NIST table through
2016 (checked through IERS Bulletin C 72, July 2026); a later IERS announcement
requires refreshing that table. Protected structured inputs remain protected.
`invalid_date_policy` defaults to `InvalidDatePolicy::Preserve`, which retains
an impossible Gregorian date's source and diagnostic. Explicit
`InvalidDatePolicy::ReadComponents` can read an atomic date's known components
without declaring a valid date. The IR is
`TemporalValue::InvalidCalendarDate`, with evaluation kind `invalid_date`;
it is distinct from valid `Date` values. Eligibility requires resolved date
order, month 1–12, day 1–31, and a positive supported year when present. The
option does not guess order, accept month 13/year 0, or partially read an invalid
date list or calendar sequence. Invalid compound calendar input remains whole.
JSON uses `preserve|read_components`; the CLI uses `preserve|read-components`.
`currency_naming` defaults to `CurrencyNaming::Style` and retains the reading style's
established currency wording. Opt-in `CurrencyNaming::Full` retains localized
qualifiers from the shared CLDR names, such as “US dollars.” It changes naming,
not symbol-to-ISO interpretation. The lexical path exists for all twenty
languages; a display name alone does not establish Arabic count agreement.
Unknown Arabic noun/qualifier paradigms remain unsupported, with source text
and diagnostics preserved.
A `SpeechInterpretation` hint expresses caller-known intent such as number,
date, time, datetime, measure, currency or duration; it is not inferred from a test ID.
Symbol-to-ISO currency mappings and bounded unit aliases supply application
conventions. Invalid options return an error before parsing or rendering.

`speech::locales::for_locale` exposes concrete regional convention records used
by versioned option resolution and the normalization adapters. These distinguish
en-US/en-GB dates and bare-dollar policy, while retaining the separate historical
CJK policy for year-final slash dates. A known versioned locale does not expand
the experimental typed renderer's accepted regional aliases.

Both front ends share scientific, duration, fraction and range source grammars
with explicit acceptance profiles. Their padding, sign, notation and size limits
can differ while offsets and exact numeric values use the same core. Versioned
fraction/duration adapters check the rendering budget in addition to `u64`
representability; oversized values are preserved or diagnosed.
During versioned `normalize`, the private `LexicalSource` shares one original
code/tag protection map between legacy classification and typed augmentation.
`Off` and structured JSON handling skip this scan. Semantic classification still
has two compatibility dispatch paths; sharing protection metadata does not merge
their interpretation or presentation policies.

`SpeechDocument` retains the complete source and original UTF-8 byte offsets.
Parse once and pass the document plus the same options to
`verbalize_document_with_options` to avoid a second parse. This method validates
source ranges, language and budgets; callers should not construct forged spans.
`SpeechNormalization.readings` marks unsupported or ambiguous spans with
`supported: false` and a reason, preserves their source, and sets
`fully_supported: false`. That flag reports implementation support; it does not
certify pronunciation quality. Plain text and protected code/tags are preserved.
Inspect `reason` independently of `supported`: an explicitly requested literal
invalid-date reading has `supported: true` and
`reason: Some("invalid calendar date")`. Reading components successfully does
not validate their calendar meaning. Rendering a caller-constructed invalid
ordinary `Date` still fails; the opt-in applies to the distinct literal IR.
The shared pronunciation dictionary is bounded and does not recursively expand
replacement text. See [the shared core and language audit](SPEECH-CORE-AUDIT.md)
for the actual capability matrix and remaining migration work.

The versioned normalizer preserves ambiguous forms and reports warnings;
languages without rules pass through with `none` coverage. In both APIs,
callers should inspect diagnostics and choose whether to preserve, review, or
reject unresolved input before synthesis. The experimental parser never
silently selects USD for a bare `$` or guesses ambiguous slash-date order.

The current speech CLI uses the experimental typed APIs:

```sh
textintel parse-speech 'El 2024-11-13 pagué S/ 12.50.' --locale es --json
textintel normalize-speech '03/04/2024' --locale es --date-order dmy --json
textintel normalize-speech 'US$7' --locale en --reading-style natural --currency-naming full --json
textintel normalize-speech '13:10pm' --locale zh --reading-style natural --clock-policy normalize-redundant-period --json
textintel normalize-speech '30/02/1990' --locale es --reading-style natural --invalid-date-policy read-components --json
textintel normalize-speech '३१-०६' --locale hi --reading-style natural --invalid-date-policy read-components --interpret-as date --json
textintel eval-speech data/speech/evaluation.json --json
textintel eval-speech data/speech/reference-nemo.json --json
textintel eval-speech data/speech/reference-nemo.json \
  --speech-policy data/speech/reference-policy.json --accept-variants --json
```

`--currency-naming style|full` selects the typed currency naming option; its
default is `style`.
`--clock-policy strict|normalize-redundant-period` selects source clock handling;
its default is `strict`. The CLI uses hyphens while JSON uses snake_case.
`--invalid-date-policy preserve|read-components` defaults to `preserve`.
The Hindi yearless pair requires date context in this example: Natural `Auto`
can classify `३१-०६` as a range. A reading policy does not supply that context.

`eval-speech` returns a nonzero status when annotated expectations fail;
failures are expected on the external NeMo reference corpus. Reports keep literal
`spoken_exact`, separately reviewed `spoken_variant`, and their sum
`spoken_accepted`. The `--accept-variants` flag changes the exit gate and retains
all strict failures and original gold. Explicit corpus policy supplies reading
intent, so its metrics must be reported separately from automatic parsing.
`SpeechEvalReport.diagnostics` retains every reading with a reason, including
supported readings and exact matches absent from `failures`. Each
`SpeechCaseDiagnostic` carries `case_id`, `locale`, `kind`, `start`, `end`,
`source`, `spoken`, `reading_supported`, and `reason`.
`diagnostics_by_reason` counts all of these records; `unresolved_by_reason`
continues to count only unsupported readings. Missing diagnostic fields in an
older report JSON deserialize to an empty vector/map. Text evaluation output
includes `diagnostics` and `supported_with_diagnostic` counts alongside the
comparison metrics. A separate literal-date profile does not add passes to the
unchanged strict reference policy.
These commands do not invoke the versioned rule-based normalizer. See [speech coverage,
corpus policy, and API examples](SPEECH.md).

## Resources, configuration, and errors

Load custom packs with `ResourceLoader::from_resource_root` and attach them
with `with_resources` or `EngineBuilder::resources`. Custom loaders preserve
their supplied scope; call `add_speech_resources()` explicitly to fill missing
speech-derived packs using shared data. The embedded loader applies this bridge
automatically. See the [resource format](../resources/README.md) for schema and provenance rules.

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
