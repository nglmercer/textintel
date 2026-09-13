from __future__ import annotations

# Digit/symbol → letter substitutions used as *readings*, not forced replacements.
LEET_MAP: dict[str, tuple[str, ...]] = {
    "0": ("o", "0"),
    "1": ("i", "l", "1"),
    "2": ("z", "2"),
    "3": ("e", "3"),
    "4": ("a", "4"),
    "5": ("s", "5"),
    "6": ("g", "6"),
    "7": ("t", "7"),
    "8": ("b", "8"),
    "9": ("g", "9"),
    "@": ("a",),
    "$": ("s",),
    "!": ("i",),
}

_LEET_CHARS = set(LEET_MAP)


def detect_leet(text: str) -> bool:
    """True when digits/symbols sit inside letter neighborhoods (c0mpr4)."""
    if not text:
        return False
    for i, ch in enumerate(text):
        if ch in _LEET_CHARS and ch.isdigit():
            left = text[i - 1] if i else ""
            right = text[i + 1] if i + 1 < len(text) else ""
            if left.isalpha() or right.isalpha():
                return True
    return False


def apply_leet(text: str) -> str:
    """Greedy single-letter leet fold (one view, not canonical)."""
    out = []
    for ch in text:
        readings = LEET_MAP.get(ch)
        if readings:
            out.append(readings[0])
        else:
            out.append(ch)
    return "".join(out)
