# Resource packs

`ResourceLoader` indexes every `.json` file below `resources/languages` and
`resources/symbols` recursively. The repository intentionally ships only a
small seed vocabulary for common languages (`en`, `es`, `pt`, `fr`, `de`, and
`it`); it is not a complete dictionary.

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
