# Shared core and language coverage audit

Working-tree audit, 2026-11-14. This records implemented contracts and remaining
work; it does not claim that every linguistic rule has complete coverage.
The minimum is the twenty languages in
[`SpeechLocale::ALL`](../src/speech/types.rs). Existing Italian, Dutch and Polish
resources remain supported alongside that minimum.

## Language matrix

`numbers.json` and `lexicons.json` contain all twenty keys. The versioned
normalizer's [locale registry](../src/speech/locales.rs) contains those twenty
languages plus Italian. Shared symbol readings and custom fallback domain
profiles reuse the existing number, month and unit data through
[`resource_profiles.rs`](../src/speech/resource_profiles.rs); they do not add
seven independently maintained number dictionaries.

In this table, **file** means an existing language/symbol seed file;
**prose/shared** means a physical UDHR prose profile plus symbol readings
derived from shared speech/CLDR data.
Neither means a complete dictionary or independently measured language accuracy.
The transliteration column lists declared language-specific rule coverage.

| Code | Speech data / versioned locale | Embedded language + symbol seed | Chat abbreviation pack | Built-in G2P rules | Transliteration rules |
|---|---|---|---|---|---|
| en | present | file | present | present | — |
| es | present | file | present | present | — |
| zh | present | file | — | — | basic |
| hi | present | file | — | — | basic |
| ar | present | file | — | — | basic |
| fr | present | file | present | — | — |
| pt | present | file | present | — | — |
| ru | present | file | — | — | basic |
| bn | present | prose/shared | — | — | — |
| ur | present | prose/shared | — | — | — |
| id | present | file | — | — | — |
| de | present | file | present | — | — |
| ja | present | file | — | — | basic |
| pa | present | prose/shared | — | — | — |
| mr | present | prose/shared | — | — | — |
| te | present | prose/shared | — | — | — |
| tr | present | file | — | — | — |
| ta | present | prose/shared | — | — | — |
| vi | present | prose/shared | — | — | — |
| ko | present | file | — | — | — |

Evidence:

- [`resources/languages`](../resources/languages) now contains 23 language
  profiles. The seven new prose profiles are selected from pinned UDHR
  translations under the [recorded reproduction permission](../resources/UDHR-NOTICE.md).
  [`resources/symbols`](../resources/symbols) retains 16 physical symbol-language
  sets; the shared bridge fills seven more at embedded initialization.
  [`resources/abbreviations`](../resources/abbreviations) still has six languages,
  including Italian, giving five of the minimum twenty.
- Every canonical language now has readings for all 35 baseline symbols in
  the default embedded loader. Numerical/currency/percentage words reuse the
  shared core. Emoji and operator names come from a bounded 20-language CLDR
  48 annotation subset with [pinned source hashes and selection](../src/speech/data/symbol-labels.PROVENANCE.json).
  The symbol bridge preserves existing neutral concept IDs and does not choose
  a currency for an ambiguous speech amount.
- [`RuleBasedG2PProvider`](../src/phonetic/g2p.rs) declares English and Spanish.
  Generic output for other requested languages remains available with confidence
  capped at 0.15. Regional English/Spanish tags select their base rule family;
  this does not claim dialect-specific pronunciation.
- The optional [`EspeakNgG2PProvider`](../src/phonetic/espeak.rs) has curated
  voice mappings for all twenty, including Bengali, Urdu, Punjabi, Marathi,
  Tamil and Telugu. Its feature flag, installed binary and installed voices
  remain runtime dependencies. The [live 1.52.0 audit](PHONETIC-COVERAGE.md)
  now verifies 141 voice rows, twenty base requests and 29 registered aliases
  in the public example, separately from native pronunciation accuracy. Japanese
  Kanji remains partial and Arabic requires fully diacritized input according
  to that upstream version. Regional routing, tool-control parsing and explicit
  voice overrides have regression coverage; a static mapping alone is not the
  evidence for these runtime checks.
