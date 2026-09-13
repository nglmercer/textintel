from __future__ import annotations

# Multi-reading tables: never a single forced meaning.
# (reading, weight, language_hint)

EMOJI_READINGS: dict[str, list[tuple[str, float, str]]] = {
    "🏠": [
        ("casa", 0.93, "es"),
        ("hogar", 0.51, "es"),
        ("vivienda", 0.4, "es"),
        ("house", 0.55, "en"),
        ("home", 0.5, "en"),
    ],
    "🔥": [
        ("fuego", 0.7, "es"),
        ("fire", 0.7, "en"),
        ("hot", 0.45, "en"),
        ("lit", 0.35, "en"),
        ("unknown", 0.2, "und"),
    ],
    "💰": [
        ("dinero", 0.9, "es"),
        ("plata", 0.4, "es"),
        ("money", 0.85, "en"),
        ("cash", 0.4, "en"),
    ],
    "❤️": [("amor", 0.8, "es"), ("love", 0.8, "en"), ("heart", 0.5, "en")],
    "❤": [("amor", 0.8, "es"), ("love", 0.8, "en"), ("heart", 0.5, "en")],
}

NUMBER_READINGS: dict[str, list[tuple[str, float, str]]] = {
    "0": [("cero", 0.8, "es"), ("zero", 0.7, "en"), ("o", 0.5, "und")],
    "1": [("uno", 0.8, "es"), ("one", 0.7, "en"), ("un", 0.5, "es"), ("i", 0.4, "und")],
    "2": [("dos", 0.9, "es"), ("two", 0.7, "en"), ("tu", 0.3, "es")],
    "3": [("tres", 0.85, "es"), ("three", 0.7, "en"), ("e", 0.35, "und")],
    "4": [("cuatro", 0.7, "es"), ("four", 0.6, "en"), ("a", 0.55, "und"), ("for", 0.4, "en")],
    "5": [("cinco", 0.7, "es"), ("five", 0.6, "en"), ("s", 0.4, "und")],
    "6": [("seis", 0.7, "es"), ("six", 0.6, "en")],
    "7": [("siete", 0.7, "es"), ("seven", 0.6, "en"), ("t", 0.3, "und")],
    "8": [("ocho", 0.7, "es"), ("eight", 0.6, "en"), ("ate", 0.35, "en")],
    "9": [("nueve", 0.7, "es"), ("nine", 0.6, "en")],
    "10": [("diez", 0.8, "es"), ("ten", 0.7, "en")],
    "100": [("cien", 0.85, "es"), ("hundred", 0.7, "en"), ("always", 0.2, "en")],
}

# Small multilingual lexicon for lexical plausibility (not decode shortcuts).
WORDLIST: set[str] = {
    "casa", "hogar", "house", "home", "fracasado", "fracasada", "fracasar",
    "saludo", "saludos", "salud", "hola", "hello", "compra", "comprar",
    "ahora", "now", "gana", "ganar", "dinero", "money", "paypal", "cash",
    "siempre", "cien", "pre", "bro", "iphone", "fuego", "fire", "amor",
    "love", "ferrocarril", "cansado", "camino", "camino", "gracias",
    "please", "por", "para", "the", "and", "el", "la", "de", "que",
    "vivienda", "plata", "dos", "uno", "tres", "four", "two", "one",
    "lit", "hot", "unknown", "ok", "lol", "buy", "cash",
}


def readings_for_token(token: str, max_readings: int) -> list[tuple[str, float, str]]:
    if token in EMOJI_READINGS:
        return EMOJI_READINGS[token][:max_readings]
    if token in NUMBER_READINGS:
        return NUMBER_READINGS[token][:max_readings]
    if token.isdigit() and token in NUMBER_READINGS:
        return NUMBER_READINGS[token][:max_readings]
    return []
