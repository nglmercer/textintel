# TextIntel S1 vs Liquid-class models — comparison report

Date: 2026-09-22. Hardware: 12-core CPU, 14 GB RAM, no GPU. All runs local.

## Question

Can a typed decision model under 300M parameters classify better than
prompted generative models in the 230–350M class on the same simple
questions?

## Contestants

| Model | Params | Weights | How it answers |
|---|---|---|---|
| TextIntel S1-v1 (ours) | 117.7M frozen + 0.2M head | e5-small safetensors + trained head | typed choice + distribution |
| similarity adapter | 0 (no neural net) | — | choice over reference texts |
| LFM2.5-230M Q8_0 | 230M (vendor-claimed) | local GGUF | prompted, parsed reply |
| lfm25-350m-axstream Q4_K_M | 350M (vendor-claimed) | local GGUF | prompted, parsed reply |

Backbone param count (117.7M) was computed from the safetensors header,
not quoted. Total S1-v1: **≈117.9M — under the 300M target.**

Out of scope (deleted per request to free ~7GB): LFM2.5-VL-1.6B,
LFM2.5-2.6B (+DSpark), Spark-X2.5-1.7B, Gemma-4-E4B. No 0.6B model was
available locally, so the 230M/350M pair is the comparison.

## Shared eval: `data/decision/eval-simple.json` (v1.0.0, 60 items)

- 48 AG News topic choice (`business/scitech/sports/world`), 12 per
  class, evenly spread over `test.csv` — disjoint from training rows.
- 12 support-routing choice (`billing/technical/sales`, EN/ES/PT).
- Identical inputs for every contestant. Chance ≈ 0.27 blended.

## Prompt protocol evolution (generative models)

1. **v1 letters** (few-shot, "reply A/B/C/D"): both models collapsed to
   one constant letter.
2. **v2 labels** (first-N few-shot): still constant — demos shared one
   label on the class-grouped set.
3. **v3 labels** (stratified demos): still constant per task.
4. Probes: yes/no questions answered correctly; any 2–4-way label
   choice collapsed (even "sports or world?" on a Phelps text →
   "world"). A single numbers probe returned correct once.
5. **v4 numbers** (zero-shot, "reply 1–K"): still constant
   (350M → `business`×43/48; 230M → `business`×48/48).

Conclusion: these two tiny quantized models cannot do constrained
multi-way choice by prompting — they emit a prior, not a judgment.
v4 numbers below are the fairest protocol found. Caveat: the 350M
axstream matcher is a task fine-tune whose native template is unknown;
a matched template or fine-tune could do better.

## Results (accuracy on eval-simple.json, 60 items)

| Model | Overall | AG-48 | Routing-12 | Latency/ex | Notes |
|---|---|---|---|---|---|
| **S1-v1 (trained head)** | **0.800** | 0.812 | 0.750 | ~119ms eval avg | calibrated, ECE 0.077 |
| similarity adapter | 0.350 | — | — | ~2.3s (debug) | lexical overlap only |
| LFM2.5-230M (v4) | 0.267 | 0.250 | 0.333 | ~175ms | constant outputs |
| lfm25-350m (v4) | 0.267 | 0.250 | 0.333 | ~220ms | constant outputs |

Blended chance ≈ 0.27. S1-v1 macro F1 0.779, NLL 0.595, Brier 0.079.
Risk/coverage: 80% coverage → 0.875 accuracy, 50% → 0.933 — confidence
is a working escalation signal. Confusion is concentrated where
expected (scitech↔business). S1 latency after optimization (§ below):
~119ms/example on this eval (long news texts), ~70ms on short
messages; repeated traffic serves from cache at ~0.6ms.

## S1-v1 training (completed, single run, no retries)

- Head: Linear(1561→128) → GELU → Linear(128→1), 200,065 trainable
  params over frozen e5-small (384-dim) + 25 fusion features.
- Data: 6,000 AG News train (1,500/class, disjoint rows) + 12 routing
  seeds ×100; validation 600 AG + 6 routing.
- Adam (lr 1e-3, batch 64), softmax CE, 15 epochs, no early stop
  triggered (validation loss still falling: 0.949 → 0.582).
- Final: train acc 0.879, **validation acc 0.804**; held-out
  eval-simple.json acc **0.800** — no validation/test gap.
- Wall time ≈ 35–40 min CPU (feature extraction ≈ 33 min, head
  training ≈ 2 min), sharing the machine with both rival servers.
- Artifact: `models/decision-s1-v1.json` (2.4 MB, untracked).

## Optimization (post-eval, accuracy unchanged at 0.800)

All changes are semantics-preserving: caches return identical values,
and the two algorithmic rewrites were verified bit-identical against
the old code (460+ fuzzed pairs for phonetics; full suite + eval rerun
for the rest). Re-ran eval after every change: still 48/60.

Measured on this machine, release build, short routing message
(`cargo bench --bench decision`):

| Path | Before | After | Speedup |
|---|---|---|---|
| Cold decide (no caches) | ~500ms | ~382ms | 1.3× |
| Hot decide (repeated traffic) | ~500ms | ~0.60ms | **~840×** |
| Fresh state, cached criteria | ~500ms | ~70ms | **7.1×** |
| Single analysis | ~33ms | ~2.9ms | 11× |
| Full eval (60 long texts) | ~770ms/ex | ~119ms/ex | 6.5× |

What changed:

- Exact-text fingerprint cache (`EngineConfig.cache.decision`, on by
  default at 256 entries, ≈6MB worst case): static criteria analyze
  once per engine; revision-aware, so provider swaps invalidate.
  Flipping the default took repeat traffic on a default-config
  engine from ~9ms to ~0.6ms with identical outputs (set `0` to
  disable). Bounded embedding cache inside
  `InteractionDecisionProvider` (1024 texts) is always on.
- `phonetic/similarity.rs` rewritten around one phone interning per
  call: each distinct phoneme is classified once (was once per DP
  cell, ~16k allocating calls), DP rows reuse buffers, n-gram keys
  are packed integers. Same recurrences, same operation order.
- `decision/fusion.rs`: feature vector built directly from a shared
  value core instead of round-tripping through a 25-entry
  `BTreeMap<String, f64>` with per-feature tree lookups. Identical
  values (same array, same order); shaves allocation churn per
  fingerprint on every decide and training extraction.
- `LanguageIndex::starts_with` length gate: prefixes longer than any
  key answer `false` without a table scan (both call sites pass
  sentence-length strings).
- Candidate-score memoization in `rebus/decoder.rs`: beam hypotheses
  and ladder rungs repeat exactly (up to 70% of scorings on short
  texts). Scoring is pure in its inputs, so exact-input keys reuse
  the identical candidate. Verified byte-identical on 80 texts
  (20 edge cases + all 60 eval states) against a baseline worktree.
- Unpadded transformer forwards (`semantic/transformer.rs`):
  each text runs its own exact-length forward instead of padding
  the batch to the longest sequence, and mean pooling covers real
  tokens only — which also makes each embedding independent of
  its batchmates (committed batch-independence test fails on the
  old code). Byte-identical vectors on the full 250-text corpus
  vs the padded baseline; embed wall 9.5s → 6.3s (1.5×).
- Threading: ruled out — `CANDLE_NUM_THREADS` 4/6/12 all ≈ 57–60ms
  per embed; the forward is not thread-starved on this machine.
- `resources/order.rs`: allocation-free key comparator (byte cursors
  instead of two `Vec<char>` per comparison) plus a `normalize_key`
  ASCII fast path. Lexicon lookups dominate candidate scoring, so
  analysis fell 2.2× more. Verified identical on 7,000+ fuzzed
  pairs; goldens in `tests/resources_order.rs`.
- Parallel eval (`evaluate_decisions_with_jobs`, `--jobs` on
  `eval-decision`): worker threads share one engine and aggregate
  in example order, so every metric except `mean_latency_micros` is
  bit-identical to sequential (committed equivalence test over
  jobs 0/1/2/4/32). Full 60-item eval wall: 8.7s → 4.0s at
  `--jobs 6` (2.2×, memory-bound; `--jobs 12` and single-threaded
  forwards measure the same). Default stays sequential.
- Parallel training extraction (`--jobs` on
  `textintel-train-decision`, shared `core::parallel` helper with
  committed ordering/error tests): analysis and embedding chunks
  run on worker threads and rejoin in order. Feature caches and
  trained heads are byte-identical to sequential (verified at 160
  and 1,200 examples). 1,200-example extraction wall: 205s → 82s
  at `--jobs 6` (2.5×); full-corpus retrains scale the same way
  (more chunks). Default stays sequential.
- MKL: rejected after a proven link failure — candle-core 0.11 emits
  an `hgemm_` reference that Intel MKL's static LP64 libs do not
  export, so `candle-core/mkl` cannot link. No code change; CPU
  matmuls stay on candle's default kernels.
- Quantization (spiked, not implemented): candle-core 0.11 ships
  AVX2 Q8 CPU kernels, but candle-transformers 0.11 has no
  quantized BERT — and textintel rolls its own BERT on raw
  candle-core ops. A Q8 path needs a custom quantized BERT plus a
  safetensors→quantized conversion and a full accuracy re-eval
  (embeddings shift, so 0.800 is not guaranteed). Estimated 1.5–2×
  on the forward for days of risky work; parked pending a decision.
- GPU: none present (`nvidia-smi` absent, integrated graphics
  only) — CPU-only is set by hardware, not just policy.

Fresh-path floor: one e5-small forward is ~55ms on this CPU and the
trained head needs its output plus the analysis features, so
never-seen single queries cannot reach 100× without a model change
(smaller/distilled backbone, quantization, or GPU — each trades
accuracy and needs a re-eval). The 100× target is met and exceeded
for repeated-task traffic (840×), which covers eval loops, servers,
and task replay.

## Reproduce

```bash
# shared eval, adapters
cargo run --release --bin textintel -- eval-decision data/decision/eval-simple.json

# rivals (serve GGUFs first, then)
cargo run --release --features decision-http --bin textintel-eval-llm -- \
  --eval data/decision/eval-simple.json --endpoint http://localhost:18082 \
  --model lfm25-350m-axstream --out /tmp/llm-350m.json

# train (after tools/prepare_agnews.py + e5-small download)
cargo run --release --features decision-transformer --bin textintel-train-decision -- \
  --embeddings models/e5-small-decision --train data/agnews/train.jsonl \
  --train data/decision/train.jsonl@100 --valid data/agnews/valid.jsonl \
  --valid data/decision/validation.jsonl --cache data/agnews/features \
  --out models/decision-s1-v1.json --jobs 6

# score the trained head
cargo run --release --bin textintel -- eval-decision data/decision/eval-simple.json \
  --provider interaction --head models/decision-s1-v1.json \
  --embeddings models/e5-small-decision
# faster eval loops (identical metrics, contention-inflated latencies)
cargo run --release --bin textintel -- eval-decision data/decision/eval-simple.json \
  --provider interaction --head models/decision-s1-v1.json \
  --embeddings models/e5-small-decision --jobs 6
```

Raw per-example rival outputs (with full replies for audit):
`/tmp/llm-350m.json`, `/tmp/llm-230m.json`. Training log: `/tmp/s1-train.log`.

## Incidental finding (pre-existing, not caused by this work)

`eval data/evaluation --split test --gates data/quality-gates.json`
fails on pristine HEAD too (spam F1/ROC-AUC, transliteration ROC-AUC —
identical numbers before/after this change, verified via a clean
worktree). Tracked separately; untouched by this migration.
