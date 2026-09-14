# TextIntel — Production Gap Analysis

Audited against the Production-Ready Implementation Specification (§0–§95)
at repository HEAD (`textintel 0.2.0`, rustc 1.97.1). Statuses used:
`DONE` · `PARTIAL` (works, needs production hardening) · `MISSING`
(nothing or only a trait stub exists) · `BLOCKED-EXTERNAL` (needs a
resource outside this repository: binary, model download, CI billing).

> GitHub Actions: remote workflow execution is unavailable because of
> account/billing limitations. Per §1 this is recorded as
> `BLOCKED-EXTERNAL` wherever CI status would matter and is **not** treated
> as an implementation defect. Local verification (§36) is authoritative.

## 0. Architecture (§3) — DONE

The layered pipeline (Unicode → language/symbol readings → lexical /
semantic / rebus → `MessageFingerprint` → compare / search / detect) is
implemented and must be preserved, not rewritten.

- Relevant files: `src/lib.rs`, `src/engine/analyzer.rs`,
  `src/core/types.rs` (`MessageFingerprint`, `ComparisonResult`).

## 1. Presets & builder (§5, §60, §61, §62, §82) — MISSING

| Requirement | Status | Files | Missing work |
|---|---|---|---|
| `TextIntelligence::production_local()` | MISSING | `src/engine/analyzer.rs` | constructor attempting resource packs + trained models + espeak G2P with fallback + diagnostics |
| `TextIntelligence::builder()` + `EngineBuilder` | MISSING | `src/engine/analyzer.rs` | builder with `semantic_provider / g2p_provider / resources / trained_similarity_model / trained_spam_model / build()` |
| `engine.diagnostics()` (rich) | PARTIAL | `src/engine/analyzer.rs` (`provider_capabilities`, `health_check` exist; CLI has `diagnostics`) | API/fingerprint-schema/resource-revision/model-revision/ANN/degraded-capability report |
| `engine.resource_manifest()` (§28) | MISSING | `src/resources/loader.rs` | source / license / revision / sha256 per loaded pack |
| Reproducibility metadata (§62) | PARTIAL | `src/core/types.rs` (`metadata` map exists) | populate library + schema + resource + model revisions |

Required tests: `production_local` falls back gracefully with `espeak-ng`
absent; builder rejects invalid config without panic; diagnostics list every
configured provider and every degraded capability.

## 2. Capability tiers (§6, §49, §50, §13–§14) — MISSING (parts PARTIAL)

- `CapabilityLevel::{Unavailable, Basic, Production}` does not exist.
  `ProviderCapabilities` (`src/core/capabilities.rs`) has
  `provider/version/local/batch/languages/dimensions` but no quality tier,
  model revision, or fallback status.
- Honest mapping once added: `RuleBasedG2PProvider` → Basic,
  `EspeakNgG2PProvider` → Production, feature-hash/static embeddings →
  Basic, `CandleEmbeddingProvider` (static mean-pool, see §4) → Basic,
  HTTP embeddings → Production(remote), heuristic spam → Basic, trained
  spam/similarity artifacts → Production.
- `ChannelAvailability` + `channel_available/confidence` maps exist on the
  fingerprint/comparison (`src/core/types.rs`); weight renormalization on
  missing channels is implemented (`SimilarityWeights`, `src/core/config.rs`).
  §49/§50 → PARTIAL (needs per-channel provider + quality tier surfacing).

Required tests: every bundled provider reports the documented tier;
`RuleBasedG2PProvider` advertises only its real language coverage and low
confidence outside it (§14).

## 3. Abbreviations in core (§7) — MISSING (violation present)

- `chat_readings()` in `src/rebus/tokenizer.rs:138` hardcodes
  `u→you/tu, r→are, ur→your/you're, b4→before, gr8→great, l8r→later`.
- No `resources/abbreviations/` directory, no `AbbreviationProvider` trait
  (`src/core/providers.rs` has none).

Proposed implementation (Phase 13): `resources/abbreviations/{en,es,pt,fr,
de,it}.json` (schema from §7), embed via `build.rs`, index in
`ResourceLoader::abbreviation_readings()`, new `AbbreviationProvider` trait
(implemented by `ResourceLoader`), tokenizer consults the provider;
zero language-specific literals remain in `src/rebus/tokenizer.rs`.

Required tests: `u/r/b4/gr8` decode via resources; removing/overriding a
pack changes behavior (proves no hardcoding); hard-negative
(`u` must not force-decode inside unrelated words).

## 4. Symbol coverage (§8, §9, §27, §29) — PARTIAL

