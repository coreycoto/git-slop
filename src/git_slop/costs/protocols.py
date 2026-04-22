from __future__ import annotations

from typing import Any, Protocol

from git_slop.core.models import RepositoryFacts


class CostAnalyzer(Protocol):
    id: str
    version: str
    experimental: bool

    def analyze(self, facts: RepositoryFacts) -> dict[str, dict[str, Any]]: ...


class OverlayAnalyzer(Protocol):
    id: str
    version: str
    experimental: bool

    def analyze(self, facts: RepositoryFacts) -> dict[str, Any]: ...