- [`RuleBasedTransliterationProvider`](../src/transliteration.rs) declares
  `ru ar zh ja hi und`, with documented partial script tables. Latin identity
  does not require a transliteration change; Arabic/Devanagari script sharing
  does not establish Urdu/Marathi-specific transliteration accuracy.

## Resource initialization and custom scope

[`ResourceLoader::add_speech_resources`](../src/resources/loader.rs) is public
and opt-in for custom loaders. It adds only missing language and symbol-language
packs. Existing packs take precedence, including deliberately narrow custom
resources. `from_resource_root("resources")` retains the on-disk scope until
the caller explicitly invokes that method.

The default embedded loader invokes the bridge after reading existing packs.
Then [`TextIntelligence::from_parts`](../src/engine/builder.rs) constructs its
`NgramLanguageDetector` from the resulting resource profiles. The dependency
path is:

```text
embedded JSON packs
  -> missing symbol packs from shared number/currency/percentage + CLDR data
     (custom opt-in can also fill absent language-domain seeds)
  -> ResourceLoader validation, hashes, limits and provenance
  -> NgramLanguageDetector::from_resources
```

The leaf generator never calls the speech normalizer, engine, or
`embedded_resources` singleton. This avoids recursive singleton initialization.
Generated terms have explicit 1,024-entry / 128-character bounds; normal resource
byte, entry, symbol and reading limits are also enforced. A failed addition
leaves the original loader unchanged. Generated SHA-256 hashes are validated by
the same canonical-JSON path used for external resource files.

The generator uses existing exact spellout for 0–99, localized months and unit
noun patterns. Neutral SI aliases do not become language-identification words.
It supplies no invented stop words, chat expansions or general-language corpus.
The manifest labels the source, license notices, derivation revision and hash.
Its seed status remains visible even though the language becomes a candidate.

Observed quality with the detector and abstention threshold unchanged:

| Held-out set | Top-1 | Top-3 | `unknown` first |
|---|---:|---:|---:|
| UDHR articles 21–30, 21 paragraphs × seven languages | 147/147 | 147/147 | 0/147 |
| Existing seven handwritten prose challenges | 4/7 | 7/7 | 3/7 |

The initial generated domain seeds abstained on all seven prose challenges.
Adding fixed UDHR profiles improves Marathi, Punjabi, Urdu and Vietnamese to
correct top-1 on those unchanged challenges. Bengali, Tamil and Telugu still
abstain on the out-of-domain queries. Broader independent profiles and held-out
native text remain required; one legal document does not establish general
conversational accuracy. The source split and query leakage checks are in
[`language_profile_corpus.rs`](../tests/language_profile_corpus.rs).

Two detector APIs retain different contracts:

- The engine's default [`NgramLanguageDetector`](../src/language/ngram.rs)
  derives candidates from loaded profiles. New domain seeds make the seven
  languages eligible, without proving accurate detection of arbitrary prose.
- [`DefaultLanguageDetector` / `ProfileLanguageDetector`](../src/language/detector.rs)
  use the resource index's lexical/script hints. That hint table can emit
  languages such as Urdu or Marathi without a lexical profile. Its evidence
  should not be described as the same n-gram/profile guarantee.

## Reuse already implemented

