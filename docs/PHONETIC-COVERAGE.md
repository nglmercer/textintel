# Twenty-language phonetic runtime audit

Measured on 2026-11-14 with the optional `phonetic-espeak` feature and a local
eSpeak NG **1.52.0** build at commit
`4870adfa25b1a32b4361592f1be8a40337c58d6c`. The
[machine-readable audit](../data/phonetic/espeak-runtime-audit.json) retains
source/build hashes, exact commands, raw tool output, processed candidates,
language tags, controls and limitations. No executable or upstream data directory
is distributed with this crate.

| Check | Observed result |
|---|---:|
| Installed voice rows parsed | 141 / 141 |
| Base-language samples in the public example | 20 / 20 executed with nonempty IPA and phonemes |
| Regional/script aliases in the public example | 29 / 29 executed with their family's voice |
| Independent provider probe | 46 distinct base/locale/alias requests executed |
| Native pronunciation or listening accuracy | Not measured |

The provider probes and example overlap; they must not be added together as
independent accuracy examples. Samples are short authored strings, not a
representative language corpus. A process succeeding and returning IPA does not
establish a correct reading.

## Defects exposed by the live tool

- Regional tags such as `es-MX`, `pt-BR` and `hi-IN` previously selected the
  English fallback. Both G2P providers now reuse one private language-family
  lookup. The requested tag stays on the candidate; its dialect identifies the
  voice actually selected, without asserting a regional accent.
- The real `--voices` table combines age and gender as `--/M`. The former parser
  expected separate columns, retained only 21 of 141 rows, and shifted metadata.
  Parsing now uses the real five-column form, excludes optional alias text from
  the voice file, and retains compatibility with the validated separate form.
- `--ipa=3` emits U+200D ties and parenthesized phoneme-table switches such as
  `(en)` and `(ja)`. These were being counted as phonemes. The eSpeak adapter
  removes recognized tool controls before IPA, stress and syllable parsing;
  unknown parenthesized content remains intact. Generic IPA tokenization is
  unchanged. The output format is defined in the pinned upstream
  [phoneme writer](https://github.com/espeak-ng/espeak-ng/blob/4870adfa25b1a32b4361592f1be8a40337c58d6c/src/libespeak-ng/dictionary.c#L457-L461)
  and [IPA option handling](https://github.com/espeak-ng/espeak-ng/blob/4870adfa25b1a32b4361592f1be8a40337c58d6c/src/espeak-ng.c#L518-L520).
- `for_voice` previously ignored the configured voice for mapped languages.
  It now forces that voice; instance capabilities describe only its known
  language family, or no declared family for an unknown named voice. The static
  catalog remains separate from instance overrides and installed availability.
- Input beginning with `-` could be interpreted as program options: `--help`
  returned help text that looked like successful IPA, while `-12` returned no
  reading. The adapter now passes an option terminator before the text. Live
  probes verify that `--help`, `-12` and `-vfr` are read as data.

The provider's 0.8 value is an uncalibrated tool-output marker. Requests whose
family differs from the selected voice, or outputs with a recognized foreign
language switch, receive 0.5; outputs without phonemes receive 0.0. Lowering this
value does not repair a reading.

## Input limits remain visible

The pinned upstream [language documentation](https://github.com/espeak-ng/espeak-ng/blob/1.52.0/docs/languages.md)
restricts Japanese to hiragana/katakana and Arabic to fully diacritized text.

The Japanese Kanji sample produces English character-name announcements. Those
announcements remain in the cleaned IPA, with reduced confidence. The kana
control produces a different reading without a foreign-language switch. Arabic
vocalized and unvocalized controls also remain separate; producing IPA for the
latter does not validate an input outside the documented restriction. No Kanji
reader or Arabic diacritizer was added.

The built-in fallback still advertises only English and Spanish rules. This
audit of an optional external backend does not implement the other eighteen
built-in grammars or certify every script, vocabulary or dialect of the twenty
language families.

## Reproduce

Build or provide the pinned local executable using the upstream
[build instructions](https://github.com/espeak-ng/espeak-ng/blob/1.52.0/docs/building.md).
The audit records the source archive checksum, compiler and configure flags used
here. Runtime dependencies remain explicit; the library downloads nothing.
The standalone probe sources are archived in the JSON provenance; the public
example below can be rerun directly from the repository.

```sh
cargo run --locked --features phonetic-espeak --example espeak -- \
  --binary target/tools/espeak-audit/install/bin/espeak-ng --json
```

An explicit missing/broken binary or a failed required probe exits 1. Without
`--binary`, an unavailable optional PATH dependency reports a skipped audit and
zero probed cases. Its exit 0 is not evidence that a language passed. Japanese
kana and Arabic vocalization controls are diagnostic additions, outside the
twenty-language success count. The example's JSON is deterministic for the same
binary, data and inputs.
