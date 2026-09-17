#!/usr/bin/env python3
"""Generate tiny deterministic fixtures for transformer mechanics tests.

Writes tests/fixtures/mini-transformer/{config.json,vocab.txt,model.safetensors}
(bare tensor names, WordPiece) and
tests/fixtures/mini-unigram/{config.json,tokenizer.json,model.safetensors}
(`bert.`-prefixed tensor names, SentencePiece-Unigram) with fixed-seed
pseudo-random F32 weights. No third-party packages required: the safetensors
container is assembled with struct+json only.

Regenerate from the repository root:

    python3 tools/generate_mini_transformer.py
"""

import json
import random
import struct
from pathlib import Path

SEED = 20260914
HIDDEN = 16
LAYERS = 2
HEADS = 2
INTERMEDIATE = 32
MAX_POSITIONS = 32

# WordPiece vocabulary: specials first (ids matter), then latin pieces,
# continuations, a few CJK tokens, and filler to reach VOCAB_SIZE.
SPECIALS = ["[PAD]", "[UNK]", "[CLS]", "[SEP]", "[MASK]"]
WORDS = [
    "hello", "world", "buy", "ticket", "now", "today", "money", "house",
    "casa", "dinero", "fuego", "the", "a", "is", "this", "test", "with",
    "words", "and", "some", "more", "text", "here", "hola", "mundo",
    "ni", "hao", "50", "100",
]
CONTINUATIONS = ["##s", "##ed", "##ing", "##ly", "##er", "##a", "##o"]
CJK = ["你", "好", "家", "火", "爱"]

VOCAB = SPECIALS + WORDS + CONTINUATIONS + CJK
while len(VOCAB) < 64:
    VOCAB.append(f"tok{len(VOCAB)}")
VOCAB_SIZE = len(VOCAB)


def rand_tensor(rng, shape):
    count = 1
    for dim in shape:
        count *= dim
    return [rng.gauss(0.0, 0.08) for _ in range(count)], shape


def write_safetensors(path: Path, tensors: dict) -> None:
    header = {"__metadata__": {"format": "textintel-mini-fixture"}}
    offset = 0
    payload = bytearray()
    for name in sorted(tensors):
        values, shape = tensors[name]
        blob = struct.pack(f"<{len(values)}f", *values)
        header[name] = {
            "dtype": "F32",
            "shape": shape,
            "data_offsets": [offset, offset + len(blob)],
        }
        payload += blob
        offset += len(blob)
    header_bytes = json.dumps(header).encode("utf-8")
    with open(path, "wb") as handle:
        handle.write(struct.pack("<Q", len(header_bytes)))
        handle.write(header_bytes)
        handle.write(payload)


def main() -> None:
    fixtures = Path(__file__).resolve().parent.parent / "tests" / "fixtures"
    root = fixtures / "mini-transformer"
    root.mkdir(parents=True, exist_ok=True)
    rng = random.Random(SEED)

    config = {
        "model_type": "bert",
        "hidden_size": HIDDEN,
        "num_hidden_layers": LAYERS,
        "num_attention_heads": HEADS,
        "intermediate_size": INTERMEDIATE,
        "max_position_embeddings": MAX_POSITIONS,
        "hidden_act": "gelu",
        "layer_norm_eps": 1e-12,
        "vocab_size": VOCAB_SIZE,
        "type_vocab_size": 2,
        "model_id": "textintel-mini-bert-fixture",
        "revision": "fixture-1",
        "languages": ["en", "es", "zh"],
        "do_lower_case": True,
    }
    (root / "config.json").write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")
    (root / "vocab.txt").write_text("\n".join(VOCAB) + "\n", encoding="utf-8")

    tensors = {}
    tensors["embeddings.word_embeddings.weight"] = rand_tensor(rng, [VOCAB_SIZE, HIDDEN])
    tensors["embeddings.position_embeddings.weight"] = rand_tensor(rng, [MAX_POSITIONS, HIDDEN])
    tensors["embeddings.token_type_embeddings.weight"] = rand_tensor(rng, [2, HIDDEN])
    tensors["embeddings.LayerNorm.weight"] = ([1.0] * HIDDEN, [HIDDEN])
    tensors["embeddings.LayerNorm.bias"] = ([0.0] * HIDDEN, [HIDDEN])
    for layer in range(LAYERS):
        base = f"encoder.layer.{layer}"
        for name in ("query", "key", "value"):
            tensors[f"{base}.attention.self.{name}.weight"] = rand_tensor(rng, [HIDDEN, HIDDEN])
            tensors[f"{base}.attention.self.{name}.bias"] = rand_tensor(rng, [HIDDEN])
        tensors[f"{base}.attention.output.dense.weight"] = rand_tensor(rng, [HIDDEN, HIDDEN])
        tensors[f"{base}.attention.output.dense.bias"] = rand_tensor(rng, [HIDDEN])
        tensors[f"{base}.attention.output.LayerNorm.weight"] = ([1.0] * HIDDEN, [HIDDEN])
        tensors[f"{base}.attention.output.LayerNorm.bias"] = ([0.0] * HIDDEN, [HIDDEN])
        tensors[f"{base}.intermediate.dense.weight"] = rand_tensor(rng, [INTERMEDIATE, HIDDEN])
        tensors[f"{base}.intermediate.dense.bias"] = rand_tensor(rng, [INTERMEDIATE])
        tensors[f"{base}.output.dense.weight"] = rand_tensor(rng, [HIDDEN, INTERMEDIATE])
        tensors[f"{base}.output.dense.bias"] = rand_tensor(rng, [HIDDEN])
        tensors[f"{base}.output.LayerNorm.weight"] = ([1.0] * HIDDEN, [HIDDEN])
        tensors[f"{base}.output.LayerNorm.bias"] = ([0.0] * HIDDEN, [HIDDEN])

    write_safetensors(root / "model.safetensors", tensors)
    print(f"wrote {root} ({len(tensors)} tensors, {VOCAB_SIZE} vocab)")
    write_unigram_fixture(fixtures / "mini-unigram")


