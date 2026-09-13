# Resource packs

`ResourceLoader` indexes every `.json` file below `resources/languages` and
`resources/symbols` recursively. The repository intentionally ships only a
small seed vocabulary for six languages (`en`, `es`, `pt`, `fr`, `de`, and
`it`); it is not a complete dictionary. The loader itself has no language
allow-list: every valid language pack discovered on disk is indexed.

Each language can be split into multiple JSON files or subdirectories. Files
with the same `language` code are merged, while every lookup record retains
its source path, origin, provenance, and license metadata.

Use `lookup_in_language` when a report must be scoped to one language. A plain
`lookup` returns `NotFound`, `Unique`, or `Ambiguous`; it never silently treats
a word found in another language as an exact match.

Language packs use schema version `1`:

```json
{
  "schema_version": 1,
  "language": "en",
  "name": "English",
  "entries": [
    {"word": "example", "lemma": "example", "stop_word": false}
  ],
  "words": ["optional", "compact", "words"],
  "stop_words": ["optional", "compact", "stop"],
  "examples": ["this is a example", "a good example"]
}
```

Load the complete directory without changing Rust code:

```rust
let resources = textintel::ResourceLoader::from_resource_root("resources")?;
let engine = textintel::TextIntelligence::default().with_resources(resources);
# Ok::<(), textintel::ResourceError>(())
```

Keep large dictionaries and their license/provenance outside the core crate.
Add them as packs only when their redistribution terms allow it.
