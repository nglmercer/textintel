#!/usr/bin/env python3
"""Render the trained GBM ensemble as Rust inference code."""
import json
import sys

BASE_EXPR = {
    "lexicon_validity": "result.lexicon_validity",
    "single_word_pair": "result.single_word_pair",
    "swapped_word_similarity": "result.swapped_word_similarity",
    "swapped_phonetic": "result.swapped_phonetic",
    "confusable_swap": "result.confusable_swap",
    "valid_swap_similarity": "result.valid_swap_similarity",
    "normalized_identity": "result.normalized_identity",
    "substring_containment": "result.substring_containment",
    "cross_script_pair": "result.cross_script_pair",
    "cross_script_agreement": "result.cross_script_agreement",
    "exact_decode": "result.exact_decode",
    "single_word_exact": "result.single_word_exact",
    "entity_agreement": "result.entity_agreement",
    "entity_conflict": "result.entity_conflict",
    "transliteration_compatibility": "result.transliteration_compatibility",
    "contextual_semantic": "result.contextual_semantic",
    "cross_language_semantic": "result.cross_language_semantic",
    "semantic_without_lexical_overlap": "result.semantic_without_lexical_overlap",
    "transliteration_semantic_agreement": "result.transliteration_semantic_agreement",
    "transliteration_semantic_conflict": "result.transliteration_semantic_conflict",
}
OPT = {
    "semantic": "result.semantic",
    "lexical": "result.lexical",
    "character": "result.character",
    "visual": "result.visual",
    "phonetic": "result.phonetic",
    "symbolic": "result.symbolic",
    "decoded_similarity": "result.decoded_similarity",
    "transliteration_similarity": "result.transliteration_similarity",
    "transliteration_confidence": "result.transliteration_confidence",
    "obfuscation_similarity": "result.obfuscation_similarity",
}

def feature_block(names):
    """Emit Rust computing each feature into `f[<index>]` once."""
    lines = []
    for i, n in enumerate(names):
        if n in BASE_EXPR:
            lines.append(f"    f[{i}] = {BASE_EXPR[n]};")
        elif n in OPT:
            lines.append(f"    f[{i}] = {OPT[n]}.unwrap_or(0.0);")
        elif n == "semantic_present":
            lines.append(f"    f[{i}] = if result.semantic.is_some() {{ 1.0 }} else {{ 0.0 }};")
        elif n == "phonetic_present":
            lines.append(f"    f[{i}] = if result.phonetic.is_some() {{ 1.0 }} else {{ 0.0 }};")
        elif n == "symbolic_present":
            lines.append(f"    f[{i}] = if result.symbolic.is_some() {{ 1.0 }} else {{ 0.0 }};")
        elif n == "transliteration_weighted":
            lines.append(f"    f[{i}] = result.transliteration_similarity.unwrap_or(0.0) * result.transliteration_confidence.unwrap_or(0.0);")
        elif n == "inexact_decoded_overlap":
            lines.append(f"    f[{i}] = result.decoded_similarity.unwrap_or(0.0) * (1.0 - result.exact_decode);")
        elif n == "containment_character":
            lines.append(f"    f[{i}] = result.substring_containment * result.character.unwrap_or(0.0).max(0.5);")
        elif n == "typo_signal":
            lines.append(f"    f[{i}] = result.character.unwrap_or(0.0) * (1.0 - result.lexicon_validity) * result.single_word_pair.max(0.5);")
        elif n == "confusable_pressure":
            lines.append(f"    f[{i}] = result.character.unwrap_or(0.0) * result.lexicon_validity * result.single_word_pair;")
        elif n == "semantic_x_char":
            lines.append(f"    f[{i}] = result.semantic.unwrap_or(0.0) * result.character.unwrap_or(0.0);")
        elif n == "swap_form":
            lines.append(f"    f[{i}] = result.swapped_word_similarity * result.swapped_phonetic;")
        elif n == "swap_meaning_valid":
            lines.append(f"    f[{i}] = result.swapped_word_similarity * result.lexicon_validity;")
        elif n == "char_sq":
            lines.append("    { let c = result.character.unwrap_or(0.0); f[%d] = c * c; }" % i)
        elif n == "semantic_sq":
            lines.append("    { let s = result.semantic.unwrap_or(0.0); f[%d] = s * s; }" % i)
        else:
            raise KeyError(n)
    return lines

