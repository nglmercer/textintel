from __future__ import annotations

from textintel.core.config import EngineConfig, SimilarityWeights
from textintel.core.exceptions import InputTooLongError
from textintel.core.types import (
    ComparisonResult,
    DecodedCandidate,
    LexicalFeatures,
    MessageFingerprint,
    SpokenCandidate,
)
from textintel.comparison.scorer import score_fingerprints
from textintel.language.detector import detect_languages
from textintel.language.segmentation import segment_message
from textintel.lexical.character import char_features
from textintel.lexical.ngrams import word_ngrams
from textintel.lexical.tokenizer import simple_lemmas, tokenize
from textintel.normalization.leetspeak import apply_leet
from textintel.normalization.repetition import collapse_repetition
from textintel.normalization.unicode import casefold_text, nfkc
from textintel.normalization.whitespace import normalize_whitespace
from textintel.obfuscation.features import obfuscation_features
from textintel.phonetic.g2p import NullG2PProvider
from textintel.rebus.decoder import RebusDecoder
from textintel.semantic.embeddings import NullEmbeddingProvider
from textintel.symbols.resolver import resolve_symbols
from textintel.visual.unicode_features import analyze_unicode


class TextIntelligence:
    def __init__(
        self,
        *,
        semantic: bool | None = None,
        phonetic: bool | None = None,
        config: EngineConfig | None = None,
        similarity_weights: SimilarityWeights | None = None,
        beam_width: int | None = None,
        max_candidates: int | None = None,
        max_symbol_readings: int | None = None,
        max_input_length: int | None = None,
    ) -> None:
        cfg = config or EngineConfig()
        if semantic is not None:
            cfg.semantic = semantic
        if phonetic is not None:
            cfg.phonetic = phonetic
        if similarity_weights is not None:
            cfg.similarity_weights = similarity_weights
        if beam_width is not None:
            cfg.beam_width = beam_width
        if max_candidates is not None:
            cfg.max_candidates = max_candidates
        if max_symbol_readings is not None:
            cfg.max_symbol_readings = max_symbol_readings
        if max_input_length is not None:
            cfg.max_input_length = max_input_length
        self.config = cfg
        self._decoder = RebusDecoder(cfg)
        self._embed = NullEmbeddingProvider()
        self._g2p = NullG2PProvider()

    def _check_length(self, text: str) -> None:
        if len(text) > self.config.max_input_length:
            raise InputTooLongError(
                f"input length {len(text)} exceeds max_input_length={self.config.max_input_length}"
            )

    def analyze(self, text: str) -> MessageFingerprint:
        self._check_length(text)
        raw = text  # never replaced
        uni = analyze_unicode(raw)
        tokens = tokenize(raw)
        lemmas = simple_lemmas(tokens)
        lex = LexicalFeatures(
            tokens=tokens,
            lemmas=lemmas,
            word_ngrams=word_ngrams(tokens, 2),
            jaccard_ready=set(lemmas),
        )
        symbols = resolve_symbols(raw, self.config.max_symbol_readings)
        segs = segment_message(raw, self.config.max_segments)
        langs = detect_languages(raw)
        obf = obfuscation_features(raw, uni)
        normalized = normalize_whitespace(
            collapse_repetition(apply_leet(casefold_text(nfkc(raw))))
        )
        rebus = self._decoder.decode(raw, max_candidates=self.config.max_candidates)
        spoken = [
            SpokenCandidate(text=c.text, language=c.language, confidence=c.score, source="rebus")
            for c in rebus
        ]
        phonetic_cands = []
        embeddings: dict[str, list[float]] = {}
        if self.config.phonetic:
            lang = langs[0].language if langs else "und"
            phonetic_cands = [self._g2p.phonemize(raw, lang)]
        if self.config.semantic:
            vecs = self._embed.embed([raw])
            if vecs:
                embeddings["default"] = vecs[0]
        return MessageFingerprint(
            raw=raw,
            normalized=normalized,
            language_candidates=langs,
            segments=segs,
            tokens=tokens,
            lemmas=lemmas,
            char_features=char_features(raw),
            unicode_features=uni,
            symbols=symbols,
            lexical_features=lex,
            semantic_embeddings=embeddings,
            spoken_candidates=spoken,
            phonetic_candidates=phonetic_cands,
            rebus_candidates=rebus,
            obfuscation_features=obf,
            metadata={"semantic_enabled": self.config.semantic, "phonetic_enabled": self.config.phonetic},
        )

    def decode(
        self,
        text: str,
        languages: list[str] | None = None,
        max_candidates: int | None = None,
    ) -> list[DecodedCandidate]:
        self._check_length(text)
        return self._decoder.decode(text, languages=languages, max_candidates=max_candidates)

    def compare(self, a: str, b: str) -> ComparisonResult:
        fa = self.analyze(a)
        fb = self.analyze(b)
        semantic: float | None = None
        phonetic: float | None = None
        if self.config.semantic:
            semantic = None  # provider not loaded; explicit absent unless extras wired
        if self.config.phonetic:
            phonetic = None
        return score_fingerprints(
            fa,
            fb,
            self.config.similarity_weights,
            semantic=semantic,
            phonetic=phonetic,
        )