| Concern | Shared implementation | Consumers / evidence |
|---|---|---|
| Gregorian validation | [`core/calendar.rs`](../src/core/calendar.rs) | Entity dates, typed temporal parser, versioned normalization rules |
| Unicode mark and word predicates | [`core/unicode.rs`](../src/core/unicode.rs) | Segmentation, resource/LID word extraction, visual diagnostics; speech word boundaries reuse the core predicate |
| Source-preserving protected regions | [`speech/lexer.rs`](../src/speech/lexer.rs) | Typed parser, versioned classifier, sentence splitter, pronunciation dictionary; private `LexicalSource` shares one protection map across versioned classification and typed augmentation |
| Exact decimal fields and native digits | [`speech/core.rs`](../src/speech/core.rs) | Typed parser, quantities, temporal grammar and expression parsing; digit values are re-exported from `core/unicode.rs` for entity reuse |
| Entity decimal candidates | [`core/numeric.rs`](../src/core/numeric.rs) | Number entities and both currency-amount directions reuse one source-offset scanner; interpretation remains caller policy |
| Scientific, duration, fraction and range shapes | [`speech/shapes.rs`](../src/speech/shapes.rs) | Versioned classifier and typed expressions use explicit compatibility profiles and original UTF-8 offsets |
| Numeric date/time grammar | [`speech/temporal.rs`](../src/speech/temporal.rs) | Typed parser adapters call `parse_numeric_date` / `parse_numeric_time`; named/native forms use the same implementation |
| Exact subsecond clocks | [`speech/temporal.rs`](../src/speech/temporal.rs), shared number/quantity core | `FractionalClock` retains the complete clock and every fraction digit; dot is automatic, comma requires Time intent; no compact projection or timezone conversion |
| Literal invalid-date intent and diagnostics | [`speech/temporal.rs`](../src/speech/temporal.rs), [`speech/evaluation.rs`](../src/speech/evaluation.rs) | Shared component bounds/order checks produce distinct `InvalidCalendarDate` IR only under `ReadComponents`; evaluation retains reasons even for supported exact readings |
| Number composition and spellout data | [`speech/numbers.rs`](../src/speech/numbers.rs) | Typed renderer, quantity/temporal/expression renderers and most versioned language number adapters |
| Unit noun and symbol recognition | [`speech/lexicons.rs`](../src/speech/lexicons.rs), [`speech/data/unit-symbols.json`](../src/speech/data/unit-symbols.json) | Long/display alias forms derive from the same CLDR patterns used to render units; short/narrow surface forms are extracted from pinned CLDR 48 patterns with attached/separated evidence, conflict exclusion, and exact case-sensitive matching |
| Locale aliases, defaults and regional conventions | [`speech/locales.rs`](../src/speech/locales.rs) | `for_locale` supplies versioned resolution, shared adapters and all ten legacy convention constructors; typed alias scope remains explicit |
| Full localized currency names | [`speech/quantities.rs`](../src/speech/quantities.rs), shared CLDR lexicons | Opt-in `CurrencyNaming::Full` retains qualifiers across the twenty-language lexical path; the shared Arabic currencies have reviewed count/qualifier paradigms |
| Pronunciation exceptions | [`speech/dictionary.rs`](../src/speech/dictionary.rs) | One public bounded dictionary with shared lexical boundaries and protected-region handling |

Unicode fixes are functional, not merely capability labels. Before the shared
mark predicate, `शक्ति` segmented as `श` / `क्ति` and `தமிழ்` as `தமி` / `ழ்`.
They now remain complete words. Native viramas/vowel marks are reported in
Unicode diagnostics without adding the generic-accent spoof penalty. The
existing accent-folding policy retains script-specific marks.

## Remaining duplication and priorities

