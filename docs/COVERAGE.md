# Coverage and speech validation

## Current working tree — 2026-11-15

The original NeMo gold remains unchanged. The evaluator keeps literal matches,
additional bounded variants, and their sum in separate fields. All 544 cases
remain in every spoken denominator. The detailed snapshot is in
[`data/speech/current-metrics.json`](../data/speech/current-metrics.json).

| Evaluation mode | Exact | Additional variants | Exact + variants | Rendering completed |
|---|---:|---:|---:|---:|
| Original automatic policy, historical baseline | 156/544 (28.68%) | Not measured | Not measured | — |
| Same automatic policy, current core | 216/544 (39.71%) | 87/544 | 303/544 (55.70%) | 454/544 |
| Explicit natural/orthographic corpus policy | 464/544 (85.29%) | 49/544 | 513/544 (94.30%) | 540/544 |
| Corpus policy + literal invalid-date components | 467/544 (85.85%) | 49/544 | 516/544 (94.85%) | 543/544 |
| Corpus policy + literal dates + redundant clock normalization | 467/544 (85.85%) | 49/544 | 516/544 (94.85%) | 544/544 |

The corpus policy supplies category hints, date order, symbol meanings and one unit
alias through [`reference-policy.json`](../data/speech/reference-policy.json).
It is a different inference task from automatic parsing. This external sample
has now guided development; its upstream origin does not make these results
held-out accuracy or proof of correctness in all twenty languages.
This selection covers twelve locales; `bn`, `ur`, `id`, `pa`, `mr`, `te`, `tr`
and `ta` have no external cases in it. Most locales have only one complete
spoken gold in the authored corpus. More independently reviewed application text
is needed to estimate precision across the twenty-language TTS surface.

The authored regression matches the annotated spans in **214/214 cases**,
including all 210 expected spans, and passes **30/30 complete spoken golds**.
The current policy reports 178/214 fully supported authored readings; this is a
rendering diagnostic and does not add spoken gold. The catalog adds 24 sourced
forms. Its 44-entry inventory still has two typed locales with no entries.
Its other cases have no spoken gold and are not counted as correct utterances.
`parse_exact` compares annotated span offsets, source text and kind. These
legacy cases have no complete value annotations; their parse metric is not
semantic-value accuracy. The separate `semantic-values.json` matrix now supplies
full IR gold for **660 cases across twenty locales**, with **640/640 exact value
spans**. All 660 cases match their complete semantic expectations, including
explicit empty sets for ordinary prose. These are designed structural
regressions with shared templates, not independent production or pronunciation
validation; there is no spoken gold. The original NeMo input remains unchanged;
the authored 214-case regression does not acquire a semantic denominator.
The new matrix completes 600/660 renderings; the other sixty deliberately retain
invalid dates, code or JSON. A correct preserved IR is not a supported utterance.
NeMo supplies no span annotations, so it cannot establish parser precision/recall.
Its `fully_supported` count is 540/544 under the unchanged reference policy, a
rendering diagnostic rather than an accuracy measure.
An additional run with `--clock-policy normalize-redundant-period` completes
541/544 renderings but still accepts 513/544 references. It normalizes `13:10pm`
to the same time as `1:10pm`; its spoken output still differs from the original
Chinese reference. The default and checked-in reference policy remain Strict.

The last two rows also select `--invalid-date-policy read-components`. They
read three impossible dates literally while retaining `invalid_date` values and
diagnostics. Combining that with redundant clock normalization completes all
544 renderings; it does not establish 544 correct calendar values or utterances.
The combined report still has 28 unaccepted reference strings and three explicit
invalid-date diagnostics. These diagnostic records remain visible even for the
three exact literal word matches. Invalid-date readings are excluded from
variant equivalence. The original automatic policy and reference policy are
unchanged; the new profiles answer a different transcription question.

The [complete reference review](SPEECH-REFERENCE-REVIEW.md) retains all 80 strict
mismatches, including the 49 accepted variants. The remaining 31 comprise sixteen
reference spelling/grammar/pronunciation defects with evidence, eight unaccepted
Arabic register/agreement differences, three invalid calendar dates, and four
wording or interpretation-domain disagreements. Adjudications do not add passes.
Identical Russian `13:05` inputs with incompatible gold impose a literal ceiling
of 543/544 under a fixed policy; both valid clock wordings can be represented by
the separate variant metric.

