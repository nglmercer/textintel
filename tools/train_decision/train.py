#!/usr/bin/env python3
"""Train the linear decision layer for textintel comparison scoring.

Fits logistic regression over the hand-engineered, explainable feature vector
dumped by `examples/eval_features.rs`. Train split fits weights; validation
split selects the regularization strength; the test split is never used for
fitting or selection — it only receives the final baked-in verification in
the Rust integration tests.
"""
import json
import sys
import numpy as np
from collections import defaultdict


def load(path):
    rows = []
    with open(path) as handle:
        for line in handle:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def feature_names(rows):
    names = sorted(rows[0]["features"].keys())
    return [n for n in names if n != "weighted_score"]


def matrix(rows, names):
    x = np.array([[r["features"][n] for n in names] for r in rows], dtype=np.float64)
    y = np.array([1.0 if r["label"] else 0.0 for r in rows], dtype=np.float64)
    return x, y


def sigmoid(z):
    return 1.0 / (1.0 + np.exp(-np.clip(z, -60, 60)))


def train_logreg(x, y, l2, epochs=4000, lr=0.5):
    n, d = x.shape
    w = np.zeros(d)
    b = 0.0
    for _ in range(epochs):
        z = x @ w + b
        p = sigmoid(z)
        error = p - y
        grad_w = (x.T @ error) / n + l2 * w
        grad_b = error.mean()
        w -= lr * grad_w
        b -= lr * grad_b
    return w, b


def binary_metrics(scores, labels, threshold=0.5):
    scores = np.asarray(scores)
    labels = np.asarray(labels).astype(bool)
    pos = labels.sum()
    neg = len(labels) - pos
    if pos == 0 or neg == 0:
        roc = 0.0
    else:
        order = np.argsort(-scores, kind="stable")
        # concordant pairs
        sp = scores[labels]
        sn = scores[~labels]
        greater = (sp[:, None] > sn[None, :]).sum()
        ties = (sp[:, None] == sn[None, :]).sum()
        roc = (greater + 0.5 * ties) / (pos * neg)
    # pr auc (same stepwise construction as the Rust evaluator)
    order = np.argsort(-scores, kind="stable")
    tp = fp = 0
    pr_auc = 0.0
    prev_recall = 0.0
    for idx in order:
        if labels[idx]:
            tp += 1
        else:
            fp += 1
        recall = tp / pos if pos else 0.0
        precision = tp / (tp + fp) if tp + fp else 0.0
        pr_auc += precision * max(0.0, recall - prev_recall)
        prev_recall = recall
    pred = scores >= threshold
    tp = int((pred & labels).sum())
    fp = int((pred & ~labels).sum())
    fn = int((~pred & labels).sum())
    tn = int((~pred & ~labels).sum())
    f1 = 2 * tp / (2 * tp + fp + fn) if (2 * tp + fp + fn) else 0.0
    acc = (tp + tn) / len(labels) if len(labels) else 0.0
    return {"roc_auc": roc, "pr_auc": pr_auc, "f1": f1, "accuracy": acc,
            "tp": tp, "fp": fp, "fn": fn, "tn": tn}


def evaluate(rows, scores, label=""):
    labels = np.array([r["label"] for r in rows])
    overall = binary_metrics(scores, labels)
    print(f"{label}: roc={overall['roc_auc']:.3f} pr={overall['pr_auc']:.3f} "
          f"f1={overall['f1']:.3f} acc={overall['accuracy']:.3f}")
    cats = defaultdict(list)
    for i, r in enumerate(rows):
        cats[r["category"]].append(i)
    worst = []
    for cat, idxs in sorted(cats.items()):
        sub_scores = scores[idxs]
        sub_labels = labels[idxs]
        m = binary_metrics(sub_scores, sub_labels)
        print(f"  {cat}: n={len(idxs)} f1={m['f1']:.3f} acc={m['accuracy']:.3f} "
              f"roc={m['roc_auc']:.3f} pr={m['pr_auc']:.3f}")
        worst.append((cat, m))
    return overall, worst


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "tools/train_decision/features_all.jsonl"
    rows = load(path)
    names = feature_names(rows)
    splits = defaultdict(list)
    for r in rows:
        splits[r["split"]].append(r)
    print(f"features: {len(names)}; splits: {{k: len(v) for k, v in splits.items()}}")
    train_rows = splits.get("train", [])
    val_rows = splits.get("validation", [])
    test_rows = splits.get("test", [])
    xtr, ytr = matrix(train_rows, names)

    best = None
    for l2 in [0.0, 0.001, 0.01, 0.1, 1.0]:
        w, b = train_logreg(xtr, ytr, l2)
        if val_rows:
            xv, yv = matrix(val_rows, names)
            scores = sigmoid(xv @ w + b)
            m = binary_metrics(scores, yv)
            key = (m["roc_auc"], m["f1"])
            print(f"l2={l2}: validation roc={m['roc_auc']:.3f} f1={m['f1']:.3f} acc={m['accuracy']:.3f}")
            if best is None or key > best[0]:
                best = (key, l2, w, b)
        else:
            best = ((0, 0), l2, w, b)
    _, l2, w, b = best
    print(f"selected l2={l2}")

    print("\n-- validation breakdown --")
    if val_rows:
        xv, yv = matrix(val_rows, names)
        evaluate(val_rows, sigmoid(xv @ w + b), "validation")
    print("\n-- train breakdown --")
    evaluate(train_rows, sigmoid(xtr @ w + b), "train")

    print("\nweights (feature: weight):")
    for n, wi in sorted(zip(names, w), key=lambda t: -abs(t[1])):
        print(f"  {n}: {wi:+.4f}")
    print(f"  BIAS: {b:+.4f}")

    out = {"bias": float(b), "weights": {n: float(wi) for n, wi in zip(names, w)}}
    with open("tools/train_decision/weights.json", "w") as handle:
        json.dump(out, handle, indent=2, sort_keys=True)
    print("\nwrote tools/train_decision/weights.json")


if __name__ == "__main__":
    main()
