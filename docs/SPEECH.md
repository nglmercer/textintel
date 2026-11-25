# Speech text preparation

Speech parsing and normalization share exact numeric values, a language registry,
Unicode boundaries, protected regions, calendar validation, and lexical resources.
Neither entry point loads a model or applies rebus, leetspeak, case-folding, or
similarity rewrites to spoken input.

## Parse once, then render and segment

```rust
use textintel::speech::{
    parse_for_speech, verbalize_document_with_options, ReadingStyle,
    SpeechLocale, SpeechOptions, segmentation,
};

let mut options = SpeechOptions::new(SpeechLocale::Es);
options.reading_style = ReadingStyle::Natural;
let document = parse_for_speech("Tengo 12 libros.", &options)?;
let prepared = verbalize_document_with_options(&document, &options)?;
assert_eq!(prepared.text, "Tengo doce libros.");
let fragments = segmentation::segment_text(&prepared.text, &Default::default());
assert_eq!(fragments[0].text, prepared.text);
# Ok::<(), textintel::TextIntelError>(())
```

`normalize_for_speech(text, &options)` combines the same parse and rendering
steps. `verbalize_document` uses default options for an already parsed document;
`verbalize_document_with_options` retains explicit reading conventions. Public
span maps and option limits are validated before indexing or rendering.

`SpeechDocument` preserves original UTF-8 byte offsets and source strings. The
semantic values distinguish ordinary text, numbers, dates/clocks, partial and
named calendar forms, currencies, percentages, units, labelled data, measured
expressions, scientific notation, fractions, ranges, ordinals, durations,
explicit inflections, identifiers, and unresolved ambiguity. Signed vulgar
fractions keep the sign in their numeric value. Explicit scalar-number intent
does not accept dimensional durations. Automatic parsing preserves phone-shaped
ASCII-hyphen tokens and bounded common filenames before numeric grammars.

Each `SpeechReading` contains `supported` and an optional reason. An unresolved
structured expression remains byte-identical in the output. `fully_supported`
reports whether requested structured readings were implemented; it is not a
native-speaker accuracy certificate. Ordinary prose can pass through unchanged.
`reason` is not limited to unsupported readings: a literal invalid-date reading
can be supported and still retain `Some("invalid calendar date")`. Consumers
must inspect both fields before treating the result as validated calendar data.

## Explicit conventions

| Option | Purpose |
|---|---|
| `locale` | One of the twenty base-language reading conventions |
| `date_order` | DMY, MDY, YMD, or reject ambiguous numeric dates |
| `invalid_date_policy` | `preserve` (default), or explicit `read_components` for eligible atomic invalid dates |
| `clock_policy` | `strict` (default), or opt-in `normalize_redundant_period` for coherent redundant AM/PM notation |
| `decimal_separator` | Explicit separator, or conservative inference when absent |
| `reading_style` | `precise`, `natural`, or `digits` |
| `abbreviation_policy` | `contextual` (default), `preserve`, or explicit `bibliographic` forms |
| `number_notation` | Pronunciation-oriented or orthographic number forms, including Japanese kana/kanji |
| `interpret_as` | Caller-supplied context: auto, number, date, time, datetime, currency, measure, or duration |
| `currency_symbols` | Explicit per-symbol ISO assignments; absent mappings preserve uncertainty |
| `default_currency` | A caller-selected fallback for ambiguous currency symbols when no symbol map is supplied |
| `currency_naming` | `style` (default) retains the reading style's wording; opt-in `full` retains localized currency qualifiers |
| `unit_aliases` | Opt-in written aliases mapped to supported canonical unit keys |
| `max_input_chars`, `max_spans` | Positive resource limits shared by parsing and public-document rendering |

A language alone does not determine whether `03/04` is a date, `1.00` is a clock,
`007` is a count, or `f9` is a temperature or identifier. Hints are explicit
consumer intent, analogous to a TTS `say-as` instruction. They are not inferred
from evaluation IDs or expected speech. A measure hint accepts dimensionless
percentages. A number hint can explicitly treat padded digits as a count;
automatic mode retains identifier distinctions.

