# Decision datasets (seed)

Hand-written seed examples for the `support-routing` choice task. These
establish the format and give the adapters something to score; they are
**not** a production benchmark. Real training data (hard negatives,
obfuscation pairs, multilingual coverage) arrives in Phase 2.

Layout:

- `dataset.json` — dataset version.
- `train.jsonl` — training examples (future use).
- `validation.jsonl` — calibration + threshold fitting. Never final test.
- `test.jsonl` — held-out evaluation.

Evaluate with:

```bash
cargo run --bin textintel -- eval-decision data/decision --split test
```
