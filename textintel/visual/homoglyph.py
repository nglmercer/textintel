from __future__ import annotations

from textintel.normalization.confusables import CONFUSABLE_TO_LATIN, skeleton
from textintel.visual.scripts import script_name


def confusable_hits(text: str) -> list[tuple[int, str, str, str]]:
    """(index, char, script, latin_lookalike) for non-Latin letters that map to Latin."""
    hits = []
    for i, ch in enumerate(text):
        mapped = CONFUSABLE_TO_LATIN.get(ch) or CONFUSABLE_TO_LATIN.get(ch.casefold())
        scr = script_name(ch)
        if mapped and scr not in {None, "Latin", "Common", "Symbol"}:
            hits.append((i, ch, scr or "Other", mapped))
    return hits


def confusable_skeleton(text: str) -> str:
    return skeleton(text)
