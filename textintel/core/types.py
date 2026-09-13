from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class LanguageCandidate:
    language: str
    probability: float


@dataclass
class MessageSegment:
    text: str
    start: int
    end: int
    language_candidates: list[LanguageCandidate]
    segment_type: str


@dataclass
class ConfusableCharacter:
    char: str
    index: int
    script: str
    confusable_with: str | None = None


@dataclass
class UnicodeFeatures:
    scripts: list[str]
    mixed_scripts: bool
    invisible_characters: list[str]
    confusable_characters: list[ConfusableCharacter]
    confusable_skeleton: str | None
    suspicious_unicode_score: float
    nfc: str
    nfkc: str
    casefolded: str


@dataclass
class CharacterFeatures:
    length: int
    letters: int
    digits: int
    whitespace: int
    punctuation: int
    other: int
    ngrams_2: dict[str, int]
    ngrams_3: dict[str, int]


@dataclass
class CharacterSimilarity:
    levenshtein: float
    damerau_levenshtein: float
    jaro: float
    jaro_winkler: float
    ngram_similarity: float
    lcs: float
    combined: float


@dataclass
class LexicalFeatures:
    tokens: list[str]
    lemmas: list[str]
    word_ngrams: list[str]
    jaccard_ready: set[str]


@dataclass
class SymbolInstance:
    text: str
    start: int
    end: int
    kind: str
    readings: list[tuple[str, float]]


@dataclass
class SpokenCandidate:
    text: str
    language: str | None
    confidence: float
    source: str


@dataclass
class PhoneticCandidate:
    source: str
    language: str
    ipa: str | None
    phonemes: list[str]
    confidence: float


@dataclass
class Transformation:
    source: str
    replacement: str
    transformation_type: str


@dataclass
class DecodedCandidate:
    text: str
    score: float
    transformations: list[Transformation]
    language: str | None
    lexical_score: float
    phonetic_score: float
    context_score: float
    symbol_score: float


@dataclass
class ObfuscationFeatures:
    detected: bool
    score: float
    leetspeak: bool
    repetition: bool
    punctuation_flood: bool
    mixed_scripts: bool
    confusables: bool
    flags: list[str]


@dataclass
class MessageFingerprint:
    raw: str
    normalized: str | None
    language_candidates: list[LanguageCandidate]
    segments: list[MessageSegment]
    tokens: list[str]
    lemmas: list[str]
    char_features: CharacterFeatures
    unicode_features: UnicodeFeatures
    symbols: list[SymbolInstance]
    lexical_features: LexicalFeatures
    semantic_embeddings: dict[str, list[float]]
    spoken_candidates: list[SpokenCandidate]
    phonetic_candidates: list[PhoneticCandidate]
    rebus_candidates: list[DecodedCandidate]
    obfuscation_features: ObfuscationFeatures
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass
class ComparisonResult:
    score: float
    character: float | None
    lexical: float | None
    visual: float | None
    decoded_similarity: float | None
    obfuscation: float | None
    semantic: float | None
    phonetic: float | None
    evidence: list[str]
    weights_used: dict[str, float]
