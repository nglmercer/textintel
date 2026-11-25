#!/usr/bin/env python3
"""Gradient-boosted decision-tree trainer for the comparison decision layer.

Pure numpy implementation of exact-greedy GBM for binary logistic loss.
Small by design: the exported ensemble is a flat array of trees that the
Rust inference side evaluates with simple branch walks — deterministic,
dependency-free, and auditable per feature.
"""
import json
import numpy as np


def sigmoid(z):
    return 1.0 / (1.0 + np.exp(-np.clip(z, -60, 60)))


class Tree:
    __slots__ = ("feature", "threshold", "left", "right", "value")

    def __init__(self):
        self.feature = -1
        self.threshold = 0.0
        self.left = -1
        self.right = -1
        self.value = 0.0


def build_tree(x, grad, hess, idx, depth, max_depth, min_leaf, nodes):
    """Recursively build one regression tree on gradients; returns node id."""
    node_id = len(nodes)
    nodes.append(Tree())
    n = len(idx)
    if n == 0:
        return node_id
    # leaf value: Newton step -sum(g)/sum(h)
    g_sum = grad[idx].sum()
    h_sum = hess[idx].sum()
    leaf_value = -g_sum / max(h_sum, 1e-9)
    nodes[node_id].value = leaf_value
    if depth >= max_depth or n < 2 * min_leaf:
        return node_id
    best = None
    total_g, total_h = g_sum, h_sum
    for feature in range(x.shape[1]):
        values = x[idx, feature]
        order = np.argsort(values, kind="stable")
        sorted_vals = values[order]
        sorted_idx = idx[order]
        left_g = 0.0
        left_h = 0.0
        left_n = 0
        for pos in range(n - 1):
            row = sorted_idx[pos]
            left_g += grad[row]
            left_h += hess[row]
            left_n += 1
            if sorted_vals[pos] == sorted_vals[pos + 1]:
                continue
            if left_n < min_leaf or (n - left_n) < min_leaf:
                continue
            right_g = total_g - left_g
            right_h = total_h - left_h
            gain = (left_g * left_g / max(left_h, 1e-9)
                    + right_g * right_g / max(right_h, 1e-9)
                    - total_g * total_g / max(total_h, 1e-9))
            if best is None or gain > best[0]:
                best = (gain, feature, (sorted_vals[pos] + sorted_vals[pos + 1]) / 2.0,
                        sorted_idx[:pos + 1], sorted_idx[pos + 1:])
    if best is None or best[0] <= 1e-9:
        return node_id
    _, feature, threshold, left_idx, right_idx = best
    nodes[node_id].feature = feature
    nodes[node_id].threshold = float(threshold)
    nodes[node_id].left = build_tree(x, grad, hess, left_idx, depth + 1, max_depth, min_leaf, nodes)
    nodes[node_id].right = build_tree(x, grad, hess, right_idx, depth + 1, max_depth, min_leaf, nodes)
    nodes[node_id].value = 0.0  # internal nodes carry no leaf value
    return node_id


def train_gbm(x, y, n_trees=150, learning_rate=0.2, max_depth=3, min_leaf=16):
    """Greedy exact GBM for logistic loss. Returns (init_logit, trees)."""
    pos = y.sum()
    neg = len(y) - pos
    init = np.log((pos + 0.5) / (neg + 0.5))
    scores = np.full(len(y), init)
    all_trees = []
    for _ in range(n_trees):
        p = sigmoid(scores)
        grad = p - y
        hess = p * (1.0 - p) + 1e-6
        nodes = []
        build_tree(x, grad, hess, np.arange(len(y)), 0, max_depth, min_leaf, nodes)
        # apply tree: update scores
        def eval_node(row, node_id=0):
            node = nodes[node_id]
            while node.feature >= 0:
                if x[row, node.feature] <= node.threshold:
                    node = nodes[node.left]
                else:
                    node = nodes[node.right]
            return node.value
        for row in range(len(y)):
            scores[row] += learning_rate * eval_node(row)
        all_trees.append(nodes)
    return float(init), all_trees, learning_rate


def eval_gbm(init, trees, learning_rate, x):
    scores = np.full(x.shape[0], init)
    for nodes in trees:
        for row in range(x.shape[0]):
            node_id = 0
            node = nodes[node_id]
            while node.feature >= 0:
                node = nodes[node.left] if x[row, node.feature] <= node.threshold else nodes[node.right]
            scores[row] += learning_rate * node.value
    return scores


def export_json(init, trees, learning_rate, feature_names):
    out = {"init": init, "learning_rate": learning_rate, "features": feature_names, "trees": []}
    for nodes in trees:
        flat = []
        for node in nodes:
            flat.append({
                "feature": node.feature,
                "threshold": node.threshold,
                "left": node.left,
                "right": node.right,
                "value": node.value,
            })
        out["trees"].append(flat)
    return out
