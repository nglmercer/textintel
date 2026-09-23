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
| **S1-v1 (trained head)** | **0.800** | 0.812 | 0.750 | ~91ms eval avg | calibrated, ECE 0.077 |
| similarity adapter | 0.350 | — | — | ~2.3s (debug) | lexical overlap only |
| LFM2.5-230M (v4) | 0.267 | 0.250 | 0.333 | ~175ms | constant outputs |
| lfm25-350m (v4) | 0.267 | 0.250 | 0.333 | ~220ms | constant outputs |

Blended chance ≈ 0.27. S1-v1 macro F1 0.779, NLL 0.595, Brier 0.079.
Risk/coverage: 80% coverage → 0.875 accuracy, 50% → 0.933 — confidence
is a working escalation signal. Confusion is concentrated where
expected (scitech↔business). S1 latency after optimization (§ below):
~91ms/example on this eval (long news texts), ~64ms on short
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
and every algorithmic rewrite was verified bit-identical against the
old code (460+ fuzzed pairs for phonetics; byte-compared fingerprint,
embedding-bit, and decision dumps over an 83-text corpus plus the full
suite and an eval rerun for the rest). Re-ran eval after every change:
still 48/60.

### Batches one–two

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

### Batch three (analysis bottlenecks)

Profiling showed analysis at 3.1ms (short text) split ~57% rebus,
~33% language detection. Same bench, same machine:

| Path | Before | After | Speedup |
|---|---|---|---|
| Single analysis | ~2.9ms | ~1.59ms | 1.8× |
| Cold decide (no caches) | ~382ms | ~307ms | 1.25× |
| Single embed | ~58.4ms | ~55.9ms | 1.04× |
| Fresh / hot decide | ~70ms / ~0.60ms | ~69ms / ~0.59ms | ~1× |

What changed (all bit-identical per the 83-text dumps):

- `language/ngram.rs`: per-query word-spread counts hoisted out of
  the per-language loop, stored profiles/word-sets/IDF hashed, one
  casefold shared by profile and word split, profile chars collected
  once. Language stage 3.3× (detect 493µs → 125µs).
- `rebus/decoder.rs`: call-scoped G2P memo (the source text
  phonemized once per decode instead of per candidate) plus a lean
  `phonemes()` provider path that skips the IPA join, syllable scan,
  and per-phoneme feature labels scoring never reads; ladder views
  folded once; memo maps hashed. Scoring 135µs → 102µs per
  candidate, rebus stage 1.6×.
- `normalization/unicode.rs`: ASCII casefold fast path (NFKC is the
  identity on ASCII): 3.5µs → 27ns per call, ~80 calls per analysis.
- `rebus/beam_search.rs`: token/reading lowercases hoisted out of
  the beam loop.
- `engine/analyzer.rs`: tokens reused from segments (piece splits do
  not depend on the detector), skipping a duplicate segmentation
  pass; input length counted once.
- `cache/core.rs`, `semantic/embeddings.rs`: borrowed cache lookups
  (no key clone on hits); embedding cache keyed by exact text under
  the existing `model@revision` namespace instead of per-text
  model/revision/text tuples.
- `semantic/transformer/tokenizer.rs`: hashed vocabularies, one
  reusable Viterbi/WordPiece buffer instead of an allocation per
  attempt, lazy byte-fallback with ids precomputed at load.
- `semantic/transformer/weights.rs`: linear weights pre-transposed
  at load (measured ~neutral here — the forward is matmul-bound —
  kept as strictly less per-forward work).
- Reverted on measurement: hashing the small phoneme n-gram count
  maps (SipHash slower than B-tree integer compares, +8µs per
  similarity).

### Batch four (multi-text embeds, lexicon scans)

Single-text paths were now forward-bound (~56ms e5-small forward),
so this batch parallelized across texts and trimmed lexicon scans:

| Path | Before | After | Speedup |
|---|---|---|---|
| 4-text embed batch | ~214ms | ~69ms | **3.1×** |
| 5-text embed bench | ~279ms | ~138ms | **2.1×** |
| Cold decide (no caches) | ~307ms | ~170ms | **1.8×** |
| Single analysis / embed | ~1.59ms / ~56ms | ~1.60ms / ~57ms | ~1× |
| Full eval (60 long texts) | ~119ms/ex | ~91ms/ex | 1.3× |

What changed (all bit-identical per the 83-text dumps):

- Parallel chunk embeds (`semantic/transformer.rs`,
  `with_max_parallel`, default 4): each text runs its own
  exact-length forward on a worker and results rejoin in input
  order with the first-in-order error — independent forwards, so
  vectors match sequential encoding bit for bit. Single-text calls
  never spawn threads; `1` restores strictly sequential encoding.
  Committed equivalence test over worker counts 1/2/3/4/8 plus
  multi-chunk batching on the committed mini-transformer fixture.
- `starts_with` spaced-prefix gate (`resources/index.rs`): a
  normalized prefix with inner whitespace cannot match any key of a
  spaceless index, so it answers `false` without the table scan.
  Embedded packs verified spaceless (110 spaced strings all live in
  `examples`, which index per word); indexes rebuilt with spaced
  keys set a flag that keeps the full scan. Committed soundness
  test with a spaced-key pack.
- `lexicon_coverage` fold-stability precheck: clean lowercase words
  skip the redundant second lookup (fold is the identity there).

### Batch five (comparison scoring, lexicon gates)

Profiling showed `compare` spending ~3.7ms scoring short pairs:
transliteration evidence ran 3×, swap detection 4×, and every
character similarity recomputed Jaro inside Jaro-Winkler. Analysis
still spent ~60 lexicon lookups per rebus scoring. Criterion plus a
75-pair probe, same machine:

| Path | Before | After | Speedup |
|---|---|---|---|
| Compare short pair (probe) | ~7.17ms | ~4.55ms | **1.57×** |
| Compare long pair (probe) | ~20.8ms | ~16.6ms | 1.25× |
| `compare_obfuscated` bench | ~2.87ms | ~2.18ms | 1.31× |
| `decode_symbol` bench | ~0.99ms | ~0.80ms | 1.24× |
| `analyze_rebus` bench | ~3.68ms | ~3.34ms | 1.10× |
| Single analysis | ~1.60ms | ~1.46ms | 1.10× |
| Cold decide (no caches) | ~170ms | ~163ms | 1.04× |

(The batch-four `decision_similarity_end_to_end` figure was measured
under heavy machine load and is not comparable; current value is
~4.47ms.)

What changed (all bit-identical per the 83-text dumps plus 75
byte-compared compare pairs):

- `comparison/scorer.rs`: transliteration evidence, compatibility,
  and raw views computed once and shared across channels (was 3×
  evidence + 4× compatibility + 18 view scans); swap probe runs once
  instead of 4×; alphanumeric folds and language agreement computed
  once.
- `comparison/scorer/decoded.rs`: Phase-2 identical pairs share one
  value computed through the real similarity function (every channel
  reads 1.0 on equal non-empty inputs), skipping redundant full
  string comparisons.
- `lexical/character.rs`: Jaro computed once and reused for
  Jaro-Winkler; n-gram Jaccard restructured to hashed counts in one
  pass (exact integer sums, no union key-set); Damerau-Levenshtein
  moved from a full matrix to three rolling rows; Levenshtein/LCS
  rows pre-sized. Single `combined_character_similarity` 210µs →
  95µs (2.2×).
- `resources/index.rs`: overlong ASCII words skip point lookups
  (`contains`, `lemma`, `frequency`, `is_stop_word`, `lookup`) —
  NFKC-casefold preserves ASCII length past trimming, so they cannot
  match any key; non-ASCII still proceeds (NFKC composition can
  shrink). Short words pay one length compare. Lexical plausibility
  28µs → 16µs; long-text analysis rebus stage 1.35×. Committed
  boundary tests including a shrink-to-hit non-ASCII case.
- `rebus/decoder.rs`: beam memo buckets keyed on surface text with
  borrowed lookups; score bits plus language scope confirm, and
  transforms compare by Debug exactly like the old format key —
  identical dedup verdicts (including NaN/±0.0 corners) with no
  per-node formatting.
- `normalization/unicode.rs`: ASCII fast path for
  `strip_diacritics` (NFD is the identity on ASCII).
- Dropped on inspection: reusing stored fingerprint scripts in
  `cross_script_pair` (9µs, but changes semantics for
  hand-built/hostile fingerprints) and prefix-skipping in
  `can_split_known` (it works on whitespace-stripped text, so the
  skip never fires).

### Batch six (beam search, character metrics, scorer sharing)

Stage timings (`analyze_with_timing`/`compare_with_timing`) showed
rebus at 45–92% of every analyze, the language stage second, and the
compare scoring step at ~310µs (short pair) / ~690µs (long pair) —
each analyze re-ran per-node beam allocs, each of the six character
metrics re-collected its own `Vec<char>`, and the scorer folded the
same decoded keys three times per side. Criterion plus probes, same
machine:

| Path | Before | After | Speedup |
|---|---|---|---|
| `decision_similarity_end_to_end` bench | ~4.45ms | ~3.19ms | **1.40×** |
| Compare scoring step, short pair (probe) | ~310µs | ~217µs | **1.43×** |
| Compare scoring step, long pair (probe) | ~688µs | ~490µs | **1.40×** |
| `compare_obfuscated` bench | ~2.18ms | ~1.94ms | 1.12× |
| `decision_interaction_fresh_states` bench | ~69ms | ~61ms | 1.13× |
| `decision_analyze_state` bench | ~1.46ms | ~1.36ms | 1.07× |
| `analyze_rebus` bench | ~3.34ms | ~3.14ms | 1.06× |
| Long-text analyze, rebus stage (probe) | ~1.35ms | ~1.07ms | 1.26× |
| Short decode-text analyze (probe) | ~1.44ms | ~1.19ms | 1.20× |
| Cold / hot decide, embeds | — | — | ~1× (forward- / cache-bound) |

