# Model directory

This directory holds the crate's trained artifacts plus (optionally) local
transformer checkpoints. The crate **never downloads anything**: fetch files
manually with the commands below, then point the providers at them.

## Shipped artifacts

- `similarity-v5.json` — logistic similarity scorer (feature schema 9).
- `spam-v2.json` — calibrated logistic spam predictor.

Legacy revisions (`similarity-v1..v4`, `spam-v1`) were removed; there is no
fallback chain.

## Curated embedding checkpoints

`TransformerEmbeddingProvider::open(<dir>)` loads a directory holding
`config.json`, a tokenizer, and `model.safetensors`. All three curated
checkpoints declare BERT wiring (`model_type: "bert"`):

| Checkpoint | Params | Dims | Tokenizer | Pooling | Prefix |
|---|---|---|---|---|---|
| `intfloat/multilingual-e5-small` | 118M | 384 | `tokenizer.json` (Unigram) | mean | `query: ` / `passage: ` |
| `Snowflake/snowflake-arctic-embed-xs` | 22M | 384 | `vocab.txt` (WordPiece) | CLS | `Represent this sentence for searching relevant passages: ` on queries |
| `mixedbread-ai/mxbai-embed-xsmall-v1` | 24M | 384 | `vocab.txt` (WordPiece) | mean | none |

Manual download (example: multilingual default into `models/e5-small`):

```sh
pip install huggingface_hub
hf download intfloat/multilingual-e5-small \
  --include 'config.json' 'tokenizer.json' 'model.safetensors' \
  --local-dir models/e5-small
```

Then:

```rust
let provider = textintel::TransformerEmbeddingProvider::open("models/e5-small")?
    .with_pooling(textintel::TransformerPooling::Mean)
    .with_text_prefix("query: ");
```

`TEXTINTEL_TRANSFORMER_MODEL=<dir>` (or `EngineBuilder::transformer_model`)
selects the checkpoint for the `production_local` preset; `models/transformer`
is the conventional location. The loader also accepts Hugging Face
`bert.`-prefixed tensor names and casts F16/BF16 weights to F32.

## Curated generative checkpoints

`LiquidInstructProvider` needs an explicit local OpenAI-compatible server —
llama-server (GGUF, CPU-friendly), Ollama, or vLLM — already serving one of:

- `LiquidAI/LFM2.5-230M` (230M, fastest CPU generation)
- `LiquidAI/LFM2.5-350M` (350M, better quality, still CPU-runnable)

Example (llama.cpp server on CPU):

```sh
llama-server -hf LiquidAI/LFM2.5-230M-GGUF:Q4_K_M -c 2048
textintel generate "summarize: ..." --liquid-endpoint http://127.0.0.1:8080/v1/chat/completions
```

The server applies the checkpoint's chat template; only `http://`
endpoints are accepted (terminate TLS in your deployment).
