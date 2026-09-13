from __future__ import annotations

import unicodedata


def script_name(ch: str) -> str | None:
    if ch.isspace() or unicodedata.category(ch).startswith("P"):
        return None
    if unicodedata.category(ch) in {"Nd", "No"}:
        return "Common"
    try:
        name = unicodedata.name(ch, "")
    except ValueError:
        return None
    # Emoji / symbols
    if "EMOJI" in name or unicodedata.category(ch) in {"So", "Sm", "Sk"}:
        return "Symbol"
    # Unicode script from name prefix
    for prefix in (
        "LATIN",
        "CYRILLIC",
        "GREEK",
        "ARABIC",
        "HEBREW",
        "CJK",
        "HIRAGANA",
        "KATAKANA",
        "HANGUL",
        "DEVANAGARI",
        "THAI",
    ):
        if name.startswith(prefix):
            return prefix.title() if prefix != "CJK" else "Han"
    if name.startswith("CJK"):
        return "Han"
    return "Other"


def scripts_in(text: str) -> list[str]:
    found: set[str] = set()
    for ch in text:
        s = script_name(ch)
        if s and s not in {"Common", "Symbol"}:
            found.add(s)
    return sorted(found)
