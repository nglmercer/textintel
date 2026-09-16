# Modularization Audit

Date: 2026-09-16 (re-baselined 2026-09-16 against `76f2735` plus the
transformer-closeout working tree). Scope: `src/`, `tests/`, `tools/`
(Rust files only; generated files and data/resources excluded — none are
Rust sources).

Thresholds: `> 500` review, `> 800` should split, `> 1200` must split
unless strongly justified.

## Result after this audit

One file sits 10 lines over the split threshold with a documented
justification (`src/engine/analyzer.rs`, 810 — see below); everything else
is at or under 800. The `production-quality` commit landed after the
previous audit and grew several files past their audited counts, so this
pass re-baselines every count below against the current tree. Every split
is behavior-preserving (public API unchanged) and was verified with
`cargo fmt --check`, `cargo test --locked --all-features --all-targets`,
and `cargo clippy --locked --all-targets --all-features -- -D warnings`.

## Splits performed (this pass)

### 4. `src/entities.rs` (1132 → 10 modules, largest 383)

Was: one file owning the rule-based provider, all eight scanner families,
span claiming/normalization, and the agreement/conflict evidence functions.

Now (`src/entities/`):

| File | Lines | Responsibility |
|---|---|---|
| `mod.rs` | 32 | module docs, `DEFAULT_MAX_*` bounds, public re-exports |
| `provider.rs` | 383 | `RuleBasedEntityProvider`, scan orchestration, agreement/conflict/evidence |
| `normalize.rs` | 47 | span claiming, casefolding, mention construction |
| `url.rs` | 50 | URL spans |
| `email.rs` | 80 | email spans |
| `mention.rs` | 57 | `@handle` spans |
| `numeric.rs` | 66 | standalone numbers |
| `currency.rs` | 146 | currency amounts (`normalize_amount` shared with `numeric`) |
| `datetime.rs` | 150 | ISO/slashed dates, times |
| `names.rs` | 198 | title-case spans, acronyms, org suffixes |

Notes:

- Exact code moves: scanners became `pub(crate)` free functions, `scan_names`
  stayed an inherent `impl RuleBasedEntityProvider` block in `names.rs`, and
  the five unit tests moved with their modules (names → `names.rs`, the rest
  → `provider.rs`). `crate::entities::{entity_agreement, entity_conflict,
  entity_evidence_lines, RuleBasedEntityProvider, DEFAULT_MAX_*}` paths are
  unchanged, as are the `textintel::` re-exports.
- Verified with `cargo fmt --check`, the entities unit tests (5/5),
  `cargo test --locked --all-features --all-targets`, and clippy `-D warnings`.

## Earlier splits (kept)

## Splits performed

### 1. `src/engine/analyzer.rs` (1752 → 764 + 6 modules)

Was: one `impl TextIntelligence` block owning construction, analysis,
comparison, patterns, search, and diagnostics.

Now (`src/engine/`):

| File | Lines | Responsibility |
|---|---|---|
| `mod.rs` | 67 | `TextIntelligence` struct + API re-exports |
| `analyzer.rs` | 764 | text → fingerprint pipeline, rebus decode |
| `builder.rs` | 357 | constructors, `EngineBuilder` assembly, `with_*` |
| `comparison.rs` | 127 | pair scoring, duplicates |
| `diagnostics.rs` | 237 | capabilities, health, diagnostics report |
| `patterns.rs` | 137 | pattern registry, matching, spam detection |
| `search.rs` | 161 | document store, similarity search |
| `production.rs` | 483 | preset, diagnostics types (unchanged) |

Notes:

- The struct moved to `mod.rs` so every submodule reaches its fields;
  `engine::analyzer::TextIntelligence` keeps resolving via re-export.
- `analyzer.rs` at 764 stays review-tier deliberately: `analyze_stages`
  is one cohesive pipeline building one fingerprint, and splitting it
  further would be arbitrary.
- `patterns.rs` instead of the suggested `persistence.rs`: the module owns
  the pattern registry (match + file save/load), not generic persistence.

### 2. `tests/core_engine_coverage.rs` (1219 → 5 modules)

Was: one file covering engine entry points, normalization, lexical, and
`core::{config, error, types}`.

Now `tests/core_engine.rs` (+ `#[path]` modules in `tests/core_engine/`):