`AbbreviationPolicy::Contextual` expands exact catalog spellings only when the
locale-specific context matches. `Preserve` disables expansion;
`Bibliographic` opts into publication-reference forms such as Arabic
`د.ت.`/`د.م.` and Urdu `ص` followed by a number. Every expansion is a typed
`SpeechValue::Abbreviation` tied to a catalog ID, source spelling, locale, and
policy; rendering rejects forged or mismatched values. The catalog contains 44
spellings across 19 locale codes, including 20 preserved legacy forms and 24
new institutionally sourced forms. Italian entries serve the versioned adapter;
Chinese and Telugu have no entries. These counts describe available data, not
language completeness or measured accuracy. Chat substitutions remain separate.

Use `segmentation::split_sentences_with_options` or
`segmentation::segment_text_with_options` when an abbreviation's period should
remain inside its context before chunking. The older `sentence_spans` and
`split_sentences_with_limit` functions retain their historical boundaries for
compatibility. Neither API rewrites the source during segmentation.

`ClockPolicy::Strict` requires explicit AM/PM hours in 1–12.
`ClockPolicy::NormalizeRedundantPeriod` optionally normalizes nonstandard source
notation: 13–23 PM becomes 1–11 PM, and 0 AM becomes 12 AM. The source string and
byte offsets remain intact; the parsed clock stores canonical fields. Conflicting
periods, 24 with AM/PM, and out-of-range minute/second fields remain unresolved.
This convention permits a consumer to prepare coherent redundant notation for
TTS without declaring it standard clock notation. Public-document rendering
still validates canonical clock values even when the option is enabled.
For `13:10pm`, Natural keeps `TemporalValue::Clock` with hour 1 and `pm`;
Precise may use the equivalent 24-hour `SpeechValue::Time` at 13:10. Canonical
AM/PM clock fields use 1–12, while `Time` retains its 24-hour representation.
The renderer keeps the established AM/PM wording in English, Spanish and Chinese.
Other languages use the existing 24-hour clock grammar, so an explicit PM value
cannot silently become the same spoken time as AM. Twelve AM maps to hour 0 and
twelve PM to hour 12; this conversion never advances a date. Spoken register and
pronunciation still require independent language validation.
Compact IR projection uses shared validated fields and preserves temporal
metadata by retaining `TemporalValue` for labelled dates, partial dates, period
prefixes and half-hour forms. A clock with no written minutes does not acquire
zero minutes during rendering.
Serde uses `strict` and `normalize_redundant_period`; the CLI spelling is
`--clock-policy strict|normalize-redundant-period`.

Floating clocks also support exact fractional seconds: `03:15:01.0010` produces
`TemporalValue::FractionalClock`, with the complete clock and the string
`"0010"` retained separately. It never projects to compact `Time`, which has no
fraction field. All twenty languages reuse the same clock, number and quantity
grammars. Only fractional seconds force a literal Precise reading, so leading
and trailing zeros survive Natural and Digits styles too.

The profile accepts one/two-digit hours and two-digit minutes/seconds within
the numeric clock bounds, plus 1–128 fraction digits. Unicode decimal digits
are canonicalized through the shared core. A dot is recognized automatically; a comma requires explicit Time intent:

```sh
textintel normalize-speech '03:15:01,0010 pm' --locale en --interpret-as time --json
```

Without Time intent, a comma may separate independent data and is not assumed to
be a fractional-second marker. Malformed candidates, excess precision and
unsupported zone suffixes remain whole with diagnostics. Code, URLs, filenames,
paths, JSON and markup retain their existing protections. The separate
timestamp grammar recognizes complete RFC 3339 date-times as one
`TemporalValue::Timestamp`; use `--interpret-as datetime` when the caller
already knows the field is a timestamp. It keeps the written wall time and
numeric offset, preserves all fraction digits up to the shared 128-digit input
limit, accepts the RFC 3339 four-digit year range including `0000`, and
distinguishes `-00:00` (unknown local offset) from `Z` and
`+00:00`. It does not convert to another timezone. Lowercase `t`/`z` are
accepted. Malformed values, over-limit fractions and bracketed IXDTF
annotations stay one unresolved span.

