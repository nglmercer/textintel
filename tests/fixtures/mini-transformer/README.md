# Mini transformer fixture (mechanics only)

Deterministic tiny BERT checkpoint for `TransformerEmbeddingProvider`
mechanics tests. **Random weights: carries no semantics.** Paraphrase
quality must be evaluated against a real checkpoint (see the
`TransformerEmbeddingProvider` docs for supported families).

- `config.json`: hidden 16, 1 × ... actually 2 layers, 2 heads,
  intermediate 32, 32 positions, WordPiece vocab 64, lowercased.
- `vocab.txt`: specials + English/Spanish/CJK pieces.
- `model.safetensors`: fixed-seed F32 tensors (~28 KB).

Regenerate: `python3 tools/generate_mini_transformer.py` (stdlib only).
