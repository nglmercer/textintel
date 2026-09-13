from __future__ import annotations

from textintel.core.config import SimilarityWeights
from textintel.core.types import ComparisonResult, MessageFingerprint
from textintel.lexical.character import combined_character_similarity
from textintel.lexical.similarity import lexical_similarity
from textintel.normalization.leetspeak import apply_leet
from textintel.normalization.repetition import collapse_repetition
from textintel.visual.similarity import visual_similarity


def _best_decoded_overlap(fa: MessageFingerprint, fb: MessageFingerprint) -> float:
    texts_a = {fa.raw.casefold(), (fa.normalized or "").casefold()}
    texts_b = {fb.raw.casefold(), (fb.normalized or "").casefold()}
    for c in fa.rebus_candidates:
        texts_a.add(c.text.casefold())
    for c in fb.rebus_candidates:
        texts_b.add(c.text.casefold())
    best = 0.0
    for a in texts_a:
        if not a:
            continue
        for b in texts_b:
            if not b:
                continue
            if a == b:
                return 1.0
            best = max(best, combined_character_similarity(a, b))
    return best


def _obf_sim(fa: MessageFingerprint, fb: MessageFingerprint) -> float:
    # Compare *normalized* forms when obfuscation is present.
    na = collapse_repetition(apply_leet(fa.raw.casefold()))
    nb = collapse_repetition(apply_leet(fb.raw.casefold()))
    return combined_character_similarity(na, nb)


def combine_scores(
    channels: dict[str, float | None],
    weights: SimilarityWeights,
) -> tuple[float, dict[str, float]]:
    w = weights.as_dict()
    used: dict[str, float] = {}
    acc = 0.0
    tw = 0.0
    for name, val in channels.items():
        if val is None:
            continue
        wt = w.get(name, 0.0)
        if wt <= 0:
            continue
        used[name] = wt
        acc += wt * val
        tw += wt
    if tw <= 0:
        return 0.0, used
    return acc / tw, {k: v / tw for k, v in used.items()}


def score_fingerprints(
    fa: MessageFingerprint,
    fb: MessageFingerprint,
    weights: SimilarityWeights,
    *,
    semantic: float | None,
    phonetic: float | None,
) -> ComparisonResult:
    char = combined_character_similarity(fa.raw, fb.raw)
    lex = lexical_similarity(fa.raw, fb.raw)
    vis = visual_similarity(fa.raw, fb.raw)
    dec = _best_decoded_overlap(fa, fb)
    obf = _obf_sim(fa, fb)
    channels = {
        "character": char,
        "lexical": lex,
        "visual": vis,
        "decoded": dec,
        "obfuscation": obf,
        "semantic": semantic,
        "phonetic": phonetic,
    }
    combined, used = combine_scores(channels, weights)
    evidence = [
        f"character={char:.3f}",
        f"lexical={lex:.3f}",
        f"visual={vis:.3f}",
        f"decoded={dec:.3f}",
        f"obfuscation_norm={obf:.3f}",
    ]
    if semantic is None:
        evidence.append("semantic=absent")
    else:
        evidence.append(f"semantic={semantic:.3f}")
    if phonetic is None:
        evidence.append("phonetic=absent")
    else:
        evidence.append(f"phonetic={phonetic:.3f}")
    if fa.obfuscation_features.detected:
        evidence.append(f"obfuscation_flags_a={fa.obfuscation_features.flags}")
    if fb.unicode_features.confusable_characters or fa.unicode_features.confusable_characters:
        evidence.append("confusable_unicode")
    return ComparisonResult(
        score=combined,
        character=char,
        lexical=lex,
        visual=vis,
        decoded_similarity=dec,
        obfuscation=obf,
        semantic=semantic,
        phonetic=phonetic,
        evidence=evidence,
        weights_used=used,
    )