What changed (all bit-identical per the 8.2MB byte-compared dumps:
482 eval pairs × analyze/compare, 59 edge texts × analyze/decode,
68 edge pairs, direct character metrics including n = 0–5 n-grams,
and similarity decisions):

- `lexical/character.rs`: one char collection serves all six metrics
  (private `*_chars` cores keep each original recurrence; the public
  `&str` functions collect once and delegate). N-gram Jaccard counts
  by packed `u64` key for n ≤ 3 (every `char` fits 21 bits, so three
  pack exactly and key equality is sequence equality — no per-window
  `String`); n > 3 keeps the string-keyed path over the same windows.
  Either way the sums are exact integers, so values are unchanged.
- `rebus/beam_search.rs`: per-reading lowercase, in-word digit gate,
  substantive flag, and trailing-space check hoisted out of the
  per-node loop; next-token whitespace lookahead hoisted per
  position; `chars().next_back()` instead of `chars().last()` (O(1),
  same value); transforms moved instead of double-cloned when no
  boundary variants exist; `format!` replaced by `push_str` concat
  (identical bytes, one allocation).
- `comparison/scorer.rs` + `scorer/swap.rs`: word lists computed
  once per side and shared by single-word scope, the swap probe, and
  cross-language suppression (was three tokenizations per side).
- `transliteration/evidence.rs`: `transliteration_evidence_with_raw`
  inner variant lets the scorer pass its character channel's exact
  `f64` instead of recomputing the same raw comparison; the public
  function delegates with `None`.
- `comparison/scorer/decoded.rs`: folded decoded keys (literals plus
  candidates with confidences) built once per side and shared by the
  overlap sets, both confidence maps, and the Phase-3 anchors (was
  three fold passes plus a fourth for anchors).
- `lexical/tokenizer.rs`: token and suffix char counts hoisted out
  of the per-suffix strip loop (suffix lengths computed once per
  call).

### Batch seven (symbolic channel, detect loop, phonetic fusion)

Sub-profiling the batch-six leftovers: `symbolic_similarity` ran the
full 32×32 reading cross-product (only 15 unique texts) at ~1.8ms
per symbol-bearing identical pair — 93% of its scoring step; the
decoded fuzzy tier re-ran ~1ms of string comparisons even at a
perfect `best`; per-segment `NgramLanguageDetector` detection (~25µs
× 15 calls, uncached by default) dominates the language stage; and
each rebus candidate scoring pays ~90µs (lexical splits, G2P pair,
phonetic DPs). Criterion plus probes, same machine:

| Path | Before | After | Speedup |
|---|---|---|---|
| `symbolic_similarity`, obf-identical (probe) | ~1762µs | ~456µs | **3.9×** |
| `symbolic_similarity`, rebus-identical (probe) | ~2750µs | ~869µs | **3.2×** |
| Identical-obf compare, scoring step (probe) | ~1899µs | ~558µs | **3.4×** |
| Identical-rebus compare, scoring step (probe) | ~2957µs | ~1138µs | **2.6×** |
| `compare_obfuscated` bench (one side symbolic) | ~1.94ms | ~1.88ms | 1.03× |
| `decode_symbol` bench | ~812µs | ~791µs | 1.03× |
| Analyze / similarity / decision benches | — | — | ~1× (trims inside noise) |

What changed (all bit-identical per the same 8.2MB byte-compared
dumps, re-run against the batch-six baseline):

- `symbols/resolver.rs`: `symbolic_similarity` scores unique reading
  texts per side in first-occurrence order (purity + order-independent
  `max` ⇒ same maximum, ~4× fewer `combined` calls); the probability
  channel stays per-occurrence (same text, different probabilities)
  with lowercases folded once per unique text.
- `comparison/scorer/decoded.rs`: Phase-2 fuzzy tier prunes pairs
  whose weaker confidence is already at/below `best` (each pair
  scores `similarity × min_confidence` with similarity ≤ 1, so they
  cannot move the maximum) and skips the tier outright at a perfect
  `best`. Verified firing: on identical candidate-rich pairs the
  symbolic channel alone accounts for the whole scoring step, so the
  ~1ms tier cost is gone.
- `language/ngram.rs`: per-query words borrowed from the shared fold
  (no per-word allocation), per-word IDF/spread unit precomputed so
  the per-language loop pays one lookup per occurrence instead of
  two (same values, same order), and the redundant `scripts_in`
  set-rebuild dropped (the vec answers `contains` identically).
- `phonetic/similarity.rs`: weighted + exact edit recurrences fused
  into one pass with independent rows (each returns exactly its old
  value), and 2/3-gram packed counts fused into one pass per side
  with the general path kept for short sides or huge alphabets.
- `rebus/scorer.rs`: `can_split_known` no longer re-probes the
  whole-text hit its caller just checked (same lookups, same order,
  minus one duplicate per call).
- `transliteration.rs`: view-key folds computed lazily — cheap
  rejections (unchanged/empty outputs) run before either fold, and
  texts with no matching script never fold at all.
- `language/segmentation.rs`: URL checks lowercase only when the
  first byte permits an `http(s)://` match (`to_ascii_lowercase`
  never mints ASCII from non-ASCII, so other pieces skip the copy).

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
