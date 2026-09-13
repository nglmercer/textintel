from __future__ import annotations

import re

_WS = re.compile(r"[\s\u00a0\u2000-\u200b\u202f\u205f\u3000]+")


def normalize_whitespace(text: str) -> str:
    return _WS.sub(" ", text).strip()
