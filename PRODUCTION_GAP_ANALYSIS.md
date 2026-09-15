# TextIntel — Production Gap Analysis (current HEAD)

Last verified against this working tree (dataset `0.4.0`, 1122 cases).
Every number below is measured locally with
`textintel eval data/evaluation.json --gates data/quality-gates.json`
(default engine); nothing is projected or aspirational.

## Quality-gate status: PASS

```
similarity: accuracy=0.906 precision=0.959 recall=0.929 f1=0.943
similarity: roc_auc=0.949 pr_auc=0.991 brier=0.087 ece=0.147
rebus: top1=0.550 top3=0.600 top5=0.625 mrr=0.581 (n=40)
language: top1=0.568 top3=0.722 unknown_p=0.009 unknown_r=0.200 (n=1026)
search: recall@1=0.283 recall@5=0.554 recall@10=0.880 mrr=0.414 (queries=92)
quality gates: pass
```

Per-category bars (`data/quality-gates.json`, section `categories`) pass for
all twelve categories. The production preset (`--production`: trained
similarity + spam models, feature-hash semantic, rule-based G2P here) trades
precision for recall (f1=0.936, recall=0.979) and resolves the
transliteration/phonetic/rebus threshold slices (see §7); neither
configuration dominates, which is why both are gated separately.

## Requirement status

| # | Requirement | Status |
|---|-------------|--------|
| 1 | Production semantic preset | DONE |
| 2 | Transliteration | DONE |
| 3 | Symbol concepts | DONE |
| 4 | Rebus scoring | DONE |
| 5 | Transformation provenance | DONE |
| 6 | Multilingual rebus | DONE |
| 7 | Evaluation dataset | DONE |
| 8 | Calibration | DONE |
| 9 | ANN persistence | DONE (deterministic rebuild) |
| 10 | Diagnostics and performance | DONE |
| 11 | Caching | DONE |
| 12 | Fuzzing / hardening | DONE |
| 13 | CLI/API cleanup | DONE |
| 14 | Documentation | DONE (this file) |

No major requirement remains `MISSING`.

### 1. Production semantic preset — DONE

`EngineBuilder::production_local()` prefers `TransformerEmbeddingProvider`
when a local model is configured (explicit `.transformer_model(path)`,
`TEXTINTEL_TRANSFORMER_MODEL`, or `./models/transformer/` with a
`config.json`), and falls back to `FeatureHashEmbeddingProvider` otherwise.
A configured-but-unusable model is reported through
`EngineDiagnostics::degraded` (`wanted: transformer_embedding`), including
the case where the binary lacks the `semantic-transformer` feature.
`production-local = ["lang-profile", "semantic-local", "phonetic-ipa"]`
enables the local production features; `all` includes `production-local`.
Tests: `tests/production_transformer_fallback.rs`.

### 2. Transliteration — DONE

`TransliterationProvider` trait plus deterministic
`RuleBasedTransliterationProvider` (Latn/Cyrl/Arab/Hans rule tables, small
exception lexicons for short-vowel Arabic and Han). Conversions are stored as
additive `transliteration:<script>` fingerprint views and participate in
decoded-overlap comparison; `raw` is never replaced. Verified:
`privet ↔ привет`, `ni hao ↔ 你好`, `salam ↔ سلام` all match with
`decoded_similarity = 1.0`. Documented limits: Arabic short vowels fold
(`سلام` → `slam`; reverse links exactly), Han table is common-characters
only, Latin→Cyrillic is Russian-biased. Tests:
`tests/transliteration.rs` (with hard negatives).

### 3. Symbol concepts — DONE

Concept IDs are stable and namespaced (`concept:building.house`,
`concept:money.currency`, `concept:emotion.love`, plus math/nature siblings).
`resources::canonical_concept_id` normalizes bare legacy ids on load
(unknown bare ids land under `concept:legacy.*`), and
`SymbolReading`/`SymbolConcept` carry an optional `source` (pack name,
origin, or `builtin:…`). Tests: `src/resources/pack.rs` unit tests,
`tests/rust_mvp.rs` symbol coverage (26 tokens × 16 languages).

### 4. Rebus scoring — DONE

All scoring weights live in `core::config::RebusWeights`: lexical,
frequency (scale + cap), phonetic, symbol, language, context, semantic
blend, transformation-penalty cap, per-kind derivation costs, a language
switch penalty, and an in-word digit discount. JSON load/validate supports
trained vectors later (`RebusWeights::from_json`); channels renormalize so
only relative magnitudes matter. Beam search stays bounded
(`beam_width`/`max_decoded_branches` truncation, one abbreviation lookup per
word). Tests: `tests/rebus_weights.rs`.

### 5. Transformation provenance — DONE

`Transformation` carries `source`, `replacement`, `transformation_type`,
plus optional `start`/`end`/`span` (original UTF-8 slice),
`confidence`, `provider`, and `language`. The flagship chain verifies
end to end (`textintel explain "Fr4🏠d0" --language es`):

```text
Fr4🏠d0
→ 4 = a (number_reading)
→ 🏠 = casa (symbol_reading; es)
→ 0 = o (number_reading)
→ Fracasado
```

Old three-field payloads still deserialize. Tests:
`tests/transformation_spans.rs`.

### 6. Multilingual rebus — DONE

Language scoring rewards requested-language coverage (full 1.0, partial
0.8, disjoint 0.4, unknown 0.7) instead of collapsing mixes; beam paths
track the full language set with a small per-switch discount, so
mixed-language inputs stay valid. Rebus top-1 rose 0.176 → 0.550 through
resource-driven fixes only (no hardcoded strings): word-level abbreviation
expansion, literal input-space preservation, in-word digit discount
(`Fr4` → `Fra`, not `Frcuatro`), `2 → to`, `☕ → coffee/café/…` in all 16
packs, `m8/slt/bj` slang, seed-lexicon growth. Failures that remain are
out-of-scope capabilities (spelling correction: `kasa/casa`, `nite/night`;
verb readings: `j'aime`, `hou van`). Tests:
`tests/multilingual_rebus.rs`, `tests/mixed_obfuscation.rs` (hard negatives
included).

### 7. Evaluation dataset — DONE

Dataset `0.4.0` (1122 cases) covers all twelve categories
(`semantic cross_language transliteration rebus phonetic leetspeak
homoglyph unicode code_switching short_text spam hard_negatives`) with ≥8
cases each; `EvaluationReport::categories` reports full `BinaryMetrics` per
slice and `data/quality-gates.json` pins per-category bars (all pass).
Legacy cas
...[truncated 5589 chars]