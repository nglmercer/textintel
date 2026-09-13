from __future__ import annotations

import re
import unicodedata

_TOKEN = re.compile(
    r"https?://\S+|[\w]+(?:'[\w]+)?|[\U0001F300-\U0001FAFF]|[^\s\w]",
    re.UNICODE,
)


def tokenize(text: str) -> list[str]:
    return [m.group(0) for m in _TOKEN.finditer(text) if m.group(0).strip()]


def simple_lemmas(tokens: list[str]) -> list[str]:
    """Language-neutral light stem: casefold + strip common suffixes if long."""
    out = []
    suffixes = ("ing", "ed", "es", "s", "mente", "cion", "ción", "ando", "iendo")
    for t in tokens:
        x = t.casefold()
        if x.isalpha() and len(x) > 5:
            for s in suffixes:
                if x.endswith(s) and len(x) - len(s) >= 3:
                    x = x[: -len(s)]
                    break
        out.append(x)
    return out


def is_emoji(ch: str) -> bool:
    if not ch:
        return False
    o = ord(ch[0])
    if 0x1F300 <= o <= 0x1FAFF:
        return True
    name = unicodedata.name(ch[0], "")
    return "EMOJI" in name or "FACE" in name
