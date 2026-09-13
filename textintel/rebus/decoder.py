from __future__ import annotations

from textintel.core.config import EngineConfig
from textintel.core.types import DecodedCandidate, Transformation
from textintel.normalization.leetspeak import apply_leet
from textintel.normalization.repetition import collapse_repetition
from textintel.normalization.whitespace import normalize_whitespace
from textintel.rebus.beam_search import beam_decode
from textintel.rebus.scorer import score_candidate
from textintel.visual.homoglyph import confusable_skeleton


class RebusDecoder:
    def __init__(self, config: EngineConfig | None = None) -> None:
        self.config = config or EngineConfig()

    def decode(
        self,
        text: str,
        languages: list[str] | None = None,
        max_candidates: int | None = None,
    ) -> list[DecodedCandidate]:
        cfg = self.config
        k = max_candidates if max_candidates is not None else cfg.max_candidates
        nodes = beam_decode(
            text,
            beam_width=cfg.beam_width,
            max_candidates=max(k * 3, cfg.beam_width),
            max_symbol_readings=cfg.max_symbol_readings,
        )
        extra_views = [
            normalize_whitespace(text),
            text.casefold(),
            apply_leet(text.casefold()),
            collapse_repetition(apply_leet(text.casefold())),
            collapse_repetition(text.casefold()),
            confusable_skeleton(text).casefold(),
            collapse_repetition(confusable_skeleton(apply_leet(text.casefold()))),
        ]
        seen: dict[str, DecodedCandidate] = {}
        for n in nodes:
            total, lex, phon, ctx = score_candidate(n.text, n.score)
            key = n.text.casefold()
            cand = DecodedCandidate(
                text=n.text,
                score=total,
                transformations=[
                    Transformation(source=s, replacement=r, transformation_type=tt)
                    for s, r, tt in n.transforms
                ],
                language=n.lang or (languages[0] if languages else None),
                lexical_score=lex,
                phonetic_score=phon,
                context_score=ctx,
                symbol_score=min(1.0, n.score),
            )
            prev = seen.get(key)
            if prev is None or cand.score > prev.score:
                seen[key] = cand
        for view in extra_views:
            v = view.replace(" ", "")
            if not v:
                continue
            total, lex, phon, ctx = score_candidate(v, 0.4)
            cand = DecodedCandidate(
                text=v,
                score=total * 0.9,
                transformations=[],
                language=languages[0] if languages else None,
                lexical_score=lex,
                phonetic_score=phon,
                context_score=ctx,
                symbol_score=0.3,
            )
            key = v.casefold()
            prev = seen.get(key)
            if prev is None or cand.score > prev.score:
                seen[key] = cand

        ranked = sorted(seen.values(), key=lambda c: c.score, reverse=True)
        # Drop very low-probability branches
        ranked = [c for c in ranked if c.score >= 0.08]
        return ranked[:k]