- Language packs exist for all 16 required languages
  (`resources/languages/`). Symbol packs exist only for
  `en es fr de it pt` + `00-neutral.json` + 6 currency packs. **Missing:
  `ar hi id ja ko nl pl ru tr zh`** (§8).
- Reading schema has `text/language/probability/reading_type`
  (`src/core/types.rs:108`); **no `source`/provenance field** (§8, §28).
- Neutral concepts use bare English ids (`house`, `money`, `love`, …),
  not namespaced `concept:building.house` ids (§9).
- Resource validation exists (`ResourceLimits`, `ResourceError`,
  `src/resources/loader.rs`, `src/resources/error.rs`) — §29 PARTIAL
  (verify NaN/inf probability, hash, and oversize rejection with tests).

Proposed implementation (Phase 14): one pack per missing language covering
digits 0–10, 100, `$ € £ ¥ ₹`, `% + =`, `❤ ❤️ 🏠 💰 🔥` + a few
culturally-appropriate extras; add optional `source` to `SymbolReading`;
document namespaced concept ids as the follow-up (§9).

Required tests: per-language pack test (house/money readings resolve);
`❤️`/ZWJ sequences stay single graphemes (§55); invalid pack rejected.

## 5. Semantic provider (§10, §11, §63, §87) — PARTIAL

- Real providers: `FeatureHashEmbeddingProvider` (Basic fallback),
  `StaticEmbeddingProvider`, `CachedEmbeddingProvider`
  (`src/semantic/embeddings.rs`), `HttpEmbeddingProvider`
  (`src/semantic/http.rs`), `CandleEmbeddingProvider`
  (`src/semantic/candle.rs`, feature `semantic-candle`).
- **Gap:** Candle is a static WordLevel mean-pool backend, not a contextual
  transformer (no E5/BGE-M3-class encoder, no ONNX). No
  `TransformerEmbeddingProvider::open("./models/bge-m3")` abstraction (§10).
- Features `semantic-transformer` / `production-local` / `all` (§63) do not
  exist (`semantic` is an alias of `semantic-local`).
- Paraphrase/cross-language/negative semantic evaluation (§11): evaluation
  harness exists (`src/evaluation.rs`, `data/evaluation.json`) but semantic
  category depth is unverified.

Proposed implementation (Phase 16): `TransformerEmbeddingProvider` trait +
  ONNX/Candle-backed struct with explicit model path, batching, metadata,
  dimension validation, normalization, truncation reporting; tiny fixture
  model for `tests/optional_transformer.rs` (§37, §87). No giant weights in Git.

Required tests: mechanics against fixture; paraphrase > unrelated
(§11 positives + negatives); cross-language pair with transformer only.

## 6. Reranker (§12, §38) — MISSING (trait + wiring DONE)

- `RerankerProvider` trait exists (`src/core/providers.rs:153`) and
  `find_similar` implements retrieve → full-score → optional-rerank →
  top-N (`src/engine/analyzer.rs:948`). **Zero implementations exist.**
- Retrieval channels (lexical, MinHash, normalized, symbol, phonetic,
  semantic ANN) exist in storage (`src/storage/`); ANN is HNSW behind
  `ann-hnsw` (retrieval-only — verify no ANN-score-as-probability leak).

Proposed implementation (Phase 20): `ChannelScoreReranker`
(logistic over channel-agreement features, bounded to top-N, optional
weights artifact); keep default engine model-free.

Required tests: rerank improves MRR on a fixture where retrieval order is
wrong; bounded to `max_reranker_candidates`; invalid outputs clamped.

## 7. Phonetics (§13–§16, §88) — PARTIAL

- `EspeakNgG2PProvider` exists with voice mapping, IPA parsing
  (`parse_espeak_ipa`), syllable estimates (`src/phonetic/espeak.rs`,
  `src/phonetic/ipa.rs`); **no `auto_detect()` constructor** (§13).
- `espeak-ng` binary is **not installed** in this environment
  (`BLOCKED-EXTERNAL` for live §88 tests; mechanics must be tested with a
  stub binary + `with_binary()`).
- No subprocess timeout on the espeak path (§84 PARTIAL — HTTP has
  timeouts, espeak does not).
- Articulatory features + weighted distances exist
  (`src/phonetic/features.rs`, check `distance(p,b) < distance(p,a)` in
  tests); phoneme 2/3-gram similarity exists
  (`src/phonetic/similarity.rs:49,96`) — §15/§16 PARTIAL→likely DONE,
  needs explicit inequality tests.

Required tests: `auto_detect()` ok/missing-binary paths; stub-binary
multilingual IPA (`hola/hello/bonjour/привет/你好`); distance inequalities;
unknown-script low-confidence (§14).

