from __future__ import annotations

from typing import Protocol


class EmbeddingProvider(Protocol):
    def embed(self, texts: list[str]) -> list[list[float]]:
        ...


class NullEmbeddingProvider:
    def embed(self, texts: list[str]) -> list[list[float]]:
        return []
