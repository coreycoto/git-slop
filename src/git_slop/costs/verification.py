from __future__ import annotations

from collections import defaultdict
from pathlib import PurePosixPath
from typing import Any

from git_slop.core.models import RepositoryFacts
from git_slop.costs.protocols import OverlayAnalyzer


def _is_test_path(path: str, markers: list[str]) -> bool:
    return any(marker in path for marker in markers)


def _same_basename(left: str, right: str) -> bool:
    left_name = PurePosixPath(left).name.split(".")[0]
    right_name = PurePosixPath(right).name.split(".")[0]
    return left_name == right_name


class VerificationOverlayAnalyzer(OverlayAnalyzer):
    id = "verification"
    version = "1"
    experimental = True

    def analyze(self, facts: RepositoryFacts) -> dict[str, Any]:
        markers = list(facts.config["verification"]["test_path_markers"])
        test_paths = [
            record["path"]
            for record in facts.file_records
            if _is_test_path(record["path"], markers)
        ]
        commits_by_path: dict[str, list[dict[str, Any]]] = defaultdict(list)
        test_cochanges: dict[str, int] = defaultdict(int)
        for commit in facts.history.commit_records:
            touched_paths = {entry["path"] for entry in commit.get("files", [])}
            touched_tests = {path for path in touched_paths if _is_test_path(path, markers)}
            for path in touched_paths:
                commits_by_path[path].append(commit)
                if touched_tests and not _is_test_path(path, markers):
                    test_cochanges[path] += 1
        file_overlays: list[dict[str, Any]] = []
        for record in facts.file_records:
            path = record["path"]
            nearby_tests = [
                test_path
                for test_path in test_paths
                if test_path.startswith(path.rsplit("/", 1)[0] if "/" in path else "")
                or _same_basename(path, test_path)
            ]
            adjacency = min(1.0, len(nearby_tests) / 2) if nearby_tests else 0.0
            commit_count = max(1, len(commits_by_path.get(path, [])))
            test_cochange_ratio = test_cochanges.get(path, 0) / commit_count
            hotspot_without_nearby_tests = bool(record["slop_score"] >= 65 and adjacency == 0.0)
            churn_without_test_churn = bool(
                record["churn_pressure"] >= 0.6 and test_cochange_ratio < 0.25
            )
            verification_gap = min(
                1.0,
                (float(record["slop_score"]) / 100.0)
                * (1.0 - adjacency)
                * (1.0 - test_cochange_ratio),
            )
            file_overlays.append(
                {
                    "path": path,
                    "test_adjacency_score": round(adjacency, 6),
                    "test_cochange_ratio": round(test_cochange_ratio, 6),
                    "hotspot_without_nearby_tests": hotspot_without_nearby_tests,
                    "churn_without_test_churn": churn_without_test_churn,
                    "verification_gap": round(verification_gap, 6),
                    "nearby_test_paths": sorted(nearby_tests)[:10],
                }
            )
        return {
            "analysis_status": "experimental",
            "analysis_version": 1,
            "files": sorted(file_overlays, key=lambda item: item["path"]),
        }
