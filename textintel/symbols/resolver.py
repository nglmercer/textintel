from __future__ import annotations

from textintel.core.types import SymbolInstance
from textintel.language.segmentation import segment_message
from textintel.symbols.knowledge import readings_for_token


def resolve_symbols(text: str, max_readings: int = 8) -> list[SymbolInstance]:
    out: list[SymbolInstance] = []
    for seg in segment_message(text):
        if seg.segment_type in {"emoji", "symbol", "number"}:
            raw_r = readings_for_token(seg.text, max_readings)
            readings = [(t, w) for t, w, _ in raw_r] or [(seg.text, 1.0)]
            if not raw_r and seg.segment_type == "emoji":
                readings = [("unknown", 0.3)]
            out.append(
                SymbolInstance(
                    text=seg.text,
                    start=seg.start,
                    end=seg.end,
                    kind=seg.segment_type,
                    readings=readings,
                )
            )
    return out
