from __future__ import annotations

from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class FileFacts:
    path: str
    bytes: int
    lines: int
    text: str
    content_fingerprint: str

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


@dataclass(frozen=True)
class InventoryFacts:
    tracked_paths: list[str]
    files: list[FileFacts]
    skipped: dict[str, int]

    def by_path(self) -> dict[str, FileFacts]:
        return {item.path: item for item in self.files}


@dataclass(frozen=True)
class FileTokenFacts:
    path: str
    context_token_count: int
    context_band: str
    context_pressure: float
    structural_tokens: list[str]
    structural_token_count: int
    top_structural_terms: list[str]

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


@dataclass(frozen=True)
class TokenFacts:
    files: list[FileTokenFacts]
    context_tokenizer_name: str
    structural_tokenizer_version: str

    def by_path(self) -> dict[str, FileTokenFacts]:
        return {item.path: item for item in self.files}


@dataclass(frozen=True)
class HistoryFacts:
    file_metrics: dict[str, dict[str, Any]]
    commit_records: list[dict[str, Any]]
    repo_baselines: dict[str, Any]

    def to_dict(self) -> dict[str, Any]:
        return {
            "file_metrics": self.file_metrics,
            "commit_records": self.commit_records,
            "repo_baselines": self.repo_baselines,
        }


@dataclass(frozen=True)
class ChangeSetFacts:
    commit_records: list[dict[str, Any]]


@dataclass(frozen=True)
class BaselineFacts:
    repo_baselines: dict[str, Any]


@dataclass(frozen=True)
class HotspotScore:
    path: str
    slop_score: float
    slop_band: str
    context_band: str
    reason_codes: list[str]

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


@dataclass(frozen=True)
class OverlayFinding:
    path: str
    overlay_id: str
    payload: dict[str, Any]

    def to_dict(self) -> dict[str, Any]:
        return {
            "path": self.path,
            "overlay_id": self.overlay_id,
            "payload": self.payload,
        }


@dataclass(frozen=True)
class Relationship:
    id: str
    kind: str
    source_path: str
    target_path: str
    evidence_score: float
    payload: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        result = asdict(self)
        return result


@dataclass(frozen=True)
class Cluster:
    id: str
    kind: str
    member_paths: list[str]
    evidence_score: float
    payload: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        result = asdict(self)
        return result


@dataclass
class RepositoryFacts:
    repo_root: Path
    config: dict[str, Any]
    repo: dict[str, Any]
    inventory: InventoryFacts
    tokens: TokenFacts
    history: HistoryFacts
    changesets: ChangeSetFacts
    baselines: BaselineFacts
    file_records: list[dict[str, Any]] = field(default_factory=list)
    folder_records: list[dict[str, Any]] = field(default_factory=list)

    def token_facts_by_path(self) -> dict[str, FileTokenFacts]:
        return self.tokens.by_path()

    def file_facts_by_path(self) -> dict[str, FileFacts]:
        return self.inventory.by_path()