### Instrumented source coverage

The fresh report contains 144 owned source files, with one absolute source path
per file. The earlier cached report containing `/src/src/...` aliases was discarded.

| Measurement | Covered / instrumented | Percent |
|---|---:|---:|
| Entire library and CLI, lines | 29,107 / 32,796 | 88.75% |
| Entire library and CLI, functions | 2,849 / 3,232 | 88.15% |
| Speech modules, lines | 15,720 / 16,673 | 94.28% |
| Phonetic modules, lines | 994 / 1,073 | 92.64% |
| eSpeak adapter, lines | 432 / 484 | 89.26% |
| Typed speech parser, lines | 444 / 451 | 98.45% |
| Semantic value and span evaluation, lines | 419 / 454 | 92.29% |
| Exact number composition, lines | 1,010 / 1,052 | 96.01% |
| Quantity grammar, lines | 1,268 / 1,366 | 92.83% |
| Temporal grammar, lines | 1,809 / 1,951 | 92.72% |
| Reviewed variant comparator, lines | 1,168 / 1,208 | 96.69% |
| Shared scientific/duration/fraction/range shapes, lines | 515 / 521 | 98.85% |
| Regional conventions registry, lines | 35 / 35 | 100.00% |
| Shared speech lexer, lines | 374 / 375 | 99.73% |
| Speech abbreviation matcher, lines | 529 / 531 | 99.62% |

The earlier baseline at `5917360` measured 21,013/24,437 lines (85.99%) and
2,215/2,585 functions (85.69%). Executable source grows as grammar is added;
this is not a fixed-denominator comparison. Inline Rust unit tests remain in
source coverage, matching the historical filter. JSON lexical data is not an
executable-line denominator; separate contracts check its language/unit/resource
presence. Branch and MC/DC coverage were not measured. These percentages do not
establish linguistic or acoustic accuracy.

### Verification and remaining limits

The complete all-feature/all-target suite passes **773 tests**, as does the
separate instrumented run. Clippy with
warnings denied, generated documentation and Rust 1.90 all-target checks without
default features, both with and without `phonetic-espeak`, pass. Shared-core
contracts exercise all twenty languages,
including native digit values, protected spans and UTF-8 offsets. These contracts
are not independent native-speaker certification. The
[core and language audit](SPEECH-CORE-AUDIT.md) records the shared scientific/
duration/fraction/range grammar, regional conventions, shared protection map,
explicit clock policies, remaining adapters, and actual G2P, abbreviation and
transliteration subsets; the library does not claim universal twenty-language
parity in every provider.

