# Speech text preparation

TextIntel provides two independent speech paths. Use the versioned
`speech::normalization` API for rule-based text preparation. Use the experimental
typed API when inspecting exact source spans or developing additional language rules. Both run offline, without
loading a model. The analysis/rebus pipeline is separate from speech
preparation.

## Recommended normalization API

`speech::normalization` provides versioned `tn-v2.1` rules, reading styles,
ambiguity warnings, resource budgets, and a stable preparation identity.
Rule support varies by language and written form. Third-party copyright
notices are retained in the [MIT license](../src/speech/normalization/LICENSE).

```rust
use textintel::speech::{normalization as tn, segmentation};

let options = tn::NormalizeOptions::resolve_with_style(
    tn::NormalizationMode::Auto,
    "es",
    Some("es-PE"),
    tn::NormalizeLimits::default(),
    true, // Return ambiguity warnings.
    tn::ReadingStyle::Natural,
)?;
let prepared = tn::normalize("Tengo 12 libros.", &options)?;
assert_eq!(prepared.spoken, "Tengo doce libros.");

// Review prepared.warnings and prepared.coverage before synthesis.
let fragments = segmentation::segment_text(
    &prepared.spoken,
    &segmentation::SegmentPolicy::bounded(200),
);
assert!(fragments.iter().all(|fragment| fragment.char_count() <= 200));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Options resolve a base speech language and an explicit supported locale.
`Precise` is the default reading style; `Natural` and `Digits` are opt-in.
`Off` mode returns byte-identical text. Input/output character budgets and an
expansion ratio bound normalization. `NormalizedText.changes` refers to
original UTF-8 byte offsets, preserving text outside replacements.

`language_coverage()` returns the actual locale and feature declarations.
Rules cover Spanish (ES/MX/PE), English (US/GB), Portuguese (BR/PT), French,
German, Italian, Russian, Chinese (CN/TW), Japanese, and Korean. The feature
families include numbers, decimals, ordinals, percentages, currency, dates,
times, units, abbreviations, ranges, and fractions. Scientific notation,
RFC3339 timestamps, and natural-style attached durations also have rules.
Coverage differs by language and written form; inspect the returned matrix.

Ambiguous forms remain unchanged and generate warnings when enabled.
Unsupported languages pass through with `none` coverage. Protected regions
include structured identifiers, URLs, email, code, and tags; JSON objects and
arrays are preserved in auto mode. Preserving a token does not make its
pronunciation safe for every TTS engine. Applications decide whether a
warning or coverage gap requires review, rejection, or a domain-specific
reading.

`SpeechText::prepare` is a lightweight wrapper around this preparation step.
Its identity includes ruleset, mode, language, locale, and reading style. It
can help callers distinguish prepared text, but it does not perform synthesis.

## Segmentation after normalization

`speech::segmentation::segment_text` produces ordered `Fragment` values with
`index`, `total`, and `text`. `split_sentences` and
`split_sentences_with_limit` return plain strings. The default limit is 200
Unicode scalar values (`str::chars`), independent of UTF-8 byte length.
Normalize the whole input first so fragments cannot divide a written number,
date, or other expression before its classification.

Sentence boundaries protect decimals, versions, URLs, abbreviations, code,
and tags. Oversized sentences split at whitespace, then at approximate
grapheme boundaries. A grapheme longer than the configured limit is split
to preserve the hard bound; a protected token longer than the limit can also
be split. Fragment boundaries may canonicalize whitespace, so fragments are
not a byte-lossless representation of the input. Non-whitespace content
remains ordered. Empty input produces no fragments; a zero limit is treated
as one character. These functions prepare text fragments only.

For byte-offset sentence ranges without fragment whitespace handling, use
`speech::normalization::segments::sentence_spans`. The same module exposes
`clusters` and `hard_split` for custom segmentation.

## Experimental typed parsing and verbalization

The following API has a separate result model and reading policy. It does
not replace the versioned normalization API:

```rust
use textintel::speech::{
    DateOrder, SpeechLocale, SpeechOptions, normalize_for_speech, parse_for_speech,
};

