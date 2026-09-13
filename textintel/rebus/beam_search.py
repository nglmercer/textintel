from __future__ import annotations

from dataclasses import dataclass

from textintel.rebus.tokenizer import rebus_tokens, token_readings


@dataclass
class BeamNode:
    text: str
    score: float
    transforms: list[tuple[str, str, str]]  # source, replacement, type
    lang: str | None


def beam_decode(
    text: str,
    *,
    beam_width: int = 20,
    max_candidates: int = 10,
    max_symbol_readings: int = 8,
) -> list[BeamNode]:
    tokens = rebus_tokens(text)
    if not tokens:
        return [BeamNode(text="", score=0.0, transforms=[], lang=None)]

    beam = [BeamNode(text="", score=1.0, transforms=[], lang=None)]
    for tok in tokens:
        readings = token_readings(tok, max_symbol_readings)
        nxt: list[BeamNode] = []
        for node in beam:
            for surface, w, lang, ttype in readings:
                new_text = node.text + surface
                new_score = node.score * max(0.05, w)
                tr = list(node.transforms)
                if ttype != "identity" and surface.casefold() != tok.casefold():
                    tr.append((tok, surface, ttype))
                use_lang = lang if lang != "und" else node.lang
                nxt.append(BeamNode(new_text, new_score, tr, use_lang))
        nxt.sort(key=lambda n: n.score, reverse=True)
        beam = nxt[:beam_width]

    beam.sort(key=lambda n: n.score, reverse=True)
    return beam[: max(max_candidates, beam_width)]