A CLI regression additionally checks that `3:15am` and `3:15pm` produce distinct,
supported readings in all twenty locales. The snapshot retains the complete
before/after observations in `clock_meridiem_regression_audit`; the earlier,
unversioned development probe collapsed seventeen pairs. This is a semantic
information-preservation check, not native-language or pronunciation accuracy.
A separate `subsecond_clock_regression_audit` records 80 current CLI cases:
four fractional precisions in each of twenty locales, with exact fields and
distinct readings. The Rust matrix additionally covers 1,800 combinations of
seconds, precision, style and notation, plus Korean period contradictions and
source protections. The separate RFC 3339 timestamp matrix adds 100 semantic
cases (five per locale) for UTC, fractional numeric offsets, unknown local
offsets, year zero and verified historical leap seconds. Malformed timestamps remain
atomic. These formats are absent from the selected NeMo sample. The subsecond implementation
in `c3a96f5` left all seven existing NeMo/authored reports byte-identical to its
preceding snapshot. The current comparison rules add eight German exact-hour
variants and one Arabic currency variant under automatic policy, and two Arabic
currency variants under explicit corpus policy. Exact matches, rendered text,
original gold and case denominators are unchanged by these rules.
The source-backed English 24-hour correction removes one former exact NeMo
match; [E1 in the reference review](SPEECH-REFERENCE-REVIEW.md#e1-english-24-hour-whole-hours--corrected-renderer-reference-retained)
records that reference defect instead of accepting it through a variant.

A separate [live eSpeak 1.52.0 audit](PHONETIC-COVERAGE.md) verifies the real
141-row voice inventory and execution of twenty base-language examples plus
29 registered aliases. Raw and processed results, source/build hashes and
standalone reproduction helpers are retained in
[`data/phonetic/espeak-runtime-audit.json`](../data/phonetic/espeak-runtime-audit.json).
The audit also checks explicit voice overrides and leading-option text such as
`--help` and `-12`. Japanese Kanji fallback and Arabic vocalization restrictions
remain visible; runtime success is not native pronunciation accuracy. This live
audit is separate from the instrumented unit/integration test count.

The general comparison/search quality gates are all green. Measured on the
benchmark test split with `--no-default-features` — accuracy 92.1%, F1 93.4%,
ROC-AUC 0.981, PR-AUC 0.989, Brier 0.050, ECE 0.041; search recall@1 83%,
recall@5 97%, recall@10 99%, MRR 0.885, nDCG 0.910. Every global similarity
gate, every search gate, and every category gate passes — including
`cross_language` (accuracy/F1 1.000), `semantic` (F1 0.857) and `spam`
(F1 0.710, ROC-AUC 0.949).

This state combines four trained or curated layers, each explainable and
deterministic: the GBM+logistic decision ensemble over the evidence vector
(`src/comparison/decision.rs`, protocol in `tools/train_decision/README.md`),
the synonym/antonym lexicon with single- and multi-position swap probes
(`src/resources/synonyms.rs`), the cross-lingual cognate/false-friend
inventory (`src/resources/cognates.rs` — the lexical knowledge the
byte-identical false-friend cases require: `internet`/`internet` positive
versus `red`/`red` negative are separable only through lexical identity),
and the baked logistic spam calibration (`src/detection/spam.rs`, protocol
in `tools/train_spam/README.md`; held-out spam-corpus accuracy 96.9% (within-distribution),
F1 96.8%, ROC-AUC 0.998).

The cross-language resource is honest lexical knowledge, not eval leakage:
the inventory is curated from the train split and canonical false-friend
reference lists, and the gate it passes measures exactly the linguistic
distinction it encodes. The remaining honest limitation is coverage — the
inventory carries common international vocabulary, not the long tail of
cross-lingual lexicon. Applications needing wider coverage can extend the
resource or substitute a multilingual semantic provider through the public
provider boundary.

Passing speech/unit tests does not make that existing quality job green —
the gates above are the quality job, and they pass.

### Reproduce current results

```sh
cargo test --locked --all-features --all-targets
cargo clippy --locked --all-features --all-targets -- -D warnings
cargo +1.90 check --locked --no-default-features --all-targets
cargo +1.90 check --locked --no-default-features --features phonetic-espeak --all-targets
cargo doc --locked --no-deps --all-features
RUSTC_WRAPPER= cargo llvm-cov --no-rustc-wrapper --locked \
  --all-features --workspace --json \
  --output-path target/coverage/goal-semantic-values-direct.json \
  --ignore-filename-regex '(/tests/|/benches/|/examples/|/registry/)'
cargo run --target-dir target/evaluation-target --locked --quiet --no-default-features --bin textintel -- \
  eval-speech data/speech/evaluation.json --json
cargo run --target-dir target/evaluation-target --locked --quiet --no-default-features --bin textintel -- \
  eval-speech data/speech/semantic-values.json --json
cargo run --target-dir target/evaluation-target --locked --quiet --no-default-features --bin textintel -- \
  eval-speech data/speech/reference-nemo.json --json
cargo run --target-dir target/evaluation-target --locked --quiet --no-default-features --bin textintel -- \
  eval-speech data/speech/reference-nemo.json \
  --speech-policy data/speech/reference-policy.json --accept-variants --json
cargo run --target-dir target/evaluation-target --locked --quiet --no-default-features --bin textintel -- \
  eval-speech data/speech/reference-nemo.json \
  --speech-policy data/speech/reference-policy.json \
  --clock-policy normalize-redundant-period --json
cargo run --target-dir target/evaluation-target --locked --quiet --no-default-features --bin textintel -- \
  eval-speech data/speech/reference-nemo.json \
  --speech-policy data/speech/reference-policy.json \
  --invalid-date-policy read-components \
  --clock-policy normalize-redundant-period --json
cargo run --locked --features phonetic-espeak --example espeak -- \
  --binary target/tools/espeak-audit/install/bin/espeak-ng --json
```

All NeMo runs above still exit 1. `--accept-variants` changes only that gate;
original strict failures and gold remain in the JSON. Coverage compilation
bypasses the local compiler wrapper to avoid counting cached `/src/src/...`
aliases as additional files. Branch and MC/DC coverage are not measured.

## Historical snapshot — 2026-11-13

Measured on 2026-11-13. Machine-readable results, locale/category slices, and
unresolved reasons are recorded in
[`data/speech/baseline-metrics.json`](../data/speech/baseline-metrics.json).
This snapshot distinguishes executed code, regression results, and spoken agreement.

| Measurement | Result | What it establishes |
|---|---:|---|
| Overall line coverage | 81.62% → 85.99% | Executed lines, before/after these changes |
| Overall function coverage | 79.49% → 85.69% | Executed functions |
| Legacy date/time line coverage | 62.59% → 98.15% | Calendar and boundary cases exercised |
| Versioned normalizer line coverage | 94.29% | Language rules exercised |
| Experimental typed parser line coverage | 95.14% | Parser paths exercised |
| Shared segmentation line coverage | 97.93% | Bounded splitting paths exercised |
| Authored typed regression | 214 / 214 exact parses; 30 / 30 spoken gold | Covered regression cases only |
| External NeMo sample | 156 / 544 exact spoken strings (28.68%) | Strict external string agreement for the experimental API |

The overall coverage comparison uses the same command/filter as the baseline
commit `c99f333`. The denominator grows from 15,736 to 24,437 instrumented lines;
this is not merely additional tests over an unchanged codebase. Standard inline
Rust unit tests remain in source coverage; third-party macro source and files
under tests/benches/examples are excluded from the report. Branch and MC/DC coverage were **not measured**.
Coverage is not a linguistic accuracy score.

The versioned normalizer uses the `tn-v2.1` ruleset with 117 normalization
unit tests. Tests exercise supported forms, ambiguity preservation, warnings,
locale validation, and input/output budgets. They do not establish universal
linguistic accuracy or acoustic quality.

The hand-authored regression includes 210 annotated typed spans across 20
language conventions. Only 30 cases contain complete spoken gold; the remaining
cases must not be counted as correct spoken outputs. Of 214 cases, 178 report
`fully_supported`; unresolved cases include intentionally ambiguous inputs and
protected identifiers. That flag is an implementation diagnostic, not accuracy.

### Historical external gaps and priorities

The NeMo sample is a separate, pinned first-row sample with 544 cases in 12
languages. No incompatible rows were removed. It supplies spoken text, not
entity boundaries. [Selection, licensing, and caveats](../data/speech/reference-nemo.README.md)
include upstream invalid dates and reading-policy assumptions.

| Feature | Exact / cases | Exact + accepted variants |
|---|---:|---:|
| Numbers | 91 / 120 | 108 / 120 |
| Dates | 51 / 106 | 60 / 106 |
| Currency | 6 / 109 | 23 / 109 |
| Time | 23 / 99 | 38 / 99 |
| Measures | 45 / 110 | 74 / 110 |

Re-measured on the current tree with the automatic-policy command recorded in
[`data/speech/current-metrics.json`](../data/speech/current-metrics.json)
(544 cases, 215 exact, 303 accepted, 454 fully supported). The automatic-policy
snapshot is a support/agreement measurement, not a certification of reviewed
currency grammar; most strict currency differences come from missing explicit
locale/currency policy and reviewed register variants.

Investigate failures before changing behavior. Examples illustrate different
causes: English `123` differs by optional “and” and hyphenation; Japanese gold
uses kanji while the experimental renderer produces kana. Several historically
missing shapes are now implemented and verified on the current tree: named-month
dates (`1 enero` → “primero de enero”; also English and Russian), dotted date
forms (`06.03.2006` reads correctly in Russian once the date order is known —
the default ambiguity rejection is intentional protection), and case-insensitive
unit symbols (`12KG` → “doce kilogramos”). Month-year partial dates are now
implemented and regression-covered across all twenty typed locales (`May 2024`,
`May of 2024`, `mayo de 2024`, `май 2024 года`, `в мае 2024 года`, CJK
year-month forms, and width-disambiguated numeric forms such as `05/2024` under
explicit date intent), including native-script decimal digit years. Verified
currency agreement is now implemented across the shared currency matrix.
Under the explicit NeMo reference policy the category has 94 / 109 exact
matches, 5 reviewed variants, 99 / 109 accepted readings, and 109 / 109 fully
supported readings; the exact/variant split reflects reviewed register and
reference defects rather than missing implementation. Bare `$` still requires an
explicit currency convention rather than a guess.

Recommended implementation order:

1. ~~Named months, dotted date forms, year-reading conventions, localized
   clock expressions, month-year partial date forms, and CJK weekday-on-date
   validation~~ — implemented and verified on the current tree. Keep invalid
   calendars and ambiguous dates protected.
2. ~~Currency dictionaries with noun gender/case, minor units and inflected
   forms~~ — implemented and regression-covered for the shared currency matrix.
   Native short/narrow unit symbols and CLDR attachment evidence are
   implemented and regression-covered across the twenty typed locales.
   High-risk punctuation-only and one-letter ASCII forms remain explicit
   measure-intent forms, and irreconcilable narrow-symbol conflicts keep the
   established global SI meaning rather than guessing.
3. Independently reviewed spoken gold for RFC 3339 timestamps, ordinals,
   fractions, ranges, scientific notation and durations. The typed parser has
   timestamp IR/rendering, the versioned adapters now cover the shared
   expression families across typed legacy languages, and Russian ranges use
   genitive endpoints, but cross-locale wording still needs native review.
4. Semantic dispatch in typed legacy languages is consolidated around the
   shared typed document. Source protection, timestamps, scientific notation,
   attached durations, fractions, ranges, ordinals, two-component clocks,
   numeric calendar dates, plain numbers, percentages, legacy measure units,
   compatibility currencies, pre-catalog abbreviations, digit-run protection
   and vulgar-fraction diagnostics use shared parsing/shape evidence with
   versioned adapters retaining offsets, warnings, context-sensitive morphology
   and compatibility wording. Identifier-shaped spans remain fail-closed
   application data; adapters inspect them only to preserve malformed localized
   quantity warnings exactly.
5. Context distinguishing padded counts, phone/serial/postal identifiers,
   mixed-script text, and domain abbreviations. Add independently reviewed
   minimal pairs before expanding transformations.
6. Native-speaker gold for all 20 experimental conventions, regional variants,
   held-out real application text, and paired listening tests in consuming TTS engines.

Preserve established normalization behavior while these rules develop. A failing independent
example should become a reviewed regression only after deciding its intended
locale, reading style, valid alternatives, and preservation policy. Do not
replace its gold with whatever the implementation currently emits.

Validation also passed 803 tests with all features, 763 tests with no default
features and all targets, Clippy with warnings denied, documentation without
warnings, formatting checks, and a Rust 1.90 all-targets check. The compiling
[`examples/speech.rs`](../examples/speech.rs) demonstrates preparation for a TTS consumer.

### Historical reproduction

```sh
cargo test --locked --no-default-features --all-targets
cargo test --locked --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo llvm-cov --all-features --workspace --json \
  --output-path target/coverage/current.json \
  --ignore-filename-regex '(/tests/|/benches/|/examples/|/registry/)'
cargo run --locked --no-default-features --bin textintel -- \
  eval-speech data/speech/evaluation.json --json
cargo run --locked --no-default-features --bin textintel -- \
  eval-speech data/speech/reference-nemo.json --json
```

The external speech command deliberately exits 1 when any gold differs. CI gates the
authored regression without treating the independent challenge as already solved.
The analysis/search quality command now exits 0 on the current tree: comparison
accuracy is 92.12% (minimum 80%), F1 is 93.36% (minimum 86%), ROC AUC is 98.09%
(minimum 90%), PR AUC is 98.88% (minimum 95%), search recall@5 is 97% and
recall@10 is 99% (both minimum 95%), and search MRR is 88.49% (minimum 60%).
The historical failure note is therefore obsolete; thresholds were not relaxed.
Passing quality gates still do not certify native-speaker speech quality.
