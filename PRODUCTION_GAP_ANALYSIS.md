# Production Gap Analysis

Status vocabulary only: `DONE`, `QUALITY-BLOCKED`, `EXTERNAL-BLOCKED`.
Nothing below is marked `DONE` unless a measured gate passes.

## Provenance

- Measured state: working tree over `76f2735` ("Production-quality pass:
  entities, contextual features, multi-channel retrieval, similarity-v5").
  The measured code/model/data bytes are the tree as left by this session
  (uncommitted); re-running the production eval on this tree reproduces the
  numbers below.
- Crate version: `0.2.0` (kept: production gates do not all pass, so no
  `1.0.0` transition and no version-metadata churn).
- Dataset version: `0.9.0` (`train` 1229 / `validation` 469 / `test` 482).
  Only `train`/`validation` grew (+108/+54 closeout cases); the `test`
  cases are byte-identical to 0.8.0 (only the version field changed), so
  held-out numbers are directly comparable with the previous audit.
- Similarity model revision: `similarity-v5`
  (`train-20000-iter-lr0.2-l20.0001-ds0.8.0`, feature schema 9), kept. A
  `similarity-v6` candidate (same hyperparameters, dataset 0.9.0) was
  trained and REJECTED: on 0.9.0 validation it scored F1 0.827 /
  ROC-AUC 0.895 / PR-AUC 0.949 / ECE 0.093 against frozen v5's F1 0.849 /
  ROC-AUC 0.908 / PR-AUC 0.953 / ECE 0.083, so per the replacement rule
  (replace only on validation improvement) v5 stands.
- Spam model revision: `spam-v2`
  (`corpus-2.0.0-v2-train-seed12648430`, feature schema 1, corpus 2.0.0;
  unchanged).
- Semantic model/revision: `feature-hash-v1@stable` (256d, Basic) —
  the documented fallback; no local transformer files are installed, so
  the transformer preference is unmet and stays a degraded note. The
  serving order (configured local transformer → feature-hash fallback),
  normalized embeddings, bounded tokens/batches, revision-aware caches,
  and diagnostics are all wired and tested; only the model files are
  missing.
- Provider configuration (`diagnostics --production`): feature-hash
  embeddings (fallback, Basic), rule-based G2P, char-ngram language
  detection (`resource-profile-2`), resource lexicon/symbols (16 languages)
  /abbreviations (6 languages), rule-based transliteration,
  rule-based entities (`rule_based_entities`), trained similarity
  (v5) + spam (v2, Production), memory store with indexed channels
  (lexical, symbol, normalized, minhash, semantic, phonetic,
  transliteration, decoded, concept, character), reranker disabled,
  candidate budgets (per-channel 200, ANN 100, union 500), revision-aware
  caches enabled (embeddings `feature-hash-v1@stable`). Degraded notes:
  transformer embeddings, espeak-ng G2P, reranker, persistence, ANN —
  all serving documented fallbacks, nothing touches the network.

## Held-out metrics (`--split test --production`)

| Gate | Measured | Required | Status |
|---|---|---|---|
| similarity F1 | 0.914 | ≥ 0.94 | `QUALITY-BLOCKED` |
| similarity ROC-AUC | 0.956 | ≥ 0.95 | `DONE` |
| similarity PR-AUC | 0.966 | ≥ 0.97 | `QUALITY-BLOCKED` |
| similarity ECE | 0.045 | ≤ 0.08 | `DONE` |
| language top1 / top3 | 0.935 / 0.971 | ≥ 0.80 / 0.95 | `DONE` |
| rebus top1 / top3 | 0.778 / 0.963 | ≥ 0.75 / 0.90 | `DONE` |
| hard_negatives accuracy | 0.971 | ≥ 0.90 | `DONE` |
| phonetic F1 | 0.824 | ≥ 0.70 | `DONE` |
| transliteration accuracy | 0.806 | ≥ 0.80 | `DONE` |
| spam ROC-AUC / F1 / Brier / ECE | 0.970 / 0.943 / 0.059 / 0.086 | ≥ 0.90 / 0.80 / ≤ 0.15 / 0.10 | `DONE` |
| search recall@10 | 0.920 (92/100) | ≥ 0.98 | `QUALITY-BLOCKED` |
| search MRR | 0.794 | ≥ 0.70 | `DONE` |

Per-category production gates (hard_negatives, phonetic,
transliteration, semantic f1 0.667 ≥ 0.65, cross_language acc 0.688 ≥
0.55, code_switching, leetspeak, homoglyph, unicode, short_text, spam acc
0.765 ≥ 0.75, rebus, general): all pass — `DONE`. Full numbers in the
evaluation log; command exits 1 with exactly the three failures above.

## Production gate result

`QUALITY-BLOCKED`: 3 of 13 gate lines fail (similarity F1, similarity
PR-AUC, search recall@10) — the same three blockers as the previous
audit, with byte-identical measurements (same model, same frozen test
cases), and no previously-passing gate broke. Release stays at `0.2.0`;
no `1.0.0` transition. The CI quality job runs this command, so remote CI
is red on these gates too — that is the honest signal, not a billing
artifact (billing failures remain `EXTERNAL-BLOCKED`; local verification
is authoritative).

## What changed this session

- `DONE` — Transformer closeout wiring: training follows the production
  semantic preference order (`--transformer-model <dir>` /
  `TEXTINTEL_TRANSFORMER_MODEL`, feature-hash fallback, backend recorded
  in the artifact); transformer-backed semantic retrieval, revision
  validation, ANN rebuild semantics, and diagnostics verified with tests
  (`tests/transformer_semantics.rs`, 8 tests); production diagnostics
  expose candidate budgets (library + CLI). FeatureHash stays `Basic`,
  transformer reports `Production`, unavailability stays an explicit
  degraded note.
- `DONE` — Dataset 0.9.0: +108 train / +54 validation cases across
  low-overlap paraphrases, cross-language paraphrases/cognates, false
  cognates, transliteration false friends, entity substitutions,
  single-word ambiguity, semantic hard negatives, and mixed-language
  paraphrases. `tests/leakage.rs` passes; test cases byte-identical.
- `DONE` — Retraining per the split contract (train → weights, validation
  → selection + calibration, test → final measurement only). The v6
  candidate lost to frozen v5 on validation (F1 0.827 vs 0.849, every
  other metric worse too), so v5 was kept and no test number drove any
  decision — the production test eval ran once, at the end.
- `DONE` — Search budget analysis on validation only. Finding: the gated
  recall probe ranks by exhaustive comparison (no store, no budgets), so
  per-channel budgets provably cannot move the gate — a store-path probe
  at full-store scale reproduced the CLI numbers exactly (recall@10
  0.860). On a forced union path (450 docs, cut 200), smaller budgets
  outscored larger ones (per-channel 50: 0.880 vs 200/1000: 0.730)
  because the heuristic pre-rank cut crowds out low-overlap relevants
  before the learned scorer sees them. Defaults kept (200/100/500): the
  small-corpus effect does not generalize to production scale, and the
  durable fix is a transformer semantic channel with preserved semantic
  candidates, not a smaller cap. Regression tests pin union
  deduplication, bounds, noisy-channel survival, and ANN-retrieval-only
  ranking.
- `DONE` — `src/entities.rs` (1132) split into `src/entities/` by
  responsibility (largest file 383), public API unchanged, all entity
  tests green; `MODULARIZATION_AUDIT.md` re-baselined (one file 10 lines
  over threshold with documented justification).
- `DONE` — Entity evidence preserved: agreement/conflict/missing
  semantics unchanged, rule-based channel and regression tests intact.
- `DONE` — Leakage: `tests/leakage.rs` passes (pairs, texts, families).
- `DONE` — Previous session, kept: duplicate-query grouping with graded
  relevance; production command in CI/local gates; evaluation/engine/test
  splits (see `MODULARIZATION_AUDIT.md`).

## Known limitations (why the three gates fail)

- Similarity needs +0.026 F1 and +0.004 PR-AUC. The v6 candidate shows
  why reweighting cannot close it: trained on harder data with identical
  hyperparameters, the model learned to distrust overlap channels
  (lexical weight flipped to −0.62, character 3.47 → 1.05, semantic 0.60
  → 0.04) yet validation F1 fell 0.849 → 0.827 with recall collapsing
  (0.781 → 0.748). Argument swaps ("dog bit man" / "man bit dog"),
  negations, and entity substitutions are invisible to symmetric
  order-insensitive channels — no linear reweighting separates them. The
  weakest held-out slices are cross_language (F1 0.667), semantic (0.667),
  and transliteration (0.769). Closing the gap needs a real multilingual
  transformer (unmet: no local model files) — explicitly out of scope for
  this pass, so the gates stay `QUALITY-BLOCKED` instead of being tuned
  around.
- Search recall@10 holds at 0.920 (92/100, MRR 0.794): the gated probe is
  exhaustive scorer ranking, so this gate is the same scorer-quality gap
  as similarity F1, not a retrieval-budget gap (budgets verified
  non-binding on the gate; see above). Recovering recall needs the
  transformer-backed semantic channel — future work, still
  `QUALITY-BLOCKED`.
- Calibration is preserved (temperature scaling on validation; ECE/Brier
  gates pass); no operating-point tuning was applied (validation-tuned
  thresholds do not transfer).
- Pre-existing, unchanged this session: the default (model-free) engine
  does not meet `data/quality-gates.json` — `QUALITY-BLOCKED`, out of
  production scope.
- Serving fallbacks (documented, not hidden): feature-hash instead of
  transformer embeddings, rule-based instead of espeak-ng G2P, no
  reranker weights shipped, in-memory store without ANN.
