from __future__ import annotations

import re

from textintel.core.types import ObfuscationFeatures, UnicodeFeatures
from textintel.normalization.leetspeak import detect_leet
from textintel.normalization.repetition import repetition_ratio

_PUNCT_FLOOD = re.compile(r"[!?.]{3,}")


def obfuscation_features(text: str, unicode_features: UnicodeFeatures) -> ObfuscationFeatures:
    leet = detect_leet(text)
    rep = repetition_ratio(text) >= 0.08 or bool(re.search(r"(.)\1{3,}", text))
    flood = bool(_PUNCT_FLOOD.search(text))
    mixed = unicode_features.mixed_scripts
    conf = bool(unicode_features.confusable_characters)
    flags: list[str] = []
    if leet:
        flags.append("leetspeak")
    if rep:
        flags.append("repetition")
    if flood:
        flags.append("punctuation_flood")
    if mixed:
        flags.append("mixed_scripts")
    if conf:
        flags.append("confusables")
    score = 0.0
    if leet:
        score += 0.35
    if rep:
        score += 0.25
    if flood:
        score += 0.15
    if mixed:
        score += 0.2
    if conf:
        score += 0.25
    score = min(1.0, score)
    return ObfuscationFeatures(
        detected=score >= 0.25 or bool(flags),
        score=score,
        leetspeak=leet,
        repetition=rep,
        punctuation_flood=flood,
        mixed_scripts=mixed,
        confusables=conf,
        flags=flags,
    )
