# Designed semantic-value regression corpus

`semantic-values.json` contains 660 designed cases: 33 per locale for all twenty
members of `SpeechLocale::ALL`. It annotates 640 non-Text spans, including 100
RFC 3339 timestamp cases. This is an
authored regression matrix with shared structural templates, not an independent
held-out benchmark, a representative production sample, or native pronunciation
validation. There are no spoken-form golds.

The expectations were authored before running the parser or evaluator. Literal
input/value pairs were expanded across the locale matrix; only native decimal
digit transcription and UTF-8 byte lengths were calculated mechanically. No
parser output or external reference gold was used to construct
the expected values.

All cases use the default Precise reading style, Phonetic number notation,
Auto interpretation, Strict clock policy, Preserve invalid-date policy,
`decimal_separator: "."`, and `date_order: "reject_ambiguous"`. No evaluation
policy supplies category hints, regional conventions, or inferred separators.

## Annotated contracts

Each locale has the following twelve contrast pairs:

| Contrast | Inputs | Expected distinction |
|---|---|---|
| Decimal sign | `12.0010` / `-12.0010` | Exact negative flag; fraction `0010` retained |
| Signed decimal zero | `0.000` / `-0.000` | Negative zero remains distinct in source-preserving IR |
| Written precision | `12.000` / `12.0000` | Three versus four fractional zeros |
| Magnitude | `12.500` / `125.00` | Integer and fractional strings both differ |
| Asset identity | `USD 2.00` / `EUR 2.00` | USD versus EUR; no symbol/country inference |
| Mass sign | `2.000 kg` / `-2.000 kg` | Canonical `mass-kilogram`, signed exact amount |
| Calendar validity | `2024-02-29` / `2023-02-29` | Gregorian leap date versus whole ambiguous invalid date |
| Written AM/PM | `03:15am` / `03:15pm` | Precise compact Time hours 3 versus 15 |
| Explicit seconds | `09:05` / `09:05:00` | `second: null` versus `second: 0` |
| Fractional-clock precision | `03:15:01.001` / `03:15:01.0010` | FractionalClock strings `001` versus `0010`; no compact projection |
| Integer padding | `0` / `000` | Cardinal zero versus digit reading with retained padding |
| Decimal-digit magnitude | Native decimal `42` / `43` | Canonical ASCII integer values, original source retained |

The original cases per locale cover a number embedded in Unicode prose,
the corresponding backtick-protected numeric text, protected structured JSON,
and ordinary prose with no structured spans. The code and JSON cases explicitly
expect Identifier values, not an empty list. Only plain prose expects
`expected_values: []`. Four further cases per locale cover RFC 3339 UTC,
fractional numeric offsets, unknown local offsets, verified leap seconds, and
year zero.

The digit matrix uses Devanagari for Hindi/Marathi, Bengali for Bengali,
Arabic-Indic for Arabic, extended Arabic-Indic for Urdu, Gurmukhi for Punjabi,
Telugu for Telugu, Tamil decimal digits for Tamil, and fullwidth decimal digits
for Chinese/Japanese/Korean. The other locales use ordinary ASCII decimal
digits. Fullwidth digits are a typographic Unicode input form, not a claim of
kanji/Han/Hangul number grammar or pronunciation. No glyph normalization is
applied to the annotated source or its byte offsets.

## Expectation provenance and limits

The gold describes explicit numeric notation and the public lossless IR contract:
exact decimal strings, explicit ISO currency codes and SI mass symbol, Gregorian
calendar validity, and written twelve-hour clock periods. These are designed
semantic expectations, not strings copied from the implementation's predictions.
The invalid-date reason is the documented current diagnostic `invalid calendar
date`; this case must remain unresolved rather than being repaired or spoken as
a valid calendar date.

`expected_values` contains every non-Text SpeechSpan, including its complete
SpeechValue, source, and UTF-8 offsets. Twenty `fraction-four` cases intentionally
omit `expected_spans` to exercise semantic-only evaluation. The other 640 cases
also annotate the existing span/type contract. This does not retroactively add
value validation to the historical `parse_exact` metric.

These cases do not evaluate natural-language inflection, regional reading
preferences, G2P quality, pronunciation or ambiguous date-order resolution.
Timestamp repetition across languages verifies the common API contract; it
does not make 660 independent linguistic observations. Native and
independently collected evaluation data remain separate work.

Equality here means exact IR under these declared options. It is not an
option-agnostic equivalence between Natural and Precise representations. In
particular, scientific notation, fractions, ranges, durations, ordinals and
composed localized readings need another explicitly selected profile and their
own designed gold; timestamp entries have no spoken-form gold.

The tests also substitute deliberately wrong signs, magnitudes, precision,
assets, clock fields and protected-span expectations in temporary in-memory
cases. Those counterexamples must fail the semantic gate even when their
span/type expectations still pass. They never rewrite the committed corpus.

## Rendering support is separate

The initial evaluation also exposed a rendering gap: Arabic `EUR 2.00` had the
correct typed value but unsupported money grammar. The library now supports the
reviewed one/dual euro forms through the shared count helper; the committed
semantic gold was not changed. Full-name paradigms remain separately bounded.
The [core audit](../../docs/SPEECH-CORE-AUDIT.md) records the primary sources.

The current matrix completes 600 of 660 renderings. The remaining 60 are the
intentionally preserved invalid dates, backtick code and structured JSON (twenty
each). Semantic agreement counts recognition of those preserved values; it does
not count them as supported utterances. The corpus still has no spoken gold.

## Reproduce

```sh
cargo test --locked --test speech_semantic_values
cargo run --locked --no-default-features --bin textintel -- eval-speech data/speech/semantic-values.json --json
```

Inspect semantic cases/exact/expected_spans/predicted_spans/correct_spans
separately from spoken exact/variant counts, rendering support, and source-code
coverage. A disagreement should be investigated and reported; expected values
must not be replaced with parser output merely to make this matrix pass.
