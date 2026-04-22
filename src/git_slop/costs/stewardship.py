from __future__ import annotations

import math
from collections import Counter, defaultdict
from datetime import datetime, timezone
from typing import Any

from git_slop.core.models import RepositoryFacts
from git_slop.costs.protocols import OverlayAnalyzer


def _shannon_entropy(weights: list[int]) -> float:
    total = sum(weights)
    if total <= 0:
        return 0.0
    entropy = 0.0
    for weight in weights:
        probability = weight / total
        entropy -= probability * math.log2(probability)
    return entropy


def _is_bot(author: str, markers: list[str]) -> bool:
    lowered = author.lower()
    return any(marker in lowered for marker in markers)


class StewardshipOverlayAnalyzer(OverlayAnalyzer):
    id = "stewardship"
    version = "1"
    experimental = True

    def analyze(self, facts: RepositoryFacts) -> dict[str, Any]:
        markers = [marker.lower() for marker in facts.config["stewardship"]["bot_name_markers"]]
        commits_by_path: dict[str, list[dict[str, Any]]] = defaultdict(list)
        for commit in facts.history.commit_records:
            for entry in commit.get("files", []):
                commits_by_path[entry["path"]].append(commit)
        now = datetime.now(timezone.utc)
        file_overlays: list[dict[str, Any]] = []
        for record in facts.file_records:
            path = record["path"]
            commits = commits_by_path.get(path, [])
            authors = [
                commit.get("author_key") or commit.get("author_name") or "unknown"
                for commit in commits
            ]
            counts = Counter(authors)
            top_author_share = (counts.most_common(1)[0][1] / len(authors)) if authors else 0.0
            non_bot_commits = [
                commit
                for commit in commits
                if not _is_bot(
                    str(commit.get("author_key") or commit.get("author_name") or "unknown"),
                    markers,
                )
            ]
            latest_non_bot_ts = max(
                [int(commit.get("timestamp", 0)) for commit in non_bot_commits],
                default=0,
            )
            days_since_non_bot_edit = (
                int((now.timestamp() - latest_non_bot_ts) // 86400) if latest_non_bot_ts else 0
            )
            recent_cutoff = now.timestamp() - (90 * 86400)
            recent_authors = {
                commit.get("author_key") or commit.get("author_name") or "unknown"
                for commit in commits
                if int(commit.get("timestamp", 0)) >= recent_cutoff
            }
            recent_diversity = len(recent_authors) / max(1, len(counts))
            stewardship_pressure = min(
                1.0,
                (float(record["priority_score"]) / 100.0)
                * top_author_share
                * (1.0 - min(1.0, recent_diversity)),
            )
            file_overlays.append(
                {
                    "path": path,
                    "author_count_window": len(counts),
                    "author_entropy": round(_shannon_entropy(list(counts.values())), 6),
                    "top_author_share": round(top_author_share, 6),
                    "days_since_non_bot_edit": days_since_non_bot_edit,
                    "recent_maintainer_diversity": round(recent_diversity, 6),
                    "stewardship_pressure": round(stewardship_pressure, 6),
                }
            )
        return {
            "analysis_status": "experimental",
            "analysis_version": 1,
            "files": sorted(file_overlays, key=lambda item: item["path"]),
        }
