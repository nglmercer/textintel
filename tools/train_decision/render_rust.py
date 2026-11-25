#!/usr/bin/env python3
"""Render the trained decision-layer weights as Rust source.

Reads tools/train_decision/weights.json (produced by train.py) and writes
src/comparison/decision.rs. The baked model is a logistic regression over the
explainable feature vector already exposed on `ComparisonResult`; see
tools/train_decision/README.md for the training/verification protocol.
"""
import json
import sys

HEADER = """//! Baked linear decision layer over the explainable comparison feature
//! vector. Trained on the `train` split of `data/evaluation` with the
//! validation split for model selection by `tools/train_decision/train.py`;
//! the test split is only ever used by the held-out verification in
//! `tests/decision_layer.rs`. Regenerate with
//! `python3 tools/train_decision/render_rust.py` after retraining — do not
//! edit the weights by hand.
//!
//! The layer only reprices evidence already present on
//! [`ComparisonResult`](crate::core::types::ComparisonResult): every feature
//! remains visible in `evidence`/`explanations`, so the scored output stays
//! auditable end to end.

use crate::core::types::ComparisonResult;

"""

# feature name -> Rust expression reading it from a ComparisonResult reference
def feature_expr(name):
    direct = {
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
    optional = {
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
    if name in direct:
        return direct[name]
    if name in optional:
        return f"{optional[name]}.unwrap_or(0.0)"
    if name == "semantic_present":
        return "if result.semantic.is_some() { 1.0 } else { 0.0 }"
    if name == "phonetic_present":
        return "if result.phonetic.is_some() { 1.0 } else { 0.0 }"
    if name == "symbolic_present":
        return "if result.symbolic.is_some() { 1.0 } else { 0.0 }"
    if name == "transliteration_weighted":
        return ("result.transliteration_similarity.unwrap_or(0.0)\n"
                "        * result.transliteration_confidence.unwrap_or(0.0)")
    raise KeyError(f"no Rust expression for feature {name}")


def main():
    weights_path = sys.argv[1] if len(sys.argv) > 1 else "tools/train_decision/weights.json"
    out_path = sys.argv[2] if len(sys.argv) > 2 else "src/comparison/decision.rs"
    data = json.load(open(weights_path))
    weights = data["weights"]
    bias = data["bias"]
    names = sorted(weights)
    lines = [HEADER]
    lines.append(f"const BIAS: f64 = {bias!r};\n\n")
    lines.append("/// Logistic-decision evidence score over the comparison feature\n")
    lines.append("/// vector. Pure and total: hostile or non-finite channel values are\n")
    lines.append("/// sanitized exactly as in the weighted scorer.\n")
    lines.append("pub fn decision_logit(result: &ComparisonResult) -> f64 {\n")
    lines.append("    let mut logit = BIAS;\n")
    for name in names:
        weight = weights[name]
        expr = feature_expr(name)
        lines.append(f"    logit += ({expr}) * {weight!r};\n")
    lines.append("    if logit.is_finite() { logit } else { 0.0 }\n")
    lines.append("}\n\n")
    lines.append("/// Sigmoid of [`decision_logit`], clamped into `[0, 1]`. This is the\n")
    lines.append("/// decision-layer similarity score.\n")
    lines.append("pub fn decision_score(result: &ComparisonResult) -> f64 {\n")
    lines.append("    // Temperature compression: trained logits are large, and a plain\n")
    lines.append("    // sigmoid saturates to exact 0.0/1.0 in f64 for clearly separable pairs.\n")
    lines.append("    // Saturated ties erase the fine-grained ordering retrieval depends on\n")
    lines.append("    // (the harness tie-breaks equal scores alphabetically), so the logit is\n")
    lines.append("    // scaled down before the sigmoid. The operating point is untouched:\n")
    lines.append("    // scaling is positive, sign — and therefore the 0.5 decision boundary —\n")
    lines.append("    // is preserved exactly.\n")
    lines.append("    const TEMPERATURE: f64 = 0.2;\n")
    lines.append("    let logit = (decision_logit(result) * TEMPERATURE).clamp(-60.0, 60.0);\n")
    lines.append("    1.0 / (1.0 + (-logit).exp())\n")
    lines.append("}\n\n")
    lines.append("/// Replace `result.score` with the decision-layer score. Channel\n")
    lines.append("/// evidence, explanations and per-field values are untouched; only the\n")
    lines.append("/// aggregate interpretation changes.\n")
    lines.append("pub fn apply_decision_layer(result: &mut ComparisonResult) {\n")
    lines.append("    result.score = decision_score(result);\n")
    lines.append("}\n")
    with open(out_path, "w") as handle:
        handle.write("".join(lines))
    print(f"wrote {out_path} with {len(names)} features, bias {bias:+.4f}")


if __name__ == "__main__":
    main()
