from __future__ import annotations

import re
import unicodedata

from textintel.core.types import LanguageCandidate, MessageSegment
from textintel.language.detector import detect_languages

_URL = re.compile(r"https?://\S+")
_EMAIL = re.compile(r"[\w.+-]+@[\w.-]+")
_MENTION = re.compile(r"@\w+")
_HASH = re.compile(r"#\w+")


def _kind(piece: str) -> str:
    if _URL.match(piece):
        return "url"
    if _EMAIL.match(piece):
        return "email"
    if _MENTION.match(piece):
        return "mention"
    if _HASH.match(piece):
        return "hashtag"
    if piece.isdigit():
        return "number"
    o = ord(piece[0]) if piece else 0
    if 0x1F300 <= o <= 0x1FAFF:
        return "emoji"
    cat = unicodedata.category(piece[0]) if piece else ""
    if cat == "So":
        return "symbol"
    if piece.isalpha():
        return "text"
    return "unknown"


def segment_message(text: str, max_segments: int = 256) -> list[MessageSegment]:
    parts: list[MessageSegment] = []
    # Split keeping emoji, numbers, words
    pattern = re.compile(
        r"https?://\S+|[\w.+-]+@[\w.-]+|@\w+|#\w+|\d+|[A-Za-zÁÉÍÓÚÜÑáéíóúüñ]+|[\U0001F300-\U0001FAFF]|[^\s]",
        re.UNICODE,
    )
    for m in pattern.finditer(text):
        piece = m.group(0)
        kind = _kind(piece)
        langs = detect_languages(piece) if kind == "text" else [LanguageCandidate("unknown", 1.0)]
        if kind == "text" and piece.lower() in {"bro", "now", "ok", "lol"}:
            langs = [
                LanguageCandidate("en", 0.7),
                LanguageCandidate("unknown", 0.3),
            ]
        parts.append(
            MessageSegment(
                text=piece,
                start=m.start(),
                end=m.end(),
                language_candidates=langs,
                segment_type=kind,
            )
        )
        if len(parts) >= max_segments:
            break
    return parts
