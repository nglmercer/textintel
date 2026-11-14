# Coverage and speech validation

## Current working tree — 2026-11-14

The original NeMo gold remains unchanged. The evaluator keeps literal matches,
additional bounded variants, and their sum in separate fields. All 544 cases
remain in every spoken denominator. The detailed snapshot is in
[`data/speech/current-metrics.json`](../data/speech/current-metrics.json).

| Evaluation mode | Exact | Additional variants | Exact + variants | Rendering completed |
|---|---:|---:|---:|---:|
| Original automatic policy, historical baseline | 156/544 (28.68%) | Not measured | Not measured | — |
| Same automatic policy, current core | 216/544 (39.71%) | 78/544 | 294/544 (54.04%) | 454/544 |
| Explicit natural/orthographic corpus policy | 464/544 (85.29%) | 47/544 | 511/544 (93.93%) | 540/544 |
| Corpus policy + literal invalid-date components | 467/544 (85.85%) | 47/544 | 514/544 (94.49%) | 543/544 |
| Corpus policy + literal dates + redundant clock normalization | 467/544 (85.85%) | 47/544 | 514/544 (94.49%) | 544/544 |

The corpus policy supplies category hints, date order, symbol meanings and one unit
alias through [`reference-policy.json`](../data/speech/reference-policy.json).
It is a different inference task from automatic parsing. This external sample
has now guided development; its upstream origin does not make these results
held-out accuracy or proof of correctness in all twenty languages.

The authored regression still passes **209/209 exact parses and 25/25 complete
spoken golds**, including all 205 annotated spans. Its other cases have no spoken
gold and are not counted as correct utterances. NeMo supplies no span annotations,
so it cannot establish parser precision/recall. Its `fully_supported` count is
540/544 under the unchanged reference policy, a rendering diagnostic rather than an accuracy measure.
An additional run with `--clock-policy normalize-redundant-period` completes
541/544 renderings but still accepts 511/544 references. It normalizes `13:10pm`
to the same time as `1:10pm`; its spoken output still differs from the original
Chinese reference. The default and checked-in reference policy remain Strict.

The last two rows also select `--invalid-date-policy read-components`. They
read three impossible dates literally while retaining `invalid_date` values and
diagnostics. Combining that with redundant clock normalization completes all
544 renderings; it does not establish 544 correct calendar values or utterances.
The combined report still has 30 unaccepted reference strings and three explicit
invalid-date diagnostics. These diagnostic records remain visible even for the
three exact literal word matches. Invalid-date readings are excluded from
variant equivalence. The original automatic policy and reference policy are
unchanged; the new profiles answer a different transcription question.

The [complete reference review](SPEECH-REFERENCE-REVIEW.md) retains all 80 strict
mismatches, including the 47 accepted variants. The remaining 33 comprise sixteen
reference spelling/grammar/pronunciation defects with evidence, ten unaccepted
Arabic register/agreement differences, three invalid calendar dates, and four
wording or interpretation-domain disagreements. Adjudications do not add passes.
Identical Russian `13:05` inputs with incompatible gold impose a literal ceiling
of 543/544 under a fixed policy; both valid clock wordings can be represented by
the separate variant metric.

### Instrumented source coverage

The fresh report contains 142 owned source files, with one absolute source path
per file. The earlier cached report containing `/src/src/...` aliases was discarded.

| Measurement | Covered / instrumented | Percent |
|---|---:|---:|
| Entire library and CLI, lines | 27,838 / 31,522 | 88.31% |
| Entire library and CLI, functions | 2,739 / 3,122 | 87.73% |
| Speech modules, lines | 14,478 / 15,427 | 93.85% |
| Phonetic modules, lines | 994 / 1,073 | 92.64% |
| eSpeak adapter, lines | 432 / 484 | 89.26% |
| Typed speech parser, lines | 427 / 435 | 98.16% |
| Exact number composition, lines | 1,010 / 1,052 | 96.01% |
| Quantity grammar, lines | 1,229 / 1,325 | 92.75% |
| Temporal grammar, lines | 1,809 / 1,951 | 92.72% |
| Reviewed variant comparator, lines | 1,057 / 1,092 | 96.79% |
| Shared scientific/duration/fraction/range shapes, lines | 515 / 521 | 98.85% |
| Regional conventions registry, lines | 35 / 35 | 100.00% |
| Shared speech lexer, lines | 374 / 375 | 99.73% |

The previous committed snapshot measured 21,013/24,437 lines (85.99%) and
2,215/2,585 functions (85.69%). Executable source grows as grammar is added;
this is not a fixed-denominator comparison. Inline Rust unit tests remain in
source coverage, matching the historical filter. JSON lexical data is not an
executable-line denominator; separate contracts check its language/unit/resource
presence. Branch and MC/DC coverage were not measured. These percentages do not
establish linguistic or acoustic accuracy.

### Verification and remaining limits

The complete all-feature/all-target suite passes **713 tests**, as does the
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
source protections. Full timestamp support remains outside those counts;
twenty timestamp probes remain complete, unexpanded identifiers. These new
formats are absent from the selected NeMo sample. All seven existing
NeMo/authored speech reports remain byte-identical to the preceding snapshot.
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

