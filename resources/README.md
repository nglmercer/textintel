# Resource packs

`ResourceLoader` indexes every `.json` file below `resources/languages` and
`resources/symbols` recursively. The repository ships small seed vocabularies
for 16 languages (`ar`, `de`, `en`, `es`, `fr`, `hi`, `id`, `it`, `ja`, `ko`,
`nl`, `pl`, `pt`, `ru`, `tr`, and `zh`); they are not complete dictionaries.
The loader itself has no language allow-list: every valid language pack
discovered on disk is indexed.

Each language can be split into multiple JSON files or subdirectories. Files
with the same `language` code are merged, while every lookup record retains
its source path, origin, provenance, and license metadata.

Use `lookup_in_language` when a report must be scoped to one language. A plain
`lookup` returns `NotFound`, `Unique`, or `Ambiguous`; it never silently treats
a word found in another language as an exact match.

Symbol resources use one neutral metadata pack (`00-neutral.json`) plus
language packs per locale. The seed packs cover emoji, numbers, currency, and
math symbols. A pack's `language` fills missing reading-language fields, so
every reading remains attributable to a language.

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