1. **Keep compatibility policies separate from shared shape grammar.**
   [`normalization/classify.rs`](../src/speech/normalization/classify.rs) now
   delegates exact decimal/group extraction to `speech::core`, numeric calendar
   and clock decoding to `speech::temporal`, and scientific/duration/fraction/range shape
   recognition to [`speech/shapes.rs`](../src/speech/shapes.rs). Both front ends
   use the same source grammars with explicit `Versioned` and `Typed`
   profiles. Different aliases, padding, digit budgets, activation styles and
   presentation remain intentional adapters. The private date-order enum was
   removed in favor of public `DateOrder`; historical en-GB and CJK policies
   remain unchanged. Legacy fraction/duration adapters now guard their supported
   rendering budget even for source values that fit `u64`, preventing calls
   outside the numeric renderer's range. Typed legacy locales now consume one
   typed parse for timestamps, scientific notation, attached durations,
   fractions, ranges, ordinals, two-component clocks, numeric calendar dates,
   plain numbers, percentages, legacy measure units, compatibility currencies
   and pre-catalog abbreviations, with explicit compatibility renderers retaining
   established versioned wording, gender context, quantity context, minor-unit
   precision, scale-word warnings and meridiem punctuation boundaries; Italian
   remains versioned-only because it has no typed locale implementation.
   Identifier-shaped spans remain fail-closed application data. Adapters
   inspect them only because the typed parser can use an alphanumeric shape for
   a malformed localized quantity candidate; suppressing that inspection would
   lose established warnings. This is compatibility evidence, not a second
   scientific, duration, fraction/range, grouping, Gregorian or quantity source
   grammar.

2. **The legacy field classifier is now confined to Italian and unknown pass-through languages.**
   [`normalization/shared.rs`](../src/speech/normalization/shared.rs) sends eleven
   shared-only languages directly to the typed core. Typed legacy languages now
   parse the same typed document once and project versioned behavior through
   explicit compatibility adapters; they no longer run the legacy field
   classifier at all. Italian remains on that classifier because it has no
   typed locale implementation. The choice comes from `has_versioned_adapter`
   in the shared locale registry, so adding a language no longer requires a
   second dispatch list. The private `LexicalSource` shares the original
   code/tag protection map across classification and typed augmentation within
   `normalize`; there is at most one protection scan per call, and `Off` /
   structured JSON branches skip it. Within augmentation, compatibility
   adapters project timestamps, clocks, expressions, numeric/calendar values,
   joined quantities, abbreviations, digit-run protection and vulgar-fraction
   diagnostics from typed spans or explicit lexical guards rather than parallel
   recognizers. Versioned wording remains an explicit rendering adapter over
   the shared parsed document.

