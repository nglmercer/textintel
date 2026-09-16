# Production Gap Analysis

Status vocabulary only: `DONE`, `QUALITY-BLOCKED`, `EXTERNAL-BLOCKED`.
Nothing below is marked `DONE` unless a measured gate passes.

## Provenance

- Measured state: this commit's tree (parent `89b4322`); the measured
  code/model/data bytes are committed here with no post-measurement
  changes — re-running the production eval on a clean checkout of HEAD
  reproduces the numbers below.
- Crate version: `0.2.0` (kept: production gates do not all pass, so no
  `1.0.0` transition and no version-metadata churn).
- Dataset version: `0.8.0` (`train` 1121 / `validation` 415 /
  `test` 482). Only `train`/`validation` grew; the `test` cases are
  byte-identical to 0.7.0 (only the version field changed), so
  held-out numbers are directly comparable with the previous audit.
- Similarity model revision: `similarity-v5`
  (`train-20000-iter-lr0.2-l20.0001-ds0.8.0`, feature schema 9),
  trained on `train` only, calibrated on `validation`, selected on
  validation (replaces v4 per the replacement rule).
- Spam model revision: `spam-v2`
  (`corpus-2.0.0-v2-train-seed12648430`, feature schema 1, corpus 2.0.0;
  unchanged).
- Semantic model/revision: `feature-hash-v1@stable` (256d, Basic) —
  the documented fallback; no local transformer files are installed,
  so the transformer preference is unmet and stays a degraded note.
- Provider configuration (`diagnostics --production`): feature-hash
  embeddings (fallback), rule-based G2P, char-ngram language detection
  (`resource-profile-2`), resource lexicon/symbols (16 languages)
  /abbreviations (6 languages), rule-based transliteration,
  rule-based entities (`rule_based_entities`), trained similarity
  (v5) + spam (v2, Production), memory store with indexed channels
  (lexical, symbol, normalized, minhash, semantic, phonetic,
  transliteration, decoded, concept, character), reranker disabled.
  Degraded notes: transformer embeddings, espeak-ng G2P, reranker,
  persistence, ANN — all serving documented fallbacks, nothing touches
  the network.

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
transliteration, semantic f1 0.667 ≥ 0.65, cross_language,
code_switching, leetspeak, homoglyph, unicode, short_text, spam acc
0.765 ≥ 0.75, rebus, general): all pass — `DONE`. Full numbers in the
evaluation log; command exits 1 with exactly the three failures above.

## Production gate result

`QUALITY-BLOCKED`: 3 of 13 gate lines fail (similarity F1, similarity
PR-AUC, search recall@10) — the same three blockers as the previous
audit, no previously-passing gate broke. Release stays at `0.2.0`; no
`1.0.0` transition. The CI quality job runs this command, so remote CI
is red on these gates too — that is the honest signal, not a billing
artifact (billing failures remain `EXTERNAL-BLOCKED`; local
verification is authoritative).

## What changed this session

- `DONE` — Production-quality pass: rule-based entity evidence
  provider, contextual semantic features with compatibility-gated
  transliteration, multi-channel candidate union with a semantic ANN
  channel (bounded per-channel budgets), dataset 0.8.0 (`train` 1121 /
  `validation` 415), and `similarity-v5` trained on `train`,
  calibrated and selected on `validation` only.
- `DONE` — Leakage: `tests/leakage.rs` passes; no test number drove
  any keep/drop decision (test measured once at the end, plus one
  byte-identical output-capture re-run with no code changes between).
- `DONE` — Previous session, kept: search retrieval probe fixed. The
  old probe scored one query
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

- Similarity needs +0.026 F1 and +0.004 PR-AUC. v5 was selected on
  validation (beating v4 there) yet test F1 moved 0.920 → 0.914 while
  PR-AUC moved 0.965 → 0.966: near-boundary churn between retrains is
  noise, not signal, and per the no-test-tuning rule v5 stands as the
  validation winner. The misses need meaning, not reweighting:
  paraphrases with disjoint lexicons, transliteration false friends
  that outread true pairs on cross-script channels, and single-word
  rescues that are zero-sum against matched negatives. The weakest
  held-out slices are cross_language (F1 0.667), semantic (0.667),
  and transliteration (0.769) — all pass their category gates but show
  the feature-hash fallback cannot carry paraphrase meaning. Closing
  the gap needs a real multilingual transformer (unmet: no local
  model files) — explicitly out of scope for this pass, so the gates
  stay `QUALITY-BLOCKED` instead of being tuned around.
- Search recall@10 moved 0.970 → 0.920 (92/100, MRR 0.794) under the
  multi-channel retrieval rewrite: per-channel budgets and the union
  ranker trade ranking depth for bounded cost, and the semantic
  channel rides the same feature-hash fallback. This is a measured
  movement inside an already-failing gate, not a broken passing gate;
  no test-driven adjustment was made. Recovering recall needs the
  transformer-backed semantic channel plus validation-driven budget
  analysis — future work, still `QUALITY-BLOCKED`.
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
