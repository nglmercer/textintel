from __future__ import annotations

import unicodedata

from textintel.core.types import ConfusableCharacter, UnicodeFeatures
from textintel.normalization.unicode import casefold_text, nfc, nfkc
from textintel.visual.homoglyph import confusable_hits, confusable_skeleton
from textintel.visual.scripts import scripts_in


_INVISIBLE_CATS = {"Cf", "Cc", "Zl", "Zp"}
_INVISIBLE_CHARS = {
    "\u200b",
    "\u200c",
    "\u200d",
    "\u2060",
    "\ufeff",
    "\u00ad",
}


def analyze_unicode(text: str) -> UnicodeFeatures:
    scripts = scripts_in(text)
    mixed = len(scripts) > 1
    invisible: list[str] = []
    for ch in text:
        if ch in _INVISIBLE_CHARS or unicodedata.category(ch) in _INVISIBLE_CATS:
            if ch not in ("\n", "\r", "\t"):
                invisible.append(ch)

    hits = confusable_hits(text)
    conf_chars = [
        ConfusableCharacter(char=ch, index=i, script=scr, confusable_with=lat)
        for i, ch, scr, lat in hits
    ]
    suspicious = 0.0
    if mixed:
        suspicious += 0.4
    if hits:
        suspicious += min(0.5, 0.15 * len(hits))
    if invisible:
        suspicious += min(0.3, 0.1 * len(invisible))
    suspicious = min(1.0, suspicious)

    return UnicodeFeatures(
        scripts=scripts,
        mixed_scripts=mixed,
        invisible_characters=invisible,
        confusable_characters=conf_chars,
        confusable_skeleton=confusable_skeleton(text),
        suspicious_unicode_score=suspicious,
        nfc=nfc(text),
        nfkc=nfkc(text),
        casefolded=casefold_text(text),
    )