3. **Keep regional interpretation and pronunciation scope explicit.**
   `locales::for_locale` now derives concrete regional records from the shared
   registry. Resolution, shared normalization and every legacy `Conventions`
   constructor consume the same decimal/group/date/dollar policy. en-US and
   en-GB retain different date and bare-dollar conventions. CJK YMD defaults
   remain separate from historical MDY year-final slash dates; Japanese/Korean
   defaults match CLDR48 short patterns (`y/MM/dd`, `yy. M. d.`). The Korean
   reference policy keeps its explicit `RejectAmbiguous` choice. Unknown regions
   are rejected rather than inferred, and a versioned locale record does not
   widen typed pronunciation aliases. Public `DateOrder` is reused without
   excluding Italian. Future regional additions still need reviewed grammar;
   resource loading retains its separate open-ended Italian/Dutch/Polish scope.
   The typed API separately exposes `ClockPolicy::Strict` (default) and opt-in
   `NormalizeRedundantPeriod`. The latter converts coherent 13–23 PM / 0 AM
   source forms into canonical 1–11 PM / 12 AM fields; contradictory periods and
   out-of-range fields remain unresolved. Rendering always enforces canonical
   IR, including for caller-constructed documents. Natural retains an AM/PM
   `TemporalValue::Clock`; Precise may project the same value to 24-hour
   `SpeechValue::Time` (13:10 for 1:10 PM). This TTS input convention
   does not change the strict external reference policy or accept its
   `十三点十分pm` wording for `13:10pm`.

   Shared compact projection now checks scalar validity and excludes metadata
   such as date labels/connectors, periods and half-hour notation. Rendering
   preserves explicit AM/PM through the established English/Spanish/Chinese
   branches or the existing 24-hour grammar in the other seventeen languages.
   All twenty locales have a 24-hour `Hm` format in [CLDR 48 Gregorian data](https://github.com/unicode-org/cldr-json/tree/48.0.0/cldr-json/cldr-dates-full/main).
   The clock primitive accepts optional minutes instead of manufacturing zero
   minutes for an hour-only source. The language-specific spoken forms remain
   subject to independent linguistic review.

   `InvalidDatePolicy::Preserve` remains the default for impossible Gregorian
   dates. Explicit `ReadComponents` reads eligible atomic components while
   preserving source, `invalid_date` semantics and a diagnostic; it is not a
   calendar repair or a valid `Date`. It requires resolved order, bounded
   month/day and a positive supported year, never infers an ambiguous order,
   and does not accept month 13/year 0. Invalid lists and calendar sequences
   remain whole rather than receiving partial literal readings. Hindi
   `३१-०६` additionally needs date intent because Natural `Auto` may classify
   that pair as a range. The CLI combination is
   `--invalid-date-policy read-components --interpret-as date`; JSON uses
   `read_components`.

4. **Entity and speech policies remain explicit adapters.**
   Entity number, currency and date scanners now reuse
   [`core::unicode::ascii_decimal_digit`](../src/core/unicode.rs) for native
   decimal digits. Dates/times reuse their strict existing scanner through a
   value projection and restore original UTF-8 offsets; the shared Gregorian
   validator remains authoritative. The twenty-language test matrix covers
   native-script values, calendar rejection, word/mark boundaries and claimed
   URL spans in [`entity_unicode_digits.rs`](../tests/entity_unicode_digits.rs).
   Entity currency policy still assigns `$` to USD and `¥` to JPY for evidence,
   while speech requires caller conventions. Those interpretation policies
   are not yet one shared document representation.

5. **Abbreviation data and sentence boundaries remain incomplete.**
   Formal speech now uses the separate, sourced
   [`speech/data/abbreviations.json`](../src/speech/data/abbreviations.json)
   catalog with exact spellings, locale and context checks. It has 44 entries
   across 19 locale codes: 20 preserve legacy rules and 24 add forms supported
   by primary institutional sources. Chinese and Telugu have no entries, and Italian is outside the typed
   twenty-language registry. This count is an inventory, not an accuracy or
   completeness score. Bibliographic forms require an explicit policy; medical
   and academic titles require a documented following-name context. See the
   catalog's [source and scope notes](../src/speech/data/abbreviations.PROVENANCE.md).
   The typed API and option-aware sentence splitter use those same contextual
   matches. Legacy `sentence_spans` retains its historical fixed boundary list
   for compatibility. Existing chat packs contain mappings such as German
   `bb → bis bald`; they do not become default TTS substitutions. Native
   sentence terminators `؟ ۔ । ॥` are recognized by the shared core, including
   runs and closers. Independent, locale-balanced spoken gold is still needed
   to measure precision and naturalness for the added catalog.

6. **Full phonetics/transliteration remains an independent language task.**
   The confidence fix prevents unsupported Latin text from inheriting 0.65
   English/Spanish confidence. It does not implement eighteen missing G2P
   grammars. Runtime eSpeak execution now has a separate pinned audit; native
   pronunciation gold and script- or language-specific transliteration still
   require independent checks. The audit exposes Japanese Kanji and Arabic
   vocalization restrictions rather than treating output presence as accuracy. Keep provider
   capability subsets and unavailable/fallback diagnostics explicit.

## Next accuracy work

1. Extend independently reviewed semantic and spoken data. Optional
   `expected_values` now reuses complete `SpeechSpan` / `SpeechValue` IR, with
   separate semantic counts and exit gates that cannot accept a wrong value
   through spoken variants. The new designed matrix checks 660 cases and 640
   exact value spans across twenty locales, including sign, currency, precision,
   AM/PM and protected-source contrasts. Its expectations were written before
   running the parser; it has no spoken gold and is not independent production
   data. Historical `parse_exact` continues to compare source/kind spans only.
   The selected NeMo sample covers only twelve locales; `bn ur id pa mr te tr ta`
   need external annotated examples, and most locales currently have only one
   authored complete spoken gold. Natural expression types and localized
   grammar need their own explicit-profile semantic and spoken gold.
2. Expand the partial speech abbreviation inventory only with sourced, exact
   locale spellings and independently reviewed spoken gold. Keep chat
   substitutions separate; test ambiguous titles, treatments and document
   references in prose, with exact-string results distinct from reviewed
   acceptable variants.
3. Collect independently reviewed spoken gold for complete timestamps and
   region-specific date/time wording across all twenty locales. The typed
   parser now preserves exact offsets, fractional precision, unknown-offset
   semantics and malformed-input boundaries; existing references do not
   establish native pronunciation quality.

## Verification scope

[`speech_resource_profiles.rs`](../tests/speech_resource_profiles.rs) verifies
the twenty-language minimum, preserved extra languages/custom scope, baseline
symbol attribution, seed bounds, hashes, idempotence and limit rollback.
Handwritten native-script challenges contain unseen words and check finite
candidates and eligibility; they deliberately do not assert perfect top-1
general-language detection or native-speaker certification.

[`unicode_word_contract.rs`](../tests/unicode_word_contract.rs) checks Indic
word boundaries, original offsets, isolated/native mark diagnostics and retained
accent folding. [`g2p_language_coverage.rs`](../tests/g2p_language_coverage.rs)
checks every canonical language's confidence contract and region-tag selection.
[`speech_adversarial_review.rs`](../tests/speech_adversarial_review.rs) checks
signed vulgar-fraction values, strict hint dimensions, Indian spacing, phone/range
priority, bounded filename protection, prose and original offsets. Filename
recognition is intentionally limited to six common extensions and 256-character
cores, including relative POSIX filenames. Explicit absolute, home-relative and
drive prefixes have a separate shared field guard; this is not a complete
filesystem grammar. Separator regressions check that Arabic marks cannot change
numeric magnitude under conflicting consumer options.
The current all-feature/all-target and direct instrumented runs each passed
773 tests, including regional metadata, full currency naming, fraction/range
consolidation, clock policies, the shared lexical protection map and eSpeak
voice/control regression fixtures. The pinned live eSpeak audit is recorded
separately from those test counts and does not establish native accuracy.
These counts include the literal invalid-date and retained-diagnostic changes.
Japanese phonetic calendar and clock formatting also reuse the compact API's
year, hour and minute counter rules. Calendar-day exceptions follow the Japan
Foundation's [Marugoto date table](https://www.marugoto-online.jp/a1/support/pdf/MarugotoStarterWordbook_jpn.pdf#page=76).
Regression cases cover both reading styles and retain omitted clock components.
The CLI and
evaluation contracts now distinguish supported literal component readings from
valid calendar values: `SpeechReading.reason` remains present, and all such
readings are retained in `SpeechEvalReport.diagnostics` with per-reason totals.
`unresolved_by_reason` still counts only unsupported readings. Regression checks
also reject valid-date gold kinds for literal invalid-date IR and keep diagnostic
records unchanged when spoken gold is wrong. The current coverage snapshot
keeps the profiles separate; opt-in literal-profile matches are not
bonus passes for the unchanged strict reference policy.
The snapshot records the measured source checksum and executed
test counts. A before/after capture of 1,890 versioned fraction/range examples
kept 1,888 outputs unchanged and fixed two previously panicking oversized
fractions by preserving input. These are compatibility checks, not independent
linguistic judgments. Clippy, Rust 1.90 without default features, and API
documentation checks also passed.
Arabic full currency names now have reviewed paradigms for all 37 shared
codes, including irregular or lexically disputed families (euro, won, taka,
dong, peso, naira, baht, new shekel and zloty). One shared feminine numeral
core replaces the former unit-only table. Reviewed dual scale endings are
applied only in construct position; isolated/conjoined duals retain their
ending. Higher compounds ending in 01/02 use the documented conjoined-cardinal
convention (مئة وواحدة / مئة واثنتان), while coefficients of masculine scale
nouns remain masculine. Nonzero decimal tails are read independently of the
currency noun because فاصلة separates two numerals; natural rational tails are
governed by their denominator. Exact regression tests cover every shared
currency in both short and full naming across zero, one, dual, plural,
accusative, hundred/thousand compounds and nonzero decimal fractions. A second
matrix covers the same regimes for all 37 shared currencies in every one of the
twenty speech locales, in both reading styles and naming modes. CLDR labels
alone are still not treated as sufficient evidence for inflection.
The ruble noun/gender and plural are supported by [Birzeit Qabas](https://sina.birzeit.edu/qabas/lemma/2023241124)
and a [university Arabic manual](https://skiu.ru/wp-content/uploads/2024/12/uchebnoe-posobie-arabskij-yazyk-v-religioznoj-sfere.-2-chast.pdf).
WIPO documents the [Swiss-franc dual](https://www.wipo.int/ar/web/hague-system/hague-system-certified-documents)
and [plural/accusative forms](https://www.wipo.int/ar/web/hague-system/w/news/2024/news_0004).
The feminine numeral/dual rules and contextual readings of eight are supported
by [university number grammar](https://uomus.edu.iq/img/lectures21/MUCLecture_2025_6129126.pdf)
and [lira/counting examples](https://uomus.edu.iq/img/lectures21/MUCLecture_2024_51215125.pdf);
[MANUU's Arabic grammar](https://dde.manuu.edu.in/sites/default/files/2021-12/Arabic%20Grammar%20and%20Communication_0.pdf)
documents the rupee noun and plural. Implicit one/dual unit patterns no longer
erase explicit decimal zeros: decimal quantities use a number-bearing pattern.

The designed semantic matrix exposed an additional rendering gap for Arabic
`EUR 2.00`. Style naming now reads integral one/dual euro amounts through the
same count helper as existing currencies, while retaining written fraction digits
in IR. The forms are attested in UN [A/CONF.219/2021/INF/1, p.16 §107](https://digitallibrary.un.org/record/3950930/files/A_CONF.219_2021_INF_1-AR.pdf)
and [E/C.12/69/D/48/2018, p.2](https://digitallibrary.un.org/record/3993942/files/E_C.12_69_D_48_2018-AR.pdf).
This adds no complete plural/full-name paradigm and no new comparison variant;
existing major/minor composition reuses the reviewed cent grammar.

The subsecond regression matrix covers 1,800 language/style/notation/precision
combinations, plus explicit AM/PM, unsupported suffixes, field bounds and source
protection. It verifies exact fields and distinct written precision, not native
pronunciation. A separate timestamp matrix now covers 100 semantic cases (five
per locale), including fractions, known and unknown offsets, year zero, and verified
historical leap seconds. The parser follows the [RFC 3339 grammar](https://www.rfc-editor.org/rfc/rfc3339.html#section-5.6);
its -00:00 handling follows the original [unknown-local-offset convention](https://www.rfc-editor.org/rfc/rfc3339.html#section-4.3).
The positive leap-second snapshot is dated to IERS Bulletin C 72 (2026-07) and
must be updated after later announcements. Neither a numeric offset nor a
locale supplies a geographical time zone.
[Coverage and evaluation](COVERAGE.md) records the
separate metrics and remaining reference/quality-gate failures. These checks do
not make the remaining architecture and language work above complete.

## Implementation design: native unit symbols and attachment

Status: implemented and regression-covered. `scripts/generate_unit_symbols.py`
extracts short and narrow suffix patterns from pinned CLDR JSON 48.0.0 into
`src/speech/data/unit-symbols.json` (1,574 locale-scoped surface forms). The
extract records only the semantic distinction between attached (`{0}kg`) and
separated (`{0} kg`) forms; directional formatting controls are removed, all
Unicode whitespace widths satisfy separation at match time, and leading-slash
reciprocal patterns are not treated as simple unit aliases. Forms that map to
more than one canonical unit at the same attachment are excluded rather than
guessed.

Typed matching preserves original UTF-8 spans, applies NFKC compatibility for
fullwidth forms, and remains case-sensitive because SI prefixes and bit/byte
case are semantic. Established global aliases remain compatibility behavior.
Newly projected punctuation-only symbols (`′`, `″`, bare `°`) and one-letter
ASCII narrow forms (`B` and French `o`) require explicit measure intent. The
irreconcilable narrow conflicts `m` (metre/minute) and Turkish `s`
(hour/second) keep the established metre/second reading. Versioned typed
legacy normalization surfaces new localized/non-ASCII unit evidence while
keeping identifier-shaped tokens such as `4K`, duration opt-in policy, warning
behavior and Italian's versioned-only boundary intact.

## Implementation design: month-year partial dates

Status: implemented and regression-covered across the twenty typed locales.
The design facts below are retained because they explain the small IR surface
and the guardrails; one initial grammar assumption is corrected inline for
Russian. The pinned NeMo aggregate is unchanged at 216 / 544 exact and 303 /
544 accepted: the reference's dayless month-year cases use already-supported
East Asian/Vietnamese forms and none exercise the newly added named-month arm.

Verified scope-reducing facts (measured on the tree, not assumed):

- The IR already supports partial dates: `TemporalValue::Date { year: Option<String>,
  month: Option<u32>, day: Option<u32>, .. }` — a month-year value is
  `day: None` with no IR change required.
- Month-name recognition exists in `month_number()` (`src/speech/temporal.rs`),
  covering CLDR month forms plus grammatical aliases (e.g. Russian nominative
  `май` alongside genitive `мая`), with 3+ letter abbreviations.
- `parse_date` (`src/speech/temporal.rs`) already produces partial forms
  for bare years and decades; the month-year rule slots in as another arm
  before falling through to numeric parsing: recognize `<month-name> [connector]
  <4-digit year>` and locale-appropriate inversions, producing
  `Date { day: None }`. Numeric `YYYY-MM` and width-disambiguated `MM/YYYY`
  forms use the same IR under explicit date intent.
- The real work is per-locale rendering in `render()` for `day: None`:
  connector and case rules (English "may twenty twenty four"; Spanish "mayo
  de dos mil veinticuatro"; Russian nominative month `май` plus genitive year
  reading and explicit `года`; CJK `2024年5月` ordering). Russian genitive
  `мая` is grammatical only when governed by a day number, so the initial
  design assumption that it should be repeated here was wrong. Year rendering
  itself already exists in all locales — this is composition, not new number
  grammar.
- Guardrails: `SpeechInterpretation::Date` hints must force numeric partial
  forms; 1-2 digit years after a month name are ordinals/days, not month-year;
  ambiguous bare numbers stay protected exactly as today. Four-digit width is
  decimal-digit count after Unicode digit folding, never UTF-8 byte length.
  Connectors and Russian year labels are recognized case-insensitively and
  consumed atomically rather than left as prose.

Execution result: the parse arm and renderer are covered by exact regression
cases for all twenty locales, every public month across every locale/style/
notation combination, all twelve Russian nominative and prepositional month
forms, Korean `유월`/`시월` and Japanese phonetic irregulars, representative
en/es/ru/ja semantic IR checks, the short-number guardrail, English `of`,
uppercase connectors, Russian full `года` labels and prepositional `в мае`
forms, trailing numeric
`MM/YYYY` forms, and native-script digit years across the applicable Unicode
decimal scripts. The authored speech evaluation now includes month-year cases
and remains exit 0 at 214/214 exact
parses and 30/30 spoken golds. The NeMo aggregate is unchanged as described
above, and `COVERAGE.md` no longer lists month-year partial dates as a verified
gap.
