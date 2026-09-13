from __future__ import annotations

from textintel.lexical.character import combined_character_similarity
from textintel.visual.homoglyph import confusable_skeleton


def visual_similarity(a: str, b: str) -> float:
    sa = confusable_skeleton(a).casefold()
    sb = confusable_skeleton(b).casefold()
    return combined_character_similarity(sa, sb)