| File | Lines | Behavior area |
|---|---|---|
| `entry_points.rs` | 368 | analyze/compare/decode/batch, accessors |
| `normalization.rs` | 170 | unicode/confusables/leetspeak/repetition/whitespace |
| `lexical.rs` | 243 | character metrics/similarity/ngrams/minhash/tokenizer |
| `config.rs` | 196 | `core::config` limits and weights |
| `error_types.rs` | 236 | `core::{error, types}` display/builders |

No shared helpers were factored out: every test builds its own engine
inline, so there is no duplicated setup to share. 76/76 tests pass
unchanged.

### 3. `src/evaluation.rs` (603 → 185 + 3 modules)

Was: dataset parsing, report aggregates, and evaluation runners in one
file (review-tier, but parsing + reporting + running together).

Now (`src/evaluation/`):

| File | Lines | Responsibility |
|---|---|---|
| `dataset.rs` | 269 | cases, splits, spam corpus (parsing) |
| `report.rs` | 182 | metric aggregates, options (reporting) |
| `gates.rs` | 204 | threshold files (moved from the CLI) + unit tests |
| `metrics.rs` | 400 | scoring primitives (unchanged) |
| `evaluation.rs` | 185 | runners + re-exports (public paths unchanged) |

Notes:

- `check_gates` moved from `src/bin/textintel/eval.rs` into the library
  so gates are unit-tested (fail-closed spam, min/max, unknown-section
  tolerance); the CLI calls the shared function.
- The runner (`evaluate_with_options`) stays whole: it is one cohesive
  flow, and per-slice scoring already lives in `metrics.rs`. Extracting
  `categories`/`search`/`spam` runner slices would be arbitrary.
- Leakage checks stay in `tests/leakage.rs` (harness-level dataset
  assertions need no library API).

## Reviewed and kept

| File | Lines | Justification |
|---|---|---|
| `src/engine/analyzer.rs` | 810 | **Over threshold by 10, justified:** still one cohesive `analyze_stages` pipeline building one fingerprint; the growth since the last audit (764 → 810) is the semantic-provenance block inside the same pipeline function (`semantic_model`/`semantic_revision`/`semantic_quality` metadata), not a new responsibility. Splitting the pipeline mid-function would be arbitrary. |
| `src/core/types.rs` | 791 | Shared DTOs (`MessageFingerprint`, `ComparisonResult`, …). Splitting them across files invites circular imports for no responsibility gain; central types are the correct shape. |
| `src/storage/memory.rs` | 746 | One responsibility: the in-memory multi-channel index (lexical, symbol, normalized, minhash, semantic, phonetic, transliteration, decoded, concept, character) plus the bounded candidate-union retrieval over it. |
| `src/phonetic/espeak.rs` | 585 | One responsibility: the espeak-ng backend client (voices, IPA, provider). |
| `src/resources/loader.rs` | 583 | `ResourceLoader` core; validation already extracted to `loader/validation.rs` (260). Further division would be arbitrary. |
| `src/comparison/scorer.rs` | 530 | One cohesive fingerprint-pair scoring function plus channel helpers; `decoded`/`swap` already extracted. |
| `src/resources/index.rs` | 519 | One responsibility: the language index. |
| `src/engine/production.rs` | 525 | Builder + preset + diagnostics types; under threshold. |
| `src/detection/spam.rs` | 487 | Heuristic predictor + shared spam features; under threshold. |
| `src/bin/textintel/main.rs` | 450 | CLI dispatcher; `eval` already extracted to `eval.rs` (173). Under threshold. |
| `src/comparison/model/` | ≤ 440 | Already modular (`model.rs`, `model/features.rs`, `model/optim.rs`). |
| `src/semantic/transformer*` | ≤ 470 | Already modular (`transformer.rs` + `transformer/`). |
| `src/entities/*` | ≤ 383 | Split this pass (see above). |
| `tests/production_v5.rs` | 575 | Production v5 behavior suite; under threshold. |
| `tests/production.rs` | 471 | Production integration suite; under threshold. |
| `tests/rust_mvp.rs` | 482 | MVP contract suite; under threshold. |
| `tests/transformer_semantics.rs` | 371 | Transformer evidence routing suite (new this pass); under threshold. |
| `tools/train_similarity/*` | ≤ 440 | Trainer already split (`similarity.rs`, `spam.rs`, `metrics.rs`). |

All other files are under 440 lines with a single responsibility each;
small files were not split for consistency.