def main():
    model = json.load(open(sys.argv[1] if len(sys.argv) > 1 else "tools/train_decision/gbm_model.json"))
    out_path = sys.argv[2] if len(sys.argv) > 2 else "src/comparison/decision.rs"
    names = model["features"]
    nfeat = len(names)
    init = model["init"]
    lr = model["learning_rate"]
    idx = {n: i for i, n in enumerate(names)}

    out = []
    out.append("""//! Baked gradient-boosted decision layer over the explainable comparison
//! feature vector. Trained on the `train` split of `data/evaluation` with the
//! validation split for model selection by `tools/train_decision/gbm.py`;
//! the test split is only ever used by held-out verification. Regenerate with
//! `python3 tools/train_decision/render_gbm_rust.py` after retraining — do not
//! edit the ensemble by hand.
//!
//! The layer only reprices evidence already present on
//! [`ComparisonResult`](crate::core::types::ComparisonResult): every input
//! feature remains visible in `evidence`/`explanations`, so scored output
//! stays auditable end to end. Inference is a deterministic branch walk over
//! the baked trees: no allocation, no floating-point platform dependence
//! beyond ordered `f64` comparisons, and no external state.

use crate::core::types::ComparisonResult;

""")
    out.append(f"const INIT: f64 = {init!r};\n")
    out.append(f"const LEARNING_RATE: f64 = {lr!r};\n")
    out.append(f"pub(crate) const FEATURE_COUNT: usize = {nfeat};\n\n")
    out.append("/// Feature values extracted once per comparison, in the fixed baked\n")
    out.append("/// order the ensemble was trained with.\n")
    out.append(f"fn feature_vector(result: &ComparisonResult) -> [f64; {nfeat}] {{\n")
    out.append(f"    let mut f = [0.0f64; {nfeat}];\n")
    out.append("    let semantic = result.semantic.unwrap_or(0.0);\n")
    out.append("    let character = result.character.unwrap_or(0.0);\n")
    for line in feature_block(names):
        # normalize to use locals where emitted by hand
        out.append(line + "\n")
    out.append("    for value in &mut f {\n")
    out.append("        if !value.is_finite() {\n")
    out.append("            *value = 0.0;\n")
    out.append("        }\n")
    out.append("    }\n")
    out.append("    f\n}\n\n")

    # emit trees as static arrays: [feature, threshold, left, right, value] flattened
    out.append("struct Node {\n    feature: i32,\n    threshold: f64,\n    left: i32,\n    right: i32,\n    value: f64,\n}\n\n")
    for t, nodes in enumerate(model["trees"]):
        out.append(f"static TREE_{t}: &[Node] = &[\n")
        for node in nodes:
            out.append("    Node { feature: %d, threshold: %r, left: %d, right: %d, value: %r },\n" % (
                node["feature"], node["threshold"], node["left"], node["right"], node["value"]))
        out.append("];\n")
    out.append("\nstatic TREES: &[&[Node]] = &[\n")
    for t in range(len(model["trees"])):
        out.append(f"    TREE_{t},\n")
    out.append("];\n\n")

    # logistic component baked beside the trees
    import os
    lw_path = os.path.join(os.path.dirname(sys.argv[1] if len(sys.argv) > 1 else "tools/train_decision/gbm_model.json"), "weights_noise_sem.json")
    lm = json.load(open(lw_path))
    lnames = sorted(lm["weights"])
    out.append("/// Logistic-layer weights over the same feature vector, baked from\n")
    out.append("/// `tools/train_decision/weights_noise_sem.json` (indices match `feature_vector`).\n")
    out.append("static LOGISTIC_WEIGHTS: &[f64] = &[\n")
    feature_idx = {n: i for i, n in enumerate(names)}
    for n in lnames:
        if n not in feature_idx:
            raise SystemExit(f"logistic feature missing from gbm feature vector: {n}")
        out.append("    /* %-40s */ %r,\n" % (n, lm["weights"][n]))
    out.append("];\n")
    out.append(f"const LOGISTIC_BIAS: f64 = {lm['bias']!r};\n")
    order_comment = "// logistic weight order: " + ",".join(lnames) + "\n"
    out.append(order_comment)
    out.append("fn logistic_decision_score(result: &ComparisonResult) -> f64 {\n")
    out.append("    let features = feature_vector(result);\n")
    out.append("    let mut logit = LOGISTIC_BIAS;\n")
    out.append("    for (weight, value) in LOGISTIC_WEIGHTS.iter().zip(logistic_order_indices().iter()) {\n")
    out.append("        logit += weight * features[*value];\n")
    out.append("    }\n")
    out.append("    let logit = logit.clamp(-60.0, 60.0);\n")
    out.append("    1.0 / (1.0 + (-logit).exp())\n}\n\n")
    out.append("fn logistic_order_indices() -> &'static [usize] {\n")
    out.append("    &[")
    out.append(",".join(str(feature_idx[n]) for n in lnames))
    out.append("]\n}\n\n")
    out.append("""/// Ensemble log-odds score over the comparison feature vector.
pub fn decision_logit(result: &ComparisonResult) -> f64 {
    let features = feature_vector(result);
    let mut score = INIT;
    for tree in TREES {
        let mut node = &tree[0];
        while node.feature >= 0 {
            node = if features[node.feature as usize] <= node.threshold {
                &tree[node.left as usize]
            } else {
                &tree[node.right as usize]
            };
        }
        score += LEARNING_RATE * node.value;
    }
    if score.is_finite() { score } else { 0.0 }
}

/// Sigmoid of [`decision_logit`]: the decision-layer similarity score.
pub fn decision_score(result: &ComparisonResult) -> f64 {
    let logit = decision_logit(result).clamp(-60.0, 60.0);
    1.0 / (1.0 + (-logit).exp())
}

/// Replace `result.score` with the ensemble decision score: 0.7 x gradient-
/// boosted trees + 0.4 x logistic layer. The logistic component keeps scores
/// finely graduated for retrieval ranking (the trees are piecewise-constant),
/// while the trees capture the nonlinear feature interactions. Channel
/// evidence, explanations and per-field values are untouched; only the
/// aggregate interpretation changes.
pub fn apply_decision_layer(result: &mut ComparisonResult) {
    let tree_score = decision_score(result);
    let logistic_score = logistic_decision_score(result);
    result.score = 0.6 * tree_score + 0.4 * logistic_score;
}
""")
    open(out_path, "w").write("".join(out))
    print(f"wrote {out_path}: {len(model['trees'])} trees, {nfeat} features")

main()
