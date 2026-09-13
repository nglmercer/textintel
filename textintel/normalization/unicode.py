from __future__ import annotations

import unicodedata


def nfc(text: str) -> str:
    return unicodedata.normalize("NFC", text)


def nfkc(text: str) -> str:
    return unicodedata.normalize("NFKC", text)


def casefold_text(text: str) -> str:
    return nfc(text).casefold()