let mut options = SpeechOptions::new(SpeechLocale::Es);
options.date_order = DateOrder::DayMonthYear;
options.decimal_separator = Some(',');
let document = parse_for_speech("La fecha es 03/04/2024.", &options)?;
let spoken = normalize_for_speech(&document.text, &options)?;
assert_eq!(spoken.text, "La fecha es tres de abril de dos mil veinticuatro.");
# Ok::<(), textintel::TextIntelError>(())
```

`parse_for_speech` returns a `SpeechDocument`: the unchanged input and
contiguous `SpeechSpan` values with UTF-8 byte offsets, source strings, and
typed values. Values distinguish text, numbers, dates, times, currencies,
percentages, measures, identifiers, and ambiguous expressions.
`normalize_for_speech` parses and verbalizes; `verbalize_document` accepts an
already parsed document and rejects invalid externally supplied spans.
`verbalize_number` and `cardinal_max` expose numeric rendering and its range.

The 20 base-language conventions are experimental:

| Language family or group | Available conventions |
|---|---|
| Germanic, Romance, and Slavic | English, German, Spanish, French, Portuguese (BR), Russian |
| Indo-Aryan | Hindi, Bengali, Urdu, Punjabi (Gurmukhi), Marathi |
| Dravidian | Tamil, Telugu |
| Semitic | Arabic |
| Other language families | Chinese (simplified), Japanese, Korean, Indonesian, Turkish, Vietnamese |

These labels do not imply support for every regional variant or script.
Accepted regional aliases map to the corresponding base convention; they do
not select additional grammars. Unsupported tags are rejected rather than
silently substituted. Italian is available in the versioned normalization API, not in
this experimental set.

| Feature family | Experimental behavior and limits |
|---|---|
| Integers and decimals | Basic cardinals and digit readings across the listed languages; ranges vary by language. Fractional digits retain zeros. This is not complete grammatical number agreement. |
| Dates and times | Gregorian dates and 24-hour clocks with selected localized readings. Slash dates require an explicit order when ambiguous. Arabic date grammar remains unsupported. |
| Money | Explicit codes and safely recognized symbols; selected currency names and precision. English/Spanish include selected minor-unit readings. Arabic currency grammar and unhandled forms remain unsupported. |
| Percentages and measures | Localized readings for selected units, with agreement where implemented. Unit, gender, and large-quantity combinations can be unsupported. |
| Identifiers and ambiguity | Protected identifiers and unresolved expressions retain their original source and receive unsupported diagnostics. They are not silently read as ordinary counts. |

`SpeechOptions::new` defaults to rejecting ambiguous date order and leaving
the decimal separator unspecified. Set these policies for the actual source
format; do not infer them from the machine's locale. A bare `$` or `¥` does
not choose a currency. Negative signs, leading zeros, fractions, native
digits, and grouping conventions need their own cases in application data.

Every `SpeechReading` includes `supported` and an optional `reason`.
Unhandled structured values preserve their original source and make
`fully_supported` false. Ordinary text passes through. `fully_supported`
means the implementation produced every requested structured reading, not
that native speakers validated the entire result. No speech path applies
case folding, leetspeak, or rebus transformations to spoken input.

## CLI and evaluation

The current speech CLI exercises the experimental typed API. It does not
invoke the versioned rule-based normalizer:

```sh
cargo run --locked --no-default-features --bin textintel -- \
  parse-speech 'El 2024-11-13 pagué S/ 12.50.' --locale es --json
cargo run --locked --no-default-features --bin textintel -- \
  normalize-speech '03/04/2024' --locale es --date-order dmy --json
cargo run --locked --no-default-features --bin textintel -- \
  normalize-speech '1.234,50 EUR' --locale es --decimal-separator comma --json
cargo run --locked --no-default-features --bin textintel -- \
  eval-speech data/speech/evaluation.json --json
cargo run --locked --no-default-features --bin textintel -- \
  eval-speech data/speech/reference-nemo.json --json
```

The in-repository corpus checks exact spans and independently annotated
spoken strings where available. The [NeMo-derived external
reference](../data/speech/reference-nemo.README.md) deliberately exercises
broader forms and conventions. **Failures on that reference are expected**
and identify gaps; they should not be hidden or rewritten merely to match
the implementation. `eval-speech` returns status 1 when expectations fail.

The report separates parse cases from cases with spoken gold, and includes
span counts for precision/recall, exact matches, locale/category slices,
unresolved reasons, and failed cases. Unsupported and erroneous results remain in the
applicable denominators. Source-code coverage measures executed code;
it does not measure linguistic accuracy. A passing regression test suite
demonstrates captured behavior, not correctness on all languages or inputs.

The [coverage snapshot and gap analysis](COVERAGE.md) records measured line
coverage, regression results, and strict external spoken agreement.

## Improving accuracy

Build a versioned, held-out corpus from the application's real domains and
source locales. Add independently reviewed complete spoken gold, including
native-speaker judgments of gender, case, counters, year readings, currency
subunits, and alternative valid forms. Keep structural-only cases distinct
from cases with spoken gold. Split near-duplicate templates across training
and evaluation sets carefully to avoid measuring memorized formatting.

Add minimal pairs for ambiguous dates, separators, locale-sensitive symbols,
identifiers versus quantities, and leading zeros. Include negative cases:
invalid calendars and clocks, unsupported precision, mixed scripts, units
without whitespace, code, URLs, malformed structured input, and resource
limits. Expand named-entity, abbreviation, fraction, range, scientific,
duration, and timestamp cases before claiming those families for the typed
API. Preserve accepted normalization behavior in regression fixtures when
adding new rules.

Report exact spoken accuracy and unresolved/error rates by language, domain,
and feature family. Publish corpus provenance, accepted variants, sample
counts, and regression results with each ruleset change. Add code coverage
to find untested branches, then add independent gold to establish whether
those branches speak correctly. No global accuracy guarantee follows from
the number of supported locale names.
