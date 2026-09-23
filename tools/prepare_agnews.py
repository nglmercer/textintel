#!/usr/bin/env python3
"""Prepare AG News decision data (stdlib only, deterministic).

Reads data/agnews/{train,test}.csv (mirrored AG News) and writes:
  data/agnews/train.jsonl   6,000 examples (1,500/class, "i % 10 == 0")
  data/agnews/valid.jsonl    600 examples (150/class, "i % 10 == 5" head)
  data/decision/eval-simple.json  60 shared eval examples:
    48 AG News from test.csv (12/class, evenly spread) + 12 routing seeds.

Splits are disjoint by construction. eval-simple.json is the committed
cross-model comparison contract; the JSONL files are gitignored
reproducible intermediates.
"""

import csv
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
AG = ROOT / "data" / "agnews"

LABELS = {
    "1": ("world", "World news: international politics, wars, diplomacy and global events."),
    "2": ("sports", "Sports news: matches, scores, teams, athletes and tournaments."),
    "3": ("business", "Business news: markets, companies, stocks, economy and finance."),
    "4": ("scitech", "Science and technology news: research, space, computers and gadgets."),
}
INSTRUCTIONS = "Which topic is this news article about?"


def criteria():
    return {name: desc for _, (name, desc) in LABELS.items()}


def load_csv(path):
    rows = []
    with open(path, newline="", encoding="utf-8") as handle:
        for record in csv.reader(handle):
            if len(record) != 3:
                continue
            label, title, description = (field.strip() for field in record)
            if label not in LABELS:
                continue
            state = f"{title}. {description}".strip()
            if len(state) > 600:
                state = state[:597] + "..."
            rows.append((label, state))
    return rows


def example(prefix, index, label, state):
    gold, _ = LABELS[label]
    return {
        "id": f"{prefix}_{index:05d}",
        "state": state,
        "question": {
            "type": "choice",
            "instructions": INSTRUCTIONS,
            "criteria": criteria(),
        },
        "gold": gold,
        "task": "ag-news-topic",
    }


def main():
    train_rows = load_csv(AG / "train.csv")
    test_rows = load_csv(AG / "test.csv")
    print(f"train rows: {len(train_rows)}, test rows: {len(test_rows)}", file=sys.stderr)

    # Per-class index tracking (AG files are class-ordered).
    train_examples, valid_examples = [], []
    seen = {}
    for label, state in train_rows:
        position = seen.get(label, 0)
        seen[label] = position + 1
        if position % 10 == 0 and sum(1 for e in train_examples if e["gold"] == LABELS[label][0]) < 1500:
            train_examples.append(example("ag_train", len(train_examples), label, state))
        elif position % 10 == 5 and sum(1 for e in valid_examples if e["gold"] == LABELS[label][0]) < 150:
            valid_examples.append(example("ag_valid", len(valid_examples), label, state))
    with open(AG / "train.jsonl", "w", encoding="utf-8") as handle:
        for item in train_examples:
            handle.write(json.dumps(item, ensure_ascii=False) + "\n")
    with open(AG / "valid.jsonl", "w", encoding="utf-8") as handle:
        for item in valid_examples:
            handle.write(json.dumps(item, ensure_ascii=False) + "\n")

    # Eval: 12 evenly spread per class from test.csv.
    by_label = {}
    for label, state in test_rows:
        by_label.setdefault(label, []).append(state)
    eval_examples = []
    for label in ("1", "2", "3", "4"):
        states = by_label[label]
        step = len(states) / 12
        for slot in range(12):
            state = states[int(slot * step)]
            eval_examples.append(example("ag_eval", len(eval_examples), label, state))

    # Plus the 12 routing test seeds (same objects, shared criteria).
    routing_test = ROOT / "data" / "decision" / "test.jsonl"
    with open(routing_test, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                eval_examples.append(json.loads(line))

    document = {"version": "1.0.0", "examples": eval_examples}
    output = ROOT / "data" / "decision" / "eval-simple.json"
    with open(output, "w", encoding="utf-8") as handle:
        json.dump(document, handle, ensure_ascii=False, indent=1)
    counts = {}
    for item in eval_examples:
        counts[item.get("task", "?")] = counts.get(item.get("task", "?"), 0) + 1
    print(f"train={len(train_examples)} valid={len(valid_examples)} eval={len(eval_examples)} {counts}")


if __name__ == "__main__":
    main()
