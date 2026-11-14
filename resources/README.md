# Resource packs

`ResourceLoader` indexes every `.json` file below `resources/languages` and
`resources/symbols` recursively. The repository ships 23 language profiles:
the twenty canonical speech languages plus the existing Italian, Dutch and
Polish profiles. Thirteen canonical languages and those three extras use the
original starter vocabularies. Bengali, Marathi, Punjabi (Gurmukhi), Tamil,
Telugu, Urdu and Vietnamese now use pinned UDHR prose profiles. None is a
complete dictionary. The loader has no language allow-list: every valid
language pack discovered on disk is indexed.

The [UDHR attribution/permission notice](UDHR-NOTICE.md) and
[provenance manifest](udhr-provenance.json) document the seven new profiles.
Preamble/articles 1–20 populate profiles; articles 21–30 remain held out.
The held-out legal text and seven handwritten prose challenges are evaluated
separately, without detector-threshold changes. See the
[shared-core audit](../docs/SPEECH-CORE-AUDIT.md) for measured results and limits.

Each language can be split into multiple JSON files or subdirectories. Files
with the same `language` code are merged, while every lookup record retains
its source path, origin, provenance, and license metadata.

Use `lookup_in_language` when a report must be scoped to one language. A plain
`lookup` returns `NotFound`, `Unique`, or `Ambiguous`; it never silently treats
a word found in another language as an exact match.

Symbol resources use one neutral metadata pack (`00-neutral.json`) plus
localized language readings. The 16 original symbol languages retain their
existing files. `ResourceLoader::embedded()` derives the missing seven symbol
languages from shared number/currency/percentage data and pinned CLDR emoji
annotations. All twenty canonical languages then cover the same 35 baseline
tokens: 0–10, 100, five currency glyphs, `% + =`, and the baseline emoji set.
Neutral concept IDs remain intact. Currency symbol names are possible readings;
they do not resolve an ambiguous currency amount in the speech parser.

Custom loaders retain their declared scope. To add missing speech resource
seeds explicitly after loading a directory, call:

```rust
let mut resources = textintel::ResourceLoader::from_resource_root("resources")?;
resources.add_speech_resources()?;
# Ok::<(), textintel::ResourceError>(())
```

That method fills missing language/symbol-language packs, preserves existing
ones, enforces configured resource limits, and rolls back on failure. An empty
custom loader receives bounded number/month/unit domain profiles; the UDHR
prose is loaded from its physical profile files by the embedded or directory
loader. No separate numerical dictionaries are maintained for the seven
languages. CLDR annotation attribution, pinned paths and hashes are in
[`symbol-labels.PROVENANCE.json`](../src/speech/data/symbol-labels.PROVENANCE.json).

Chat abbreviations live in `resources/abbreviations/<language>.json`
(schema version 1, same provenance fields) and are the only source of
mappings such as `u → you` or `gr8 → great`: generic core logic holds no
language-specific abbreviation table. Use
`TextIntelligence::with_abbreviation_provider` (or
`RebusDecoder::decode_with_abbreviations`) to override them per application.

Language packs use schema version `1`:

```json
{
  "schema_version": 1,
  "language": "en",
  "name": "English",
  "revision": "seed-2026-09",
  "sha256": "optional-hex-digest-with-sha256-field-removed",
  "entries": [
    {"word": "example", "lemma": "example", "stop_word": false}
  ],
  "words": ["optional", "compact", "words"],
  "stop_words": ["optional", "compact", "stop"],
  "examples": ["this is a example", "a good example"]
}
```

`revision` and `sha256` are optional metadata fields. When a digest is
provided, loading fails if the canonical JSON (with its `sha256` field removed)
does not match it. This avoids a self-referential hash while making formatting
and object-key order irrelevant. `ResourceLimits` can cap pack bytes, entries,
symbols, and readings before data is merged; this is useful when packs come
from an untrusted or user-selected directory.

The loader keeps source path, origin, revision, license, and provenance on
indexed records. Use the language-scoped lookup APIs and the deterministic
natural index order (numeric runs first, then Unicode letters, then other
characters) when generating tracking reports. Ordering never substitutes for
language-aware matching: ambiguous and missing records remain explicit.

Language detection uses the resource profiles as a local character n-gram
detector. It returns top-k probabilities and can score individual text
segments, including mixed-script/code-switched input. Larger probabilistic
models can be injected through `LanguageDetectionProvider` without changing
the resource format.

Load the complete directory without changing Rust code:

```rust
let resources = textintel::ResourceLoader::from_resource_root("resources")?;
let engine = textintel::TextIntelligence::default().with_resources(resources);
# Ok::<(), textintel::ResourceError>(())
```

Keep large dictionaries and their license/provenance outside the core crate.
Add them as packs only when their redistribution terms allow it.
