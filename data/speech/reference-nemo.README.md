# Independent NeMo reference sample

`reference-nemo.json` contains **544 written-to-spoken examples** selected from
[NVIDIA NeMo-text-processing](https://github.com/NVIDIA/NeMo-text-processing/tree/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688),
revision `ddadfb2a38d2bc6b8cc6232c4f915eb60f500688`. This is a challenge set for
measuring agreement with external examples. It is independent of TextIntel's
authored parser fixtures and was selected without running TextIntel on candidate
rows. Unsupported formats remain in the sample.

The copied fixture content is licensed under Apache-2.0. Copyright NVIDIA
CORPORATION; the upstream test harness identifies copyright year 2021. The
unmodified upstream license is included in
[LICENSE-NEMO-APACHE-2.0.txt](LICENSE-NEMO-APACHE-2.0.txt). This attribution concerns
the imported data; it does not change the library's license. No NeMo, Pynini,
model, or other runtime dependency is introduced.

## Selection and provenance

For each supported locale available upstream and each requested category, select
the first ten nonblank, noncomment, delimiter-valid physical rows in:

```text
tests/nemo_text_processing/{locale}/data_text_normalization/test_cases_{category}.txt
```

The requested categories are `cardinal`, `date`, `money`, `time`, and `measure`.
They map to TextIntel categories `number`, `date`, `currency`, `time`, and
`measure`, respectively. The pinned repository contains 55 matching files across
12 of TextIntel's 20 locales. Some files contain fewer than ten rows. No rows
were excluded for invalid formatting or for a TextIntel limitation. The JSON
records every source file's SHA-256, byte length, selected physical line numbers,
and the SHA-256 of its test consumer. Each case also carries its original path
and physical line number.

Upstream fixture direction varies. Inspection of each pinned test consumer's
parameter order establishes `spoken~written` for the German and Russian files
and Arabic measures, and `written~spoken` for the remaining selected files. This
is not inferred from which side contains digits. The pinned
[`parse_test_case_file` implementation](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/utils.py#L37)
splits on `~`; all fields after the first represent accepted alternatives.

For `written~spoken` rows, the first field becomes `text` and **every** remaining
field becomes an accepted `spoken` output. For reversed rows, the first field
becomes the spoken output and each remaining written alternative gets its own
case. Fields retain upstream spelling, case, and whitespace; only the physical
line ending is removed. Two sampled cases have multiple spoken alternatives.
No additional acceptable answers have been invented.

`expected_spans` is `null`: these upstream fixtures provide spoken gold, not
TextIntel entity boundaries. They must not be counted as annotated parser
precision or recall examples. Cases can be loaded with `SpeechCorpus::from_file`
and passed to `evaluate_speech` for spoken agreement evaluation.

| Locale | Cases |
| --- | ---: |
| ar | 30 |
| de | 50 |
| en | 50 |
| es | 50 |
| fr | 20 |
| hi | 50 |
| ja | 50 |
| ko | 50 |
| pt | 50 |
| ru | 44 |
| vi | 50 |
| zh | 50 |
| **Total** | **544** |

Category totals: number 120, date 106, currency 109, time 99, measure 110.
The other eight supported locales have no matching fixture files at this pinned
revision and are not represented here.

## Original automatic-comparison policy

Every case uses `date_order: "reject_ambiguous"`; the language alone does not
choose a regional interpretation of numeric dates. Decimal separators are
explicit: `,` for de, es, fr, pt, ru, vi; `.` for ar, en, hi, ja, ko, zh. These
settings were chosen before evaluation and are recorded in the source metadata.
They describe this comparison configuration, not every regional convention.
An upstream fixture that assumes a different interpretation can therefore expose
a deliberate policy difference.

## Reviewed comparison policy and variants

[`reference-policy.json`](reference-policy.json) is a separate, explicit
application policy added while implementing the revealed formats. It selects
natural/orthographic rendering, category-derived interpretation hints, source
locale date conventions, currency-symbol assignments and one Arabic unit alias.
Run this policy and the original automatic mode separately; their scores do not
measure the same inference task. Neither the case strings nor original spoken
alternatives were changed.

The evaluator reports literal agreement and bounded reviewed variants as separate
counts. Named variant rules operate on validated typed values and preserve
numbers, signs, units, precision, qualifiers and surrounding prose. Invalid dates,
misspellings and unreviewed differences remain failures. `--accept-variants`
changes the exit gate, not the original-gold report. See the
[case-by-case review](../../docs/SPEECH-REFERENCE-REVIEW.md) and
[current coverage snapshot](../../docs/COVERAGE.md).

This sample has now guided implementation and rule review. Its independent
upstream origin does **not** make the current results held-out accuracy.
Unseen, independently reviewed application text is still needed for that claim.

## Reproducing and checking the sample

The JSON's source metadata is sufficient to reproduce the sample without a
runtime dependency. For each `source.files` entry:

1. Fetch `https://raw.githubusercontent.com/NVIDIA/NeMo-text-processing/` plus
   `source.revision`, `/`, and the recorded `path`.
2. Verify SHA-256 and byte length before decoding UTF-8. Fetch the recorded test
   consumer and `source.fixture_parser.path`; verify their recorded SHA-256 too.
3. Remove only line endings. Skip blank rows and comment rows; require at least
   two nonempty `~` fields. Select the first ten qualifying physical rows.
4. Apply the recorded `direction`, retain all alternatives, use the recorded
   category mapping and interpretation policy, and attach physical line numbers.

IDs follow `nemo-{locale}-{upstream_category}-{physical_line:04}`; a reversed row
with multiple written inputs adds `-v{1-based_variant}`. Verify unique IDs and
that all `text`/`spoken` values reproduce the corresponding source fields.
The license SHA-256 is recorded separately. Upstream Python was inspected as
text to determine direction; it was not executed during curation.

## What this can establish

Report this set separately from the authored structural corpus. Exact spoken
agreement counts differences in punctuation, whitespace, lexical choice, year
reading, and morphology as failures. Some differences are legitimate alternatives
missing from upstream gold. For example, the English fixture asks for a digit
reading of `13000`; TextIntel's default cardinal reading has a different purpose.
The Hindi date fixture includes `३१-०६` (31 June); rejecting an impossible calendar
date can be correct even when it disagrees with the provided speech. Bare `$`
also carries an upstream currency assumption that this parser may decline to
guess. Such examples are retained, with their original gold.

This small first-row sample is not a balanced test set, an estimate of production
accuracy, native-speaker validation, or proof of support for all 20 locales.
Passing requires agreement with these particular external fixtures; failing
requires analysis before changing library behavior. It is intended to reveal
missing formats and lexical resources without concealing unsupported cases.
