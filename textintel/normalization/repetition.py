from __future__ import annotations

import re

_REPEAT = re.compile(r"(.)\1{2,}", re.DOTALL)


def collapse_repetition(text: str, keep: int = 1) -> str:
    """Collapse runs of 3+ identical characters (obfuscation flooding)."""

    def _sub(m: re.Match[str]) -> str:
        ch = m.group(1)
        if ch.isalnum() or ch in "!?.":
            return ch * keep
        return m.group(0)

    return _REPEAT.sub(_sub, text)


def repetition_ratio(text: str) -> float:
    if not text:
        return 0.0
    collapsed = collapse_repetition(text)
    saved = len(text) - len(collapsed)
    return min(1.0, saved / max(1, len(text)))