`:60` is supported only for dates in the bundled positive leap-second table,
which is based on IERS Bulletin C 72 from July 2026 and records events through
2016. Update that table when IERS announces a later leap second. The renderer
speaks the local calendar and clock fields plus the explicit UTC offset; this is
a deterministic cross-locale reading convention, not independently reviewed
native pronunciation gold.
Known Korean period prefixes stay part of the clock, including fractional
clocks. Formal `오전`/`오후` and an explicit AM/PM suffix share the same strict
12-hour validation and opt-in redundant-hour normalization. Contradictions are
preserved as a whole unresolved clock. Broader period words receive no inferred
hour intervals.

`InvalidDatePolicy::Preserve` leaves an impossible Gregorian date unresolved.
`InvalidDatePolicy::ReadComponents` instead permits literal reading of an atomic
date with resolved order and individually bounded components. Month must be
1–12, day 1–31, and a written year positive and within the supported range.
The option does not infer order, repair the date, accept month 13/year 0, or
partially expand invalid date lists or calendar sequences; invalid compound
calendar input remains complete. Valid dates retain their ordinary semantics.
An eligible impossible date becomes `TemporalValue::InvalidCalendarDate`
(evaluation kind `invalid_date`), never a valid `Date`. An invalid ordinary
public `Date` value remains rejected even with this option enabled.

For example, `30/02/1990` can be read as
“treinta de febrero de mil novecientos noventa” while the reading retains
`supported: true` and `reason: Some("invalid calendar date")`. This says that
the requested literal reading was produced, not that February has thirty days.
Serde uses `preserve|read_components`; CLI spelling is
`--invalid-date-policy preserve|read-components`. Older options JSON without
this field retains `Preserve`.

```rust
use textintel::speech::{
    normalize_for_speech, SpeechInterpretation, SpeechLocale, SpeechOptions,
};
let mut options = SpeechOptions::new(SpeechLocale::Ar);
options.interpret_as = SpeechInterpretation::Measure;
options.unit_aliases.insert("f".into(), "temperature-fahrenheit".into());
let result = normalize_for_speech("f9", &options)?;
assert!(result.fully_supported);
assert_eq!(result.text, "تسع درجات فهرنهايت");
# Ok::<(), textintel::TextIntelError>(())
```

SI case remains meaningful: `mW` is not `MW`, `Mb` is not `MB`, and `ML` is not
`mL`. Unsupported symbol cases are preserved rather than expanded as another
unit. Numeric scaling and currency subunits use decimal strings, not floating
point. Quantity candidates recognize omitted integer digits (`€.50`, `.50 kg`)
through the same decimal parser, with Unicode digits and separator policy intact.
Indian ASCII/nonbreaking grouping is scanned under explicit numeric intent or a
recognized quantity context; ordinary prose remains conservative. Arabic decimal
and grouping marks retain their meaning; conflicting decimal options are rejected
instead of changing the magnitude. Explicit path prefixes and common relative
filenames are protected before numeric classification. Source wording remains available even when the semantic value is scaled.

## Languages and resources

The shared typed registry covers en, es, zh, hi, ar, fr, pt, ru, bn, ur, id, de,
ja, pa, mr, te, tr, ta, vi, and ko. Portuguese uses Brazilian number conventions,
Chinese simplified characters, and Punjabi Gurmukhi. Accepted aliases do not
silently select additional regional grammars. The versioned normalizer also
retains its existing Italian adapter.

