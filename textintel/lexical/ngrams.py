from __future__ import annotations


def word_ngrams(tokens: list[str], n: int = 2) -> list[str]:
    if n <= 1:
        return list(tokens)
    return [" ".join(tokens[i : i + n]) for i in range(len(tokens) - n + 1)]
