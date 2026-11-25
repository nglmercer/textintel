#!/usr/bin/env python3
"""Offline retrieval-ranking simulation over extracted ranking features.

Replicates the evaluation harness ranking probe: for each query, rank corpus
documents by descending score and measure recall@1/5/10 + MRR of the first
relevant document. Lets training iterations sweep ensemble blends without
rebuilding Rust.
"""
import json
import sys
import numpy as np

sys.path.insert(0, 'tools/train_decision')
from train import load, matrix, sigmoid
from gbm import eval_gbm


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


def load_gbm(path):
    gm = json.load(open(path))

    class N:
        def __init__(s, d):
            s.feature, s.threshold, s.left, s.right, s.value = (
                d['feature'], d['threshold'], d['left'], d['right'], d['value'])
    return gm, [[N(nd) for nd in t] for t in gm['trees']]


def main():
    rows = load('tools/train_decision/ranking_features.jsonl')
    for r in rows:
        augment(r)
    gbm_path = sys.argv[1] if len(sys.argv) > 1 else 'tools/train_decision/gbm_model.json'
    logi_path = sys.argv[2] if len(sys.argv) > 2 else 'tools/train_decision/weights_noise_sem.json'
    names = sorted(n for n in rows[0]['features'] if n != 'weighted_score')
    gm, trees = load_gbm(gbm_path)
    lm = json.load(open(logi_path))
    lnames = sorted(lm['weights'])
    x, _ = matrix(rows, names)
    xl, _ = matrix(rows, lnames)
    gbm_s = sigmoid(eval_gbm(gm['init'], trees, gm['learning_rate'], x))
    lw = np.array([lm['weights'][n] for n in lnames])
    logi_s = sigmoid(xl @ lw + lm['bias'])
    # group row indices by query
    queries = {}
    for i, r in enumerate(rows):
        queries.setdefault(r['a'], []).append(i)
    print(f"queries={len(queries)} rows={len(rows)}")
    for alpha in [float(a) for a in sys.argv[3:]] or [0.0, 0.3, 0.5, 0.7, 1.0]:
        s = alpha * gbm_s + (1 - alpha) * logi_s
        r1 = r5 = r10 = 0
        rr = 0.0
        for q, idxs in queries.items():
            order = sorted(idxs, key=lambda i: -s[i])
            first_rel = None
            for rank, i in enumerate(order, 1):
                if rows[i]['label']:
                    first_rel = rank
                    break
            if first_rel is None:
                continue
            r1 += first_rel <= 1
            r5 += first_rel <= 5
            r10 += first_rel <= 10
            rr += 1.0 / first_rel
        n = len(queries)
        print(f"alpha={alpha}: recall@1={r1/n:.3f} recall@5={r5/n:.3f} recall@10={r10/n:.3f} mrr={rr/n:.3f}")


main()