The central lexical data contains 55 unit keys and 37 currency names for every
one of the twenty languages. This establishes resource presence, not complete
coverage of arbitrary nouns, grammatical cases, dialects, counters, or precision.
`CurrencyNaming::Full` uses these shared CLDR names independently of reading
style: Natural English `US$7` can retain “seven US dollars.” It does not choose
an ISO currency for an ambiguous symbol. Arabic additionally requires a reviewed
count-noun and qualifier paradigm; unknown paradigms remain unsupported even
when a CLDR display name exists. Currency precision and subunit limitations also
remain explicit diagnostics. `CurrencyNaming::Style`, the default, preserves
the established style-specific names.
Arabic Full naming has reviewed paradigms for 18 of the 37 codes. The four
feminine families INR/PKR/IDR/TRY use the shared feminine numeral core; counts
ending in 01/02 above 99, such internal scale coefficients, and nonzero feminine
currency fractions remain outside its reviewed domain. Unsupported readings
retain their source and typed value. Exact zero fractions can be read as integer
counts without deleting the fraction from IR.
Numeric ranges and independently checked composition rules are documented in
[`src/speech/data/README.md`](../src/speech/data/README.md).

Both common calendar/clock readings and basic mathematical expressions are
exercised across all twenty languages. Language-specific syntax still differs:
Japanese eras, Arabic currency agreement, Russian case markers, and Korean
native hour counters are implemented as explicit grammar rules, rather than
pretending that all languages use the same inflection.

## Shared core and versioned API

| Responsibility | Shared implementation |
|---|---|
| Gregorian calendar | `core::calendar` |
| Unicode marks and word letters | `core::unicode` |
| Decimal digits, word boundaries, exact parsing and scaling | `speech::core` |
| Code/tag protection and field scanning | `speech::lexer` |
| Optional pronunciation exception matching | `speech::dictionary` |
| Base languages, aliases and regional conventions | `speech::locales`, including `for_locale` |
| Cardinal/digit composition and presentation | `speech::numbers` |
| Calendar and clock grammar | `speech::temporal` |
| Quantities, symbols and unit dictionaries | `speech::quantities`, shared CLDR data |
| Scientific, duration, slash-fraction and range source shapes | `speech::shapes`, with explicit API compatibility profiles |
| Rational/scientific/range/ordinal/duration expressions | `speech::expressions` |
| Explicit grammatical inflections | `speech::grammar` |

`speech::normalization::normalize` exposes a versioned result containing spoken
text, changes, warnings, language-specific feature scope, and preparation identity.
`NormalizeOptions::resolve[_with_style]` resolves its mode, language/locale,
reading style, and input/output/expansion budgets. `Off` preserves input.
`SpeechText::prepare` retains the spoken text and stable identity. The current
ruleset is `tn-v4`; cache identities change with normalization semantics,
including the abbreviation policy.

`speech::locales::for_locale` is the shared regional lookup used by resolution,
legacy convention adapters and the typed normalization bridge. It distinguishes
en-US MDY/USD from en-GB DMY/unresolved bare `$`. CJK year-first defaults remain
separate from the versioned normalizer's historical MDY interpretation of
ambiguous year-final dates. Unknown regions are not guessed; Italian remains a
versioned adapter, and known versioned regions do not automatically become
accepted typed pronunciation conventions.

The versioned facade retains established presentation adapters. Its nine language
adapters that overlap the typed numeric core now share integer composition; they
do not maintain separate small-number/scale dictionaries. Classification policy
consolidation is still being audited: see [the implementation audit](SPEECH-CORE-AUDIT.md).
Within each versioned `normalize` call, a private `LexicalSource` holds one
code/tag protection map borrowed by both the legacy classifier and typed
augmentation. `Off` and structured JSON branches skip that scan. This removes
repeated protection scanning, while the two semantic compatibility dispatch
paths remain separate.
Both front ends now share fraction/range as well as scientific/duration shape
recognition. Their explicit `Typed` and `Versioned` profiles preserve different
sign, separator, padding, ordering and digit budgets. Legacy fraction/duration
adapters also check the supported rendering budget before calling a numeric
renderer: fitting in `u64` alone does not make a value readable. Oversized values
remain preserved or diagnosed rather than reaching an unsupported renderer.
Feature scope returned by `language_coverage()` describes implementation bounds,
not measured accuracy. No output from a speech API is an acoustic evaluation.

`Dictionary::new` accepts bounded, caller-supplied `(written, spoken)` exceptions.
`matches` yields original byte ranges, chooses longest matches, respects code/tag
protection, supports combining scripts and unspaced CJK phrases, and never
rescans replacements. Applying exceptions is an explicit consumer step; arbitrary
brand or pronunciation substitutions are not injected into ordinary normalization.