UNIGRAM_SEED = 20260917
UNIGRAM_HIDDEN = 8
UNIGRAM_LAYERS = 1
UNIGRAM_HEADS = 2
UNIGRAM_INTERMEDIATE = 16
UNIGRAM_POSITIONS = 16
UNIGRAM_VOCAB = [
    ("<s>", 0.0), ("<pad>", 0.0), ("</s>", 0.0), ("<unk>", 0.0),
    ("\u2581", -1.0), ("\u2581hello", 5.0), ("\u2581world", 4.0),
    ("he", 1.0), ("llo", 1.0), ("s", 0.5),
]


def write_unigram_fixture(root: Path) -> None:
    root.mkdir(parents=True, exist_ok=True)
    rng = random.Random(UNIGRAM_SEED)
    hidden, layers = UNIGRAM_HIDDEN, UNIGRAM_LAYERS
    inter, positions = UNIGRAM_INTERMEDIATE, UNIGRAM_POSITIONS
    vocab_size = len(UNIGRAM_VOCAB)

    config = {
        "model_type": "bert",
        "hidden_size": hidden,
        "num_hidden_layers": layers,
        "num_attention_heads": UNIGRAM_HEADS,
        "intermediate_size": inter,
        "max_position_embeddings": positions,
        "hidden_act": "gelu",
        "layer_norm_eps": 1e-12,
        "vocab_size": vocab_size,
        "type_vocab_size": 2,
        "model_id": "textintel-mini-unigram-fixture",
        "revision": "fixture-1",
        "languages": ["en"],
        "do_lower_case": False,
    }
    (root / "config.json").write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")
    tokenizer = {
        "version": "1.0",
        "model": {
            "type": "Unigram",
            "unk_id": 3,
            "byte_fallback": False,
            "vocab": [[piece, score] for piece, score in UNIGRAM_VOCAB],
        },
        "added_tokens": [
            {"id": 0, "content": "<s>", "special": True},
            {"id": 1, "content": "<pad>", "special": True},
            {"id": 2, "content": "</s>", "special": True},
        ],
    }
    (root / "tokenizer.json").write_text(
        json.dumps(tokenizer, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )

    tensors = {}
    prefix = "bert."
    tensors[prefix + "embeddings.word_embeddings.weight"] = rand_tensor(rng, [vocab_size, hidden])
    tensors[prefix + "embeddings.position_embeddings.weight"] = rand_tensor(
        rng, [positions, hidden]
    )
    tensors[prefix + "embeddings.token_type_embeddings.weight"] = rand_tensor(rng, [2, hidden])
    tensors[prefix + "embeddings.LayerNorm.weight"] = ([1.0] * hidden, [hidden])
    tensors[prefix + "embeddings.LayerNorm.bias"] = ([0.0] * hidden, [hidden])
    for layer in range(layers):
        base = f"{prefix}encoder.layer.{layer}"
        for name in ("query", "key", "value"):
            tensors[f"{base}.attention.self.{name}.weight"] = rand_tensor(rng, [hidden, hidden])
            tensors[f"{base}.attention.self.{name}.bias"] = rand_tensor(rng, [hidden])
        tensors[f"{base}.attention.output.dense.weight"] = rand_tensor(rng, [hidden, hidden])
        tensors[f"{base}.attention.output.dense.bias"] = rand_tensor(rng, [hidden])
        tensors[f"{base}.attention.output.LayerNorm.weight"] = ([1.0] * hidden, [hidden])
        tensors[f"{base}.attention.output.LayerNorm.bias"] = ([0.0] * hidden, [hidden])
        tensors[f"{base}.intermediate.dense.weight"] = rand_tensor(rng, [inter, hidden])
        tensors[f"{base}.intermediate.dense.bias"] = rand_tensor(rng, [inter])
        tensors[f"{base}.output.dense.weight"] = rand_tensor(rng, [hidden, inter])
        tensors[f"{base}.output.dense.bias"] = rand_tensor(rng, [hidden])
        tensors[f"{base}.output.LayerNorm.weight"] = ([1.0] * hidden, [hidden])
        tensors[f"{base}.output.LayerNorm.bias"] = ([0.0] * hidden, [hidden])
    write_safetensors(root / "model.safetensors", tensors)
    print(f"wrote {root} ({len(tensors)} tensors, {vocab_size} vocab)")


if __name__ == "__main__":
    main()
