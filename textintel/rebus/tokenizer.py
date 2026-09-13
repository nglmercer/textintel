from __future__ import annotations

import re

from textintel.normalization.leetspeak import LEET_MAP

# Runs of letters, digits, single emoji/symbol.
_PIECE = re.compile(
    r"[A-Za-zÁÉÍÓÚÜÑáéíóúüñ]+|\d+|[\U0001F300-\U0001FAFF]|[^\s]",
    re.UNICODE,
)


def rebus_tokens(text: str) -> list[str]:
    return [m.group(0) for m in _PIECE.finditer(text)]


def token_readings(token: str, max_readings: int) -> list[tuple[str, float, str, str]]:
    """Return (surface, score, lang, transform_type)."""
    from textintel.symbols.knowledge import readings_for_token

    extra = readings_for_token(token, max_readings)
    if extra:
        return [(t, w, lang, "symbol_reading" if not token.isdigit() else "number_reading") for t, w, lang in extra]

    # Leet digits inside otherwise handled as whole token
    if len(token) == 1 and token in LEET_MAP:
        out = []
        for r in LEET_MAP[token][:max_readings]:
            out.append((r, 0.55, "und", "leetspeak"))
        return out

    # Mixed alnum like c0mpr4 — expand leet as a view
    if any(ch.isdigit() for ch in token) and any(ch.isalpha() for ch in token):
        folded = []
        for ch in token:
            if ch in LEET_MAP:
                folded.append(LEET_MAP[ch][0])
            else:
                folded.append(ch)
        folded_s = "".join(folded)
        return [
            (token, 0.4, "und", "identity"),
            (folded_s, 0.75, "und", "leetspeak"),
            (folded_s.casefold(), 0.8, "und", "leetspeak"),
        ]

    return [(token, 1.0, "und", "identity"), (token.casefold(), 0.95, "und", "casefold")]
