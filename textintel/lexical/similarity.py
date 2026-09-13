from __future__ import annotations

from textintel.lexical.tokenizer import simple_lemmas, tokenize


def jaccard(a: set[str], b: set[str]) -> float:
    if not a and not b:
        return 1.0
    if not a or not b:
        return 0.0
    return len(a & b) / len(a | b)


def lexical_similarity(a: str, b: str) -> float:
    ta = set(simple_lemmas(tokenize(a.casefold())))
    tb = set(simple_lemmas(tokenize(b.casefold())))
    return jaccard(ta, tb)
