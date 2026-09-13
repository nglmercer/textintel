from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class SimilarityWeights:
    """Configurable channel weights for combined compare score.

    Missing/None channel scores are skipped and remaining weights are renormalized.
    """

    character: float = 0.22
    lexical: float = 0.18
    visual: float = 0.18
    decoded: float = 0.28
    obfuscation: float = 0.14
    semantic: float = 0.0
    phonetic: float = 0.0

    def as_dict(self) -> dict[str, float]:
        return {
            "character": self.character,
            "lexical": self.lexical,
            "visual": self.visual,
            "decoded": self.decoded,
            "obfuscation": self.obfuscation,
            "semantic": self.semantic,
            "phonetic": self.phonetic,
        }


@dataclass
class EngineConfig:
    max_input_length: int = 8192
    max_segments: int = 256
    beam_width: int = 20
    max_candidates: int = 10
    max_symbol_readings: int = 8
    max_recursion: int = 8
    similarity_weights: SimilarityWeights = field(default_factory=SimilarityWeights)
    semantic: bool = False
    phonetic: bool = False
