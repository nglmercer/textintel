from __future__ import annotations

from typing import Protocol

from textintel.core.types import PhoneticCandidate


class G2PProvider(Protocol):
    def phonemize(self, text: str, language: str) -> PhoneticCandidate:
        ...


class NullG2PProvider:
    def phonemize(self, text: str, language: str) -> PhoneticCandidate:
        return PhoneticCandidate(
            source=text,
            language=language,
            ipa=None,
            phonemes=[],
            confidence=0.0,
        )