The general comparison/search quality gates still fail with unchanged measured
accuracy 73.44%, F1 79.22%, recall@5 88% and recall@10 90%. Their targets were not
lowered. Passing speech/unit tests does not make that existing quality job green.

### Reproduce current results

```sh
cargo test --locked --all-features --all-targets
cargo clippy --locked --all-features --all-targets -- -D warnings
cargo +1.90 check --locked --no-default-features --all-targets
cargo +1.90 check --locked --no-default-features --features phonetic-espeak --all-targets
cargo doc --locked --no-deps --all-features
RUSTC_WRAPPER= cargo llvm-cov --no-rustc-wrapper --locked \
  --all-features --workspace --json \
  --output-path target/coverage/goal-subseconds-direct.json \
  --ignore-filename-regex '(/tests/|/benches/|/examples/|/registry/)'
cargo run --locked --quiet --no-default-features --bin textintel -- \
  eval-speech data/speech/evaluation.json --json
cargo run --locked --quiet --no-default-features --bin textintel -- \
  eval-speech data/speech/reference-nemo.json --json
cargo run --locked --quiet --no-default-features --bin textintel -- \
  eval-speech data/speech/reference-nemo.json \
  --speech-policy data/speech/reference-policy.json --accept-variants --json
cargo run --locked --quiet --no-default-features --bin textintel -- \
  eval-speech data/speech/reference-nemo.json \
  --speech-policy data/speech/reference-policy.json \
  --clock-policy normalize-redundant-period --json
cargo run --locked --quiet --no-default-features --bin textintel -- \
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
| Authored typed regression | 209 / 209 exact parses; 25 / 25 spoken gold | Covered regression cases only |
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

The hand-authored regression includes 205 annotated typed spans across 20
language conventions. Only 25 cases contain complete spoken gold; the remaining
cases must not be counted as correct spoken outputs. Of 209 cases, 170 report
`fully_supported`; unresolved cases include intentionally ambiguous inputs and
protected identifiers. That flag is an implementation diagnostic, not accuracy.

### Historical external gaps and priorities

The NeMo sample is a separate, pinned first-row sample with 544 cases in 12
languages. No incompatible rows were removed. It supplies spoken text, not
entity boundaries. [Selection, licensing, and caveats](../data/speech/reference-nemo.README.md)
include upstream invalid dates and reading-policy assumptions.

| Feature | Exact matches / examples |
|---|---:|
| Numbers | 91 / 120 |
| Dates | 20 / 106 |
| Currency | 10 / 109 |
| Time | 10 / 99 |
| Measures | 25 / 110 |

Investigate failures before changing behavior. Examples illustrate different
causes: English `123` differs by optional “and” and hyphenation; Japanese gold
uses kanji while the experimental renderer produces kana. Conversely, Spanish
`1 enero` exposes missing named-date grammar, Russian `06.03.2006` exposes a
missing date shape, and `12KG` exposes missing case-insensitive unit recognition.
Bare `$` also requires an explicit currency convention rather than a guess.

Recommended implementation order:

1. Named months, dotted/partial date forms, weekday validation, year-reading
   conventions, and localized clock expressions. Keep invalid calendars and
   ambiguous dates protected.
2. Currency and measure dictionaries with noun gender/case, minor units,
   inflected forms, case variants, native unit symbols, and script-specific
   attachment. Interpret symbols through explicit source-locale options.
3. Ordinals, fractions, ranges, scientific notation, durations, and RFC3339
   in the experimental typed grammar. The versioned normalizer already implements
   these features with language-specific limits.
4. A shared typed representation for new grammar, with versioned adapters
   retaining the versioned normalizer’s source offsets, warnings, and identity.
   Regression tests must protect established behavior while duplicated rules
   are consolidated.
5. Context distinguishing padded counts, phone/serial/postal identifiers,
   mixed-script text, and domain abbreviations. Add independently reviewed
   minimal pairs before expanding transformations.
6. Native-speaker gold for all 20 experimental conventions, regional variants,
   held-out real application text, and paired listening tests in consuming TTS engines.

Preserve established normalization behavior while these rules develop. A failing independent
example should become a reviewed regression only after deciding its intended
locale, reading style, valid alternatives, and preservation policy. Do not
replace its gold with whatever the implementation currently emits.

Validation also passed 479 tests with all features, 446 tests with no default
features and all targets, Clippy with warnings denied, documentation without
warnings, and a Rust 1.90 all-targets check. The compiling
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

The external command deliberately exits 1 when any gold differs. CI gates the
authored regression without treating the independent challenge as already solved.
The existing analysis/search quality command was rerun and still exits 1:
comparison accuracy is 73.44% (minimum 80%), F1 is 79.22% (minimum 86%),
search recall@5 is 88% and recall@10 is 90% (both minimum 95%). Other category
thresholds also fail. The measured report is retained in the snapshot JSON.
These historical quality failures remain unresolved; their thresholds were
not relaxed. Passing unit tests and speech regression tests do not make CI’s
existing quality job green.
