# Mini Unigram fixture (mechanics only)

Deterministic tiny BERT-wiring checkpoint with a SentencePiece-Unigram
`tokenizer.json` (multilingual-e5 layout) for `TransformerEmbeddingProvider`
mechanics tests. **Random weights: carries no semantics.** Quality must be
evaluated against a real checkpoint (see the
`TransformerEmbeddingProvider` docs for the curated modern checkpoints).

- `config.json`: hidden 8, 1 layer, 2 heads, intermediate 16,
  16 positions, Unigram vocab 10, cased.
- `tokenizer.json`: Unigram subset (`<s>`/`<pad>`/`</s>`/`<unk>`, `▁` pieces,
  no byte fallback).
- `model.safetensors`: fixed-seed F32 tensors with Hugging Face
  `bert.`-prefixed names (~6 KB), covering the real-checkpoint layout
  (the WordPiece fixture covers bare names).

Regenerate: `python3 tools/generate_mini_transformer.py` (stdlib only).
