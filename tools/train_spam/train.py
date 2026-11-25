"""Train a logistic calibration layer for the heuristic spam detector.

Uses the same deterministic NumPy logistic trainer as the decision layer.
Protocol: fit on v2-train.json rows only; report on the held-out v2-eval rows.
"""
import json, sys
import numpy as np

def load(path):
    rows = []
    for line in open(path):
        rows.append(json.loads(line))
    return rows

def sigmoid(z):
    return 1.0 / (1.0 + np.exp(-z))

def metrics(scores, labels, thr=0.5):
    pred = (scores >= thr).astype(float)
    tp = float(((pred == 1) & (labels == 1)).sum())
    fp = float(((pred == 1) & (labels == 0)).sum())
    fn = float(((pred == 0) & (labels == 1)).sum())
    acc = float((pred == labels).mean())
    prec = tp / max(tp + fp, 1e-9)
    rec = tp / max(tp + fn, 1e-9)
    f1 = 2 * prec * rec / max(prec + rec, 1e-9)
    order = np.argsort(-scores)
    ranked = labels[order]
    pos = float(labels.sum()); neg = float(len(labels) - pos)
    tps = np.cumsum(ranked); fps = np.cumsum(1 - ranked)
    tpr = tps / max(pos, 1e-9); fpr = fps / max(neg, 1e-9)
    roc = float(np.trapezoid(tpr, fpr))
    brier = float(((scores - labels) ** 2).mean())
    return {"accuracy": acc, "precision": prec, "recall": rec, "f1": f1,
            "roc_auc": roc, "brier": brier}

def fit_logistic(x, y, l2=1.0, epochs=4000, lr=0.5):
    mean = x.mean(axis=0); std = x.std(axis=0) + 1e-9
    xs = (x - mean) / std
    w = np.zeros(x.shape[1]); b = 0.0
    n = len(y)
    for _ in range(epochs):
        p = sigmoid(xs @ w + b)
        gw = xs.T @ (p - y) / n + l2 * w / n
        gb = float((p - y).mean())
        w -= lr * gw; b -= lr * gb
    # fold standardization into the raw-space weights
    raw_w = w / std
    raw_b = b - float((mean / std) @ w)
    return raw_w, raw_b

def main():
    rows = load(sys.argv[1])
    names = sorted(rows[0]["features"].keys())
    train = [r for r in rows if r["split"] == "train"]
    evalr = [r for r in rows if r["split"] == "eval"]
    def xy(rs):
        x = np.array([[r["features"][n] for n in names] for r in rs], dtype=np.float64)
        y = np.array([r["label"] for r in rs], dtype=np.float64)
        return x, y
    xt, yt = xy(train); xe, ye = xy(evalr)
    w, b = fit_logistic(xt, yt)
    for label, x, y in [("train", xt, yt), ("eval", xe, ye)]:
        s = sigmoid(x @ w + b)
        m = metrics(s, y)
        print(f"{label}: " + " ".join(f"{k}={v:.3f}" for k, v in m.items()))
    model = {"bias": b, "weights": {n: float(wi) for n, wi in zip(names, w)}}
    json.dump(model, open(sys.argv[2], "w"), indent=1)
    print(f"wrote {sys.argv[2]}: {len(names)} features")

main()
