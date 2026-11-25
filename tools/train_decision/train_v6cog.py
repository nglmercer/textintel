#!/usr/bin/env python3
"""Consolidated training on features_v3 (with swap_synonym channel)."""
import json
import sys
import numpy as np

sys.path.insert(0, 'tools/train_decision')
from train import load, matrix, sigmoid, binary_metrics, evaluate
from gbm import train_gbm, eval_gbm, export_json

SHORTS = {"bj", "slt", "m8", "b4", "gr8", "hm", "br", "ct", "dg", "rd", "pi", "pn",
          "rt", "cr", "bs", "kl", "xz", "qw", "bn", "mj"}


def augment(r):
    f = r['features']
    f['inexact_decoded_overlap'] = f['decoded_similarity'] * (1.0 - f['exact_decode'])
    f['containment_character'] = f['substring_containment'] * max(f['character'], 0.5)
    f['typo_signal'] = f['character'] * (1.0 - f['lexicon_validity']) * max(f['single_word_pair'], 0.5)
    f['confusable_pressure'] = f['character'] * f['lexicon_validity'] * f['single_word_pair']
    f['semantic_x_char'] = f['semantic'] * f['character']
    f['swap_form'] = f['swapped_word_similarity'] * f['swapped_phonetic']
    f['swap_meaning_valid'] = f['swapped_word_similarity'] * f['lexicon_validity']
    f['char_sq'] = f['character'] ** 2
    f['semantic_sq'] = f['semantic'] ** 2
    f['cognate_x_char'] = f['cognate_match'] * max(f['character'], 0.3)
    f['antonym_x_char'] = max(f['swap_antonym'], f['multi_swap_antonym']) * f['character']
    f['synonym_x_char'] = max(f['swap_synonym'], f['multi_swap_synonym_coverage']) * f['character']


rows = load('tools/train_decision/features_v6.jsonl')
for r in rows:
    augment(r)
names = sorted(n for n in rows[0]['features'] if n != 'weighted_score')
print("features:", len(names))
splits = {}
for r in rows:
    splits.setdefault(r['split'], []).append(r)
# Noise rows are kept even when substring-related: measured on validation,
# the larger set regularizes overconfident containment matches and improves
# cross-language/spam generalization versus filtering.
train_rows = list(splits['train'])
print("train rows:", len(train_rows))
xtr, ytr = matrix(train_rows, names)
test_orig = [r for r in splits['test'] if r['category'] != 'noise']
xto, yto = matrix(test_orig, names)
val_orig = [r for r in splits['validation'] if r['category'] != 'noise']
xvo, yvo = matrix(val_orig, names)

init, trees, lr = train_gbm(xtr, ytr, n_trees=200, learning_rate=0.25, max_depth=3, min_leaf=8)
mv = binary_metrics(sigmoid(eval_gbm(init, trees, lr, xvo)), yvo)
scores = sigmoid(eval_gbm(init, trees, lr, xto))
m = binary_metrics(scores, yto)
print(f"gbm-v6: val roc={mv['roc_auc']:.3f} | TEST roc={m['roc_auc']:.3f} f1={m['f1']:.3f} acc={m['accuracy']:.3f}")
evaluate(test_orig, scores, "test")
json.dump(export_json(init, trees, lr, names), open('tools/train_decision/gbm_model.json', 'w'))

def train_logreg(x, y, epochs=8000, rate=0.5):
    n, d = x.shape
    w = np.zeros(d)
    b = 0.0
    for _ in range(epochs):
        z = x @ w + b
        p = 1 / (1 + np.exp(-np.clip(z, -60, 60)))
        err = (p - y) / n
        w -= rate * (x.T @ err)
        b -= rate * err.sum()
    return w, b

w, b = train_logreg(xtr, ytr)
ml = binary_metrics(sigmoid(xvo @ w + b), yvo)
print(f"logi-v6 val: roc={ml['roc_auc']:.3f} f1={ml['f1']:.3f}")
json.dump({"bias": float(b), "weights": {n: float(wi) for n, wi in zip(names, w)}},
          open('tools/train_decision/weights_noise_sem.json', 'w'), indent=1, sort_keys=True)
print("saved v5 models")
