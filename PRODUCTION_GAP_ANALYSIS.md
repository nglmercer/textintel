# Production Gap Analysis

Status vocabulary only: `DONE`, `QUALITY-BLOCKED`, `EXTERNAL-BLOCKED`.
Nothing below is marked `DONE` unless a measured gate passes.

## Provenance

- Current commit SHA: `cde1e19` (measured state; this file plus
  `MODULARIZATION_AUDIT.md`, the `CHANGELOG.md` entry, and two rustdoc
  comment-only fixes commit on top with no code changes — verified with
  `git diff cde1e19 HEAD -- src/`).
- Crate version: `0.2.0` (kept: production gates do not all pass, so no
  `1.0.0` transition and no version-metadata churn).
- Dataset version: `0.7.0` (`train` 1045 / `validation` 375 / `test` 482).
- Similarity model revision: `similarity-v4`
  (`train-20000-iter-lr0.2-l20.0001-ds0.7.0`, feature schema 8). No
  `similarity-v5` shipped: four rescue interactions plus L2 sweeps were
  trained and rejected on validation (best 0.938 vs v4 0.943); per the
  replacement rule an unimproved v5 must not replace v4.
- Spam model revision: `spam-v2`
  (`corpus-2.0.0-v2-train-seed12648430`, feature schema 1, corpus 2.0.0).
- Provider configuration (`diagnostics --production`): feature-hash
  embeddings (256d, Basic), rule-based G2P (espeak-ng absent),
  char-ngram language detection, resource lexicon/symbols (16 languages)
  /abbreviations (6 languages), rule-based transliteration, trained
  similarity + spam (Production), memory store, reranker disabled.
  Degraded notes: transformer embeddings, espeak-ng G2P, reranker,
  persistence, ANN — all serving documented fallbacks, nothing touches
  the network.

## Held-out metrics (`--split test --production`)

| Gate | Measured | Required | Status |
|---|---|---|---|
| similarity F1 | 0.920 | ≥ 0.94 | `QUALITY-BLOCKED` |
| similarity ROC-AUC | 0.956 | ≥ 0.95 | `DONE` |
| similarity PR-AUC | 0.965 | ≥ 0.97 | `QUALITY-BLOCKED` |
| similarity ECE | 0.053 | ≤ 0.08 | `DONE` |
| language top1 / top3 | 0.935 / 0.971 | ≥ 0.80 / 0.95 | `DONE` |
| rebus top1 / top3 | 0.778 / 0.963 | ≥ 0.75 / 0.90 | `DONE` |
| hard_negatives accuracy | 0.971 | ≥ 0.90 | `DONE` |
| phonetic F1 | 0.788 | ≥ 0.70 | `DONE` |
| transliteration accuracy | 0.903 | ≥ 0.80 | `DONE` |
| spam ROC-AUC / F1 / Brier / ECE | 0.970 / 0.943 / 0.059 / 0.086 | ≥ 0.90 / 0.80 / ≤ 0.15 / 0.10 | `DONE` |
| search recall@10 | 0.970 (97/100) | ≥ 0.98 | `QUALITY-BLOCKED` |
| search MRR | 0.877 | ≥ 0.70 | `DONE` |

Per-category production gates (hard_negatives, phonetic,
transliteration, semantic, cross_language, code_switching, leetspeak,
homoglyph, unicode, short_text, spam, rebus, general): all pass —
`DONE`. Full numbers in the evaluation log; command exits 1 with exactly
the three failures above.

## Production gate result

`QUALITY-BLOCKED`: 3 of 13 gate lines fail (similarity F1, similarity
PR-AUC, search recall@10). Release stays at `0.2.0`; no `1.0.0`
transition. The CI quality job now runs this command, so remote CI is
red on these gates too — that is the honest signal, not a billing
artifact (billing failures remain `EXTERNAL-BLOCKED`; local
verification is authoritative).

## What changed this session

- `DONE` — Search retrieval probe fixed. The old probe scored one query
  per case while cases share query texts (first 100 test cases: 12
  distinct queries), which pigeonholes any ranker at MRR ~0.34 and
  recall@10 ≤ 0.935 — the gates measured family size, not quality.
  Queries are now distinct query texts with graded relevance (every
  similar-labelled `b` relevant). Production search went MRR 0.336 →
  0.877, recall@10 0.924 → 0.970. Regression test:
  `ranking_groups_duplicate_queries_with_graded_relevance`.
- `DONE` — Production command wired into CI/local gates (quality job
  runs default-engine and production evaluations).
- `DONE` — Evaluation split into `dataset`/`report`/`gates` modules;
  `check_gates` moved from the CLI into the library with unit tests.
- `DONE` — Engine split by responsibility; oversized coverage suite
  split by behavior (see `MODULARIZATION_AUDIT.md`). All splits
  behavior-preserving, public API unchanged, each verified with
  fmt + full tests + clippy.
- `DONE` — Leakage: `tests/leakage.rs` passes; training used `train`
  only (+`validation` NLL calibration), model selection used validation
  only, and no test number drove any keep/drop decision.

## Known limitations (why the three gates fail)

- Similarity needs +0.020 F1 (≈11 test fixes) and +0.005 PR-AUC.
  Validation error analysis shows the misses need meaning, not
  reweighting: paraphrases with disjoint lexicons, entity-vs-synonym
  swaps (need NER), transliteration false friends (`net`/`нет`) that
  outread true pairs on every cross-script channel, and single-word
  rescues that are zero-sum against matched negatives (`vaccum`/
  `vacuum` vs `paypal`/`papal`). Four features plus L2 sweeps moved
  validation F1 0.943 → ≤0.938; near-boundary churn between retrains is
  noise, not signal. Closing the gap needs architectural expansion
  (multilingual semantics, NER) — explicitly out of scope for this
  pass, so the gates stay `QUALITY-BLOCKED` instead of being tuned
  around.
- Search recall@10 is one miss short (97/100): a cross-language cognate
  and two paraphrases whose relevant documents sit below flooded or
  word-sharing distractors. The cognate class has no validation
  representative for honest targeting (the rescue interaction broke a
  validation leet-confusable and was rejected); the paraphrase misses
  need the same semantic expansion as similarity.
- Calibration is preserved (temperature scaling on validation; ECE/Brier
  gates pass); no operating-point tuning was applied (validation-tuned
  thresholds do not transfer).
- Pre-existing, unchanged this session: the default (model-free) engine
  does not meet `data/quality-gates.json` (e.g. similarity ROC-AUC
  0.814 < 0.90) — `QUALITY-BLOCKED`, out of production scope, identical
  before/after (only the search instrument changed, strictly for the
  better on MRR).
- Serving fallbacks (documented, not hidden): feature-hash instead of
  transformer embeddings, rule-based instead of espeak-ng G2P, no
  reranker weights shipped, in-memory store without ANN.
