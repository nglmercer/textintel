# Coverage and speech validation snapshot

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

## External gaps and next priorities

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

## Reproduce

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
