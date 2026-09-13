from textintel.normalization.leetspeak import apply_leet, detect_leet, LEET_MAP
from textintel.normalization.repetition import collapse_repetition
from textintel.normalization.whitespace import normalize_whitespace
from textintel.normalization.unicode import nfc, nfkc, casefold_text

__all__ = [
    "apply_leet",
    "detect_leet",
    "LEET_MAP",
    "collapse_repetition",
    "normalize_whitespace",
    "nfc",
    "nfkc",
    "casefold_text",
]
