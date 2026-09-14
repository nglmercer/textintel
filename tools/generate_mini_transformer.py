#!/usr/bin/env python3
"""Generate the tiny deterministic BERT fixture for transformer mechanics tests.

Writes tests/fixtures/mini-transformer/{config.json,vocab.txt,model.safetensors}
with fixed-seed pseudo-random F32 weights. No third-party packages required:
the safetensors container is assembled with struct+json only.

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


def main() -> None:
    root = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "mini-transformer"
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
    with open(root / "model.safetensors", "wb") as handle:
        handle.write(struct.pack("<Q", len(header_bytes)))
        handle.write(header_bytes)
        handle.write(payload)
    print(f"wrote {root} ({len(tensors)} tensors, {VOCAB_SIZE} vocab)")


if __name__ == "__main__":
    main()
