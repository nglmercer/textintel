from __future__ import annotations

from textintel.lexical.character import combined_character_similarity
from textintel.normalization.repetition import collapse_repetition
from textintel.symbols.knowledge import WORDLIST


def lexical_plausibility(text: str) -> float:
    t = collapse_repetition(text.casefold()).replace(" ", "")
    if not t:
        return 0.0
    if t in WORDLIST:
        return 1.0
    # substring / word-split hits
    hits = 0
    parts = text.casefold().split()
    for p in parts:
        p = collapse_repetition(p)
        if p in WORDLIST:
            hits += 1
    if parts:
        return 0.4 + 0.6 * (hits / len(parts))
    # prefix of known words
    for w in WORDLIST:
        if w.startswith(t) or t.startswith(w):
            return 0.55
    return 0.15


def score_candidate(surface: str, prior: float) -> tuple[float, float, float, float]:
    lex = lexical_plausibility(surface)
    # lightweight phonetic stand-in: character similarity to collapsed form
    collapsed = collapse_repetition(surface.casefold())
    phon = combined_character_similarity(surface.casefold(), collapsed)
    ctx = min(1.0, 0.5 + 0.5 * lex)
    sym = min(1.0, prior)
    total = 0.45 * lex + 0.2 * phon + 0.15 * ctx + 0.2 * sym
    return total, lex, phon, ctx