## Segmentation

Normalize complete input before `segment_text`; cutting first loses numeric and
calendar context. `SegmentPolicy::bounded(max_chars)` produces ordered fragments
with `index`, `total`, and `text`. The default bound is 200 Unicode scalar values.
Sentence boundaries protect expressions, then oversized pieces split at whitespace
and approximate grapheme boundaries. A cluster longer than the hard limit must
be split. Fragment boundary whitespace can be canonicalized. Empty input produces
no fragments and zero is treated as a one-character bound.

The shared sentence-boundary core now recognizes Arabic `؟`, Urdu `۔`, and
Indic `। ॥` without requiring following whitespace. Terminator runs and closing
punctuation remain together, with code/tag protections and UTF-8 partitions
retained. These four additions are STerm in [Unicode 17.0.0](https://www.unicode.org/Public/17.0.0/ucd/auxiliary/SentenceBreakProperty.txt).
ASCII abbreviation/whitespace behavior remains a tailored heuristic; this is not
full Unicode sentence-boundary conformance or a complete speech-abbreviation
inventory. The option-aware splitter consults the partial catalog, while the
legacy splitter keeps its historical list. The chunker may combine several
short sentences within one bound.

## Evaluation: original gold and reviewed variants

```sh
textintel parse-speech '03/04/2024' --locale es --date-order dmy --json
textintel normalize-speech '1234' --locale ja --number-notation orthographic --json
textintel normalize-speech 'US$7' --locale en --reading-style natural --currency-naming full --json
textintel normalize-speech '13:10pm' --locale zh --reading-style natural --clock-policy normalize-redundant-period --json
textintel normalize-speech '30/02/1990' --locale es --reading-style natural --invalid-date-policy read-components --json
textintel normalize-speech '३१-०६' --locale hi --reading-style natural --invalid-date-policy read-components --interpret-as date --json
textintel normalize-speech '2024-02-29T03:15:01.001000+05:30' --locale en --interpret-as datetime --json
textintel normalize-speech 'Dr. Smith arrived.' --locale en --abbreviation-policy contextual --json
textintel eval-speech data/speech/evaluation.json --json
textintel eval-speech data/speech/reference-nemo.json --json
textintel eval-speech data/speech/reference-nemo.json \
  --speech-policy data/speech/reference-policy.json --json
textintel eval-speech data/speech/reference-nemo.json \
  --speech-policy data/speech/reference-policy.json --accept-variants --json
```

`--currency-naming style|full` selects `CurrencyNaming`; `style` is the default.
`--abbreviation-policy preserve|contextual|bibliographic` selects the formal
catalog policy; `contextual` is the default.
The Hindi example explicitly declares date intent because the yearless
`३१-०६` pair can denote a range under Natural `Auto`. With date context and
component reading it produces “इकतीस जून”, retains the original thirteen UTF-8
bytes, and reports the invalid-calendar diagnostic.

`reference-policy.json` explicitly selects natural/orthographic rendering,
category hints, date order, and symbol meanings. Its results must be reported
separately from automatic parsing; source intent has been supplied. The reference
text and all original spoken alternatives remain unchanged.

Reports retain literal matches, separately accepted variants and rule IDs,
locale/category slices, unsupported reasons, complete failures, contradictory
references, and an upper bound imposed by identical inputs with incompatible
gold. `--accept-variants` changes only the exit-code gate; exact failures remain
in the report. Invalid or unsupported expressions do not acquire acceptance merely
because a plausible output exists elsewhere in the corpus.

Every reading whose `reason` is present also appears in
`SpeechEvalReport.diagnostics`, even when it is supported and exactly matches
the annotated spoken text. Records retain the case ID, locale, semantic kind,
original byte range/source, spoken text, support flag, and reason.
`diagnostics_by_reason` summarizes all such records. `unresolved_by_reason`
continues to summarize only unsupported readings. The two new fields default to
empty collections when reading older report JSON. Text `eval-speech` output
prints `diagnostics` and `supported_with_diagnostic` counts, so exact-match totals
do not hide literal invalid-date warnings. A comparison that opts into component
reading is a separate policy profile; it neither changes the original gold nor
adds accepted cases to the strict reference snapshot.

`speech::variants::rule_catalog()` records the bounded comparison rules and their
sources. Case, spacing, spelling, and dialect differences are accepted only in
reviewed typed contexts. Comparison never uses rebus/leet normalization, generic
accent stripping, or a table keyed by reference IDs. Unlisted differences remain
unresolved. Unit tests include sign, value, unit, prose, and forged-document
counterexamples.

Cases can additionally provide `expected_values`, an optional array of the
existing `SpeechSpan` representation, with complete `SpeechValue` payloads:

```json
{
  "start": 0,
  "end": 9,
  "source": "USD 12.50",
  "value": {
    "type": "currency",
    "code": "USD",
    "value": { "negative": false, "integer": "12", "fraction": "50" }
  }
}
```

The array describes every non-Text span under the selected parsing options.
`null` or an absent field leaves value evaluation unmeasured; `[]` explicitly
expects no typed spans. Validation checks ordered UTF-8 ranges, original source,
absence of Text spans, and agreement with `expected_spans` when both are supplied.
It does not derive expected values from the parser or certify the gold itself.
Different values remain valid annotations that can expose parser failures.

`SpeechSliceMetrics.semantic` is present only for annotated cases. Its `cases`
and `exact` count complete IR comparisons; `expected_spans`, `predicted_spans`
and `correct_spans` count full value-and-source matches. Sign, currency identity,
all fraction digits, clock fields and metadata participate in equality. This is
exact IR agreement under the selected policy, not equivalence between different
Precise/Natural representations. A parse error remains in the denominator, and a
correct kind or spoken string cannot override a value failure. Both exit gates
reject it, including `--accept-variants`. Failures retain `expected_values` and
`predicted_values`. The public report gate also rejects semantic totals whose
`exact` and `cases` differ, so removing failure metadata from deserialized JSON
cannot conceal a recorded semantic mismatch. Existing corpora and report JSON
without value annotations
retain their original span and spoken metrics.

The separate [semantic regression matrix](../data/speech/semantic-values.README.md)
covers all twenty locales with independently written structural expectations.
It contains no spoken gold and does not establish native pronunciation accuracy.

```sh
textintel eval-speech data/speech/semantic-values.json --json
```

Two additional rules remain narrowly scoped: `alternative_currency_minor_unit_policy`
permits the existing Precise/Natural decimal-versus-major/minor readings only for
one standalone German EUR or Portuguese BRL amount with a nonzero fraction of
one or two digits, after reparsing to identical IR. `chinese_currency_name_spacing`
allows layout space only between a standalone Chinese currency's unchanged
numeric reading and that ISO code's exact shared-dictionary name. It does not
split or replace the currency name. Variant comparison rejects any reading with
`reason: Some(...)`, including supported literal dates; diagnostic-bearing
readings do not gain acceptance through these rewrites. An exact literal match
under its explicitly selected profile still retains its diagnostic records.

`alternative_german_clock_zero_minute_policy` compares the existing Precise and
Natural readings of one standalone exact-hour German clock, preserving its
validated time and source spans. Explicit seconds and AM/PM/period markers are
excluded; `24:00` requires identical full IR and never becomes `00:00`.
`arabic_regional_currency_unit_singular` permits a documented regional spoken
construction only for integer counts 3–10 with a reviewed masculine currency
noun. It generates the entire amount from the shared number and currency data,
retains sign and country qualifiers, and excludes written fractions, feminine or
unknown nouns and surrounding prose. Canonical formal Arabic remains unchanged.
These are separately reported variants, not extra exact matches.

See [coverage and external development results](COVERAGE.md), the
[case-by-case reference review](SPEECH-REFERENCE-REVIEW.md), and
[reference sampling/provenance](../data/speech/reference-nemo.README.md). Code
coverage, resource availability, exact agreement, reviewed variants, and native
linguistic or acoustic validation are separate measurements.
