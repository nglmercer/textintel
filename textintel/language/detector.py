from __future__ import annotations

from collections import Counter

from textintel.core.types import LanguageCandidate

# Lightweight function-word / letter cues. Probabilistic, never a single forced language.
_ES = {
    "el", "la", "los", "las", "de", "que", "y", "en", "un", "una", "es", "por",
    "con", "para", "compra", "ahora", "gana", "dinero", "saludos", "casa",
    "fracasado", "hola", "gracias", "si", "sí",
}
_EN = {
    "the", "and", "of", "to", "a", "in", "is", "you", "that", "it", "for",
    "on", "with", "bro", "now", "buy", "money", "house", "home", "hi",
}
_PT = {"o", "a", "os", "as", "de", "que", "e", "do", "da", "para", "com", "não"}

_CHAR_ES = set("áéíóúñü¿¡")
_CHAR_PT = set("ãõçáéíóú")


def detect_languages(text: str) -> list[LanguageCandidate]:
    tokens = [t.casefold() for t in text.split() if t]
    scores: Counter[str] = Counter()
    for t in tokens:
        if t in _ES:
            scores["es"] += 2
        if t in _EN:
            scores["en"] += 2
        if t in _PT:
            scores["pt"] += 1
    for ch in text.casefold():
        if ch in _CHAR_ES:
            scores["es"] += 1
        if ch in _CHAR_PT:
            scores["pt"] += 1
    latin = sum(1 for ch in text if "LATIN" in __import__("unicodedata").name(ch, ""))
    if latin and not scores:
        scores["unknown"] += 1
        scores["es"] += 0.3
        scores["en"] += 0.3
    if not scores:
        return [LanguageCandidate("unknown", 1.0)]
    total = sum(scores.values())
    items = [
        LanguageCandidate(lang, round(c / total, 4))
        for lang, c in scores.most_common()
    ]
    unknown_mass = max(0.05, 1.0 - sum(i.probability for i in items))
    if unknown_mass > 0.04 and all(i.language != "unknown" for i in items):
        items.append(LanguageCandidate("unknown", round(unknown_mass, 4)))
    s = sum(i.probability for i in items) or 1.0
    return [LanguageCandidate(i.language, i.probability / s) for i in items]