## 8. Rebus decoder (§17–§20, §26, §71–§74) — PARTIAL

- Beam search (`src/rebus/beam_search.rs`) is bounded
  (`beam_width/max_candidates/max_symbol_readings/max_branches`) and tracks
  per-node language (§20 PARTIAL — verify transition penalty).
- Scorer has lexical/frequency/phonetic/language evidence
  (`src/rebus/scorer.rs`, `RebusEvidence`); frequency via
  `LexiconProvider::frequency` (§24/§25 PARTIAL — depends on pack weights).
- Abstention fields exist (`confidence_gap`, `strong_confidence_gap` in
  `EngineConfig`, `src/rebus/decoder.rs:256`) — §19 PARTIAL (verify
  `Vec::new()`/uncertain on low score / small gap / implausible candidate).
- Missing: configurable/learnable weight vector (§17), bounded top-N
  semantic rescoring (§26), segmentation of prefix+symbol+suffix and
  syllable/phoneme-fragment replacement (§18 — verify `Fra🏠do →
  fracasado`, `salU2 → saludos` generalize via resources, never hardcoded
  per §72), mixed-obfuscation chains (`Fr4🏠d0`, §74), transformation
  provenance graph (§75), span/offset preservation (§76/§77).

Required tests: multilingual rebus set (§89: symbol-in-word, number-in-word,
emoji-as-word, emoji-as-syllable, mixed-language); hard negatives per
feature (§71, e.g. `Fra🏠do ↔ ferrocarril` negative); adversarial
normalization ladder (§73); no-exact-string hacks (§72 — enforce by
resource-removal test).

## 9. Language detection (§21, §22, §52) — PARTIAL

- `NgramLanguageDetector` + `ProfileLanguageDetector`
  (`src/language/`), segmentation with provider
  (`src/language/segmentation.rs`). Short-text uncertainty (§22),
  segment-level probabilities, code-switching depth, and optional
  fastText/CLD-class provider (§21) unverified.
- Cross-language rebus must stay language-local-first without collapsing
  mixes (§52) — verify with `I ❤️ casa`-class test.

## 10. Transliteration (§23) — MISSING

No provider, no trait, no views (`privet/привет`, `ni hao/你好`,
`salam/سلام`). Proposed: `TransliterationProvider` trait + small rule-based
provider for a few scripts, stored as additional non-destructive views.
Fingerprint has `normalization_views` map — reuse, do not replace `raw`.

## 11. Similarity / spam models (§30–§33, §70) — PARTIAL

- Artifacts exist (`models/similarity-v1.json` logistic,
  `models/spam-v1.json`); trainer exists (`tools/train_similarity.rs`);
  heuristic spam fallback exists (`src/detection/spam.rs`).
- Gaps: artifact schema validation strictness (§30 — verify version/
  feature-names/finite-weights rejection tests), dataset category breadth +
  train/validation/test splits without leakage (§31, §70 — current
  `data/evaluation.json` has `version`+`cases`; category coverage
  unverified), calibration metrics (Brier/ECE, §33 — Brier present in
  artifact metrics; ECE + "calibrated" gating unverified), hard negatives
  density (§32 spam hard negatives).

## 12. Evaluation & gates (§34, §35) — PARTIAL

- Harness + CLI `eval` with `--split/--profile/--scorer/--gates`
  (`src/evaluation.rs`, `src/bin/textintel.rs`) exist;
  `data/quality-gates.json` exists but rebus bars (`top1_min 0.2`) are
  placeholder-low and category list (§34) is unverified. Raise only from
  measured data (§35: never fake metrics).

## 13. Search / ANN / persistence (§38–§42) — PARTIAL

- Pipeline retrieve → cheap-rank → full-compare → optional-rerank exists;
  exhaustive-scan fallback for small stores is fine, indexed retrieval for
  large stores needs benchmark proof (§38).
- HNSW behind `ann-hnsw`; persist ANN strategy (§40) undocumented; recall@k
  / latency / size tracking (§39) unverified — add benches at
  1K/10K/100K (§78).
- Fingerprint versioning + migration exists
  (`src/storage/migrate.rs`, `OLDEST_SUPPORTED_FINGERPRINT_VERSION`) —
  §41 likely DONE, needs future-schema-reject test. Pattern-definition
  reanalysis on load exists (`src/storage/patterns.rs`) — §42 likely DONE.

## 14. Limits / observability / robustness (§43, §44, §46–§48, §79–§81) — PARTIAL

- `EngineConfig` bounds exist for input/segments/beam/candidates/readings/
  recursion/documents/batch/search/branches (§43 PARTIAL — add embedding
  token length + reranker candidate caps).
