"""Public API: TextIntelligence engine and fingerprint types."""

from textintel.core.config import EngineConfig, SimilarityWeights
from textintel.core.types import (
    CharacterFeatures,
    CharacterSimilarity,
    ComparisonResult,
    DecodedCandidate,
    LanguageCandidate,
    LexicalFeatures,
    MessageFingerprint,
    MessageSegment,
    ObfuscationFeatures,
    PhoneticCandidate,
    SpokenCandidate,
    SymbolInstance,
    Transformation,
    UnicodeFeatures,
)
from textintel.engine.analyzer import TextIntelligence

__all__ = [
    "CharacterFeatures",
    "CharacterSimilarity",
    "ComparisonResult",
    "DecodedCandidate",
    "EngineConfig",
    "LanguageCandidate",
    "LexicalFeatures",
    "MessageFingerprint",
    "MessageSegment",
    "ObfuscationFeatures",
    "PhoneticCandidate",
    "SimilarityWeights",
    "SpokenCandidate",
    "SymbolInstance",
    "TextIntelligence",
    "Transformation",
    "UnicodeFeatures",
]
