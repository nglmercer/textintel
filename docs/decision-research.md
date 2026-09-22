# TextIntel S1 — Research Notes

Independent typed-decision architecture for TextIntel, inspired by the
*publicly observable behavior* of System-One-style classifiers. Jev's
internals are proprietary and unknown; nothing here claims to reproduce
them. Every statement below carries one label:

- **VERIFIED FACT** — confirmed in this repo or a cited primary source.
- **PUBLIC CLAIM** — vendor/marketing statement, not independently verified.
- **OPEN-SOURCE IMPLEMENTATION** — what an open project actually ships.
- **OUR HYPOTHESIS** — an untested belief guiding exploration.
- **OUR DESIGN** — a decision we made for TextIntel, benchmarked or not.

## 1. Observable behavior (the interface we learn from)

- **PUBLIC CLAIM**: System-One-style models expose bounded decisions
  (`choice`, binary truth, ordered score) returning a full probability
  distribution plus a winner, with multiple questions evaluated over
  shared context at low latency.
- **OUR HYPOTHESIS**: the useful principles are bounded outputs,
  distributions over prose, shared-state encoding, calibration, and
  selective abstention — all implementable without knowing any
  proprietary internals.
- **OUR DESIGN**: `src/decision/` implements exactly three primitives
  (`DecisionQuestion::{Choice, Binary, Score}`), distributions on every
  answer, and `Accept`/`Escalate` verdicts. No generative fallback.

## 2. What we do not know (and do not pretend to)

- **VERIFIED FACT**: no public primary source available to this project
  documents Jev's architecture, layers, dimensions, parameter count,
  tokenizer, training corpus, losses, sampler, KV-cache strategy,
  heads, calibration, quantization, or kernels.
- **OUR DESIGN**: code and docs say *Decision Model / Typed Decision
  Engine / TextIntel S1* — never "Jev clone", "Jev sampler", or "Jev
  architecture". If verified details appear, they land here first with
  sources, and only then influence code.

## 3. Open-source decision experiments

- **OPEN-SOURCE IMPLEMENTATION** (needs source capture): community
  `mini-jev` / `open-jev` style experiments reportedly pair small
  encoders (including ModernBERT-family `mmBERT-small`) with
  candidate-scoring heads. Exact APIs, licenses, and quality numbers
  were **not** re-verified against primary sources in this phase —
  treat as leads, not facts.
- **OUR HYPOTHESIS**: a per-candidate cross-encoder over small
  multilingual backbones is a sound v1; shared-state encoders (v2)
  trade accuracy for candidate-count scaling.
- **OUR DESIGN**: v1 is specified (`src/decision/transformer.rs`
  prompt format, one logit per candidate, softmax) but inference
  wiring waits for Phase 3, after adapters prove the plumbing.

## 4. Backbones and the current stack

- **VERIFIED FACT** (repo): `TransformerEmbeddingProvider` wires the
  BERT family only (`model_type = bert`), CPU Candle inference, eager
  shape validation, `tokenizer.json` (Unigram) or `vocab.txt`
  (WordPiece) plus `model.safetensors`.
- **VERIFIED FACT** (repo): `semantic::catalog` lists
  `intfloat/multilingual-e5-small` (384-dim) as a known embedding
  checkpoint.
- **OUR HYPOTHESIS**: `jhu-clsp/mmBERT-small` (ModernBERT family) is
  worth investigating for v2+ because of multilingual coverage, but it
  is **not** drop-in compatible with the current BERT-only wiring.
- **OUR DESIGN**: start on `multilingual-e5-small`-compatible BERT
  wiring; `DecisionArtifact` rejects non-`bert` backbones with an
  explicit error until Phase 6 (`decision-modernbert`) lands.

## 5. TextIntel evidence and fusion

- **VERIFIED FACT** (repo): `MessageFingerprint` already carries
  Unicode/script, lexical, phonetic, semantic, rebus, entity,
  obfuscation, and URL/email evidence; `models/similarity-v5.json`
  (logistic, schema 9) and `models/spam-v2.json` (schema 1) ship as
  cheap specialists.
- **OUR HYPOTHESIS**: fingerprint features add the most value on
  adversarial inputs (homoglyphs, leet, rebus) where pure encoders are
  weakest — but fusion must prove itself by ablation, not by assertion.
- **OUR DESIGN**: `src/decision/fusion.rs` publishes a versioned
  25-feature vector (`FUSION_FEATURE_SCHEMA_VERSION = 1`, append-only
  within a version); artifacts pin it and reject mismatches.

## 6. Calibration and selective classification

- **VERIFIED FACT** (literature, standard): temperature scaling
  (`softmax(logits / t)`) plus per-class bias is the minimal
  post-hoc calibrator; NLL/Brier/ECE are the standard fit/quality
  metrics; max-softmax confidence is a weak OOD signal on its own.
- **OUR DESIGN**: `TemperatureBias` + grid-fit temperature on
  validation only; per-task `TaskCalibration` records (temperature +
  accept threshold); risk/coverage curves at 100/95/90/80/70/50%;
  energy scores, entropy, and top1–top2 margin as escalation signals;
  explicit `NONE_OF_THE_ABOVE` criterion convention for OOD.

## 7. Baselines measured in this phase (seed data)

- **VERIFIED FACT** (measured, `data/decision` test split, 12 cases,
  default engine, from repo root): similarity adapter with trained
  `similarity-v5` scorer — accuracy 0.75, macro F1 0.737; with the
  deterministic profile scorer — accuracy 0.833, macro F1 0.833.
  Validation split (6 cases, v5): accuracy 0.667.
- **OUR HYPOTHESIS**: the profile scorer beating v5 here is
  small-seed noise (12 cases, criterion descriptions tuned for
  lexical overlap), not a real ranking. Do not conclude anything
  about scorer quality until Phase 2 datasets land.
- **OUR DESIGN**: `data/quality-gates-decision.json` pins
  seed-baseline gates (accuracy ≥ 0.65, macro F1 ≥ 0.6, ECE ≤ 0.55,
  50%-coverage accuracy ≥ 0.6) as regression guards only. Production
  promotion needs real datasets and strict gates.

## 8. Open leads (unresolved, for Phase 2+)

- Re-verify open-jev/mini-jev APIs and licenses against primary
  sources before borrowing any interface shape.
- Public latency/cost claims for hosted typed-decision APIs were not
  re-verified; our benchmarks measure local inference only.
- Training pipeline (losses, distillation, augmentation, consistency
  training) is specified in the migration brief but unimplemented;
  seed data cannot train anything.