- No per-stage timing diagnostics (`AnalysisDiagnostics`, §44) — MISSING.
- Fuzz targets exist for unicode/tokenization/rebus (`fuzz/`) — §47
  PARTIAL (add resource-parser, IPA-parser, scorer, migration targets +
  regression tests for crashes).
- `ChannelAvailability` exists (§49 PARTIAL, see §2); explanations/evidence
  vectors exist on `ComparisonResult` (§48 PARTIAL — audit for
  evidence-grounded strings).
- Cache: `CachedEmbeddingProvider` exists keyed by model identity (§79
  PARTIAL — extend key discipline to G2P/language/rebus or document).
- Errors are typed (`TextIntelError`, `ProviderError`) — §81 likely DONE.
- Privacy posture is local-first; HTTP providers are explicit (§45 DONE-ish,
  keep enforcing). Security hardening list (§46) needs adversarial tests.

## 15. Packaging / docs / API (§59, §63–§69, §83–§86, §90) — PARTIAL

- Public surface largely matches §59; stabilize + hide internals before 1.0.
- Features (§63): have `lang-profile semantic-local semantic-candle
  semantic-http phonetic-ipa phonetic-espeak ann-hnsw persist persist-redb
  ml`; **missing `production-local`, `semantic-transformer`, `all`**.
- README exists; must gain default vs production-local vs remote section
  (§66). Examples exist (`basic`, `patterns_spam`, `persistent_store`);
  §67 set (production_local, multilingual, rebus, custom_resources,
  transformer_embeddings, espeak, search_ann) mostly MISSING.
- CLI (§68) has analyze/explain/decode/compare/spam/batch/resources/
  diagnostics/provider-info/schema-version/eval/index/search; missing
  `duplicate`, `--production/--language/--model-path/--resource-root`
  flags (some `--json` already). JSON stability (§69) needs versioning note.
- Remote provider (§83): endpoint/auth/timeout/max-batch exist on HTTP
  provider — verify no secret logging, no unbounded retry. MSRV 1.71 with
  newer deps — verify (§65). `ModelSpec` registry (§86) MISSING (optional).

## 16. What this turn implemented (verified locally, §36)

1. **Phase 13 — DONE** — resource-driven abbreviations (§7):
   `resources/abbreviations/{en,es,pt,fr,de,it}.json`,
   `AbbreviationProvider` trait (implemented by `ResourceLoader`), zero
   language-specific literals left in `src/rebus/tokenizer.rs`; threaded
   through tokenizer → beam search → `RebusDecoder::decode_with_abbreviations`
   → engine (`with_abbreviation_provider`). Tests: `tests/abbreviations.rs`
   (4 tests, incl. empty-provider removal proof against §72 hardcoding).
2. **Phase 14 — DONE** — symbol packs for all 16 languages (§8):
   new `resources/symbols/{ar,hi,id,ja,ko,nl,pl,ru,tr,zh}.json` covering
   digits 0–10, 100, `$ € £ ¥ ₹`, `% + =`, `❤ ❤️ 🏠 💰 🔥` with
   source/revision provenance. Tests: `tests/symbols_multilingual.rs`.
3. **Phase 15 — DONE** — `CapabilityLevel::{Unavailable,Basic,Production}`
   + `quality/model_revision/fallback` on `ProviderCapabilities` with honest
   tiers for every bundled provider (§6); rule-based G2P advertises
   Latin-only coverage and low confidence on unknown scripts (§14);
   `TextIntelligence::production_local()`, `builder()/EngineBuilder`,
   `diagnostics()/EngineDiagnostics`, `resource_manifest()` (§5, §60, §61,
   §28); `EspeakNgG2PProvider::auto_detect()/is_available()` with graceful
   fallback (§13); `--production` CLI flag; `production-local` feature;
   `examples/production_local.rs`. Tests: `tests/production_local.rs` (5),
   `tests/optional_espeak.rs` (feature-gated stub-binary mechanics).
4. **Phase 20 — DONE** — real `RerankerProvider` (§12):
   `ChannelScoreReranker` (bounded logistic rescoring over recomputed
   channel evidence; sorts head, carries tail, never drops). Already wired
   into `find_similar`. Tests: `tests/reranker.rs` (4, incl. correcting an
   inverted retrieval order with real weights).

Deferred (tracked, not dropped): transformer encoder (§10/§16),
transliteration (§23), namespaced concepts (§9), eval-corpus scale-up
(§31/§70), calibration/ECE (§33), ANN persistence docs (§40), timing
diagnostics (§44), span preservation (§76/§77), `ModelSpec` (§86).
