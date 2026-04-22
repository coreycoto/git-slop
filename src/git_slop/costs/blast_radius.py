from __future__ import annotations

from pathlib import PurePosixPath
from typing import Any

from git_slop.core.models import RepositoryFacts
from git_slop.costs.protocols import OverlayAnalyzer
from git_slop.graphs.cochange import build_cochange_graph, pagerank_for_cochange_graph


def _top_level_root(path: str) -> str:
    parts = PurePosixPath(path).parts
    return parts[0] if parts else "."


class BlastRadiusOverlayAnalyzer(OverlayAnalyzer):
    id = "blast_radius"
    version = "1"
    experimental = True

    def analyze(self, facts: RepositoryFacts) -> dict[str, Any]:
        graph = build_cochange_graph(facts.history.commit_records)
        pagerank = pagerank_for_cochange_graph(graph)
        commits_by_path: dict[str, list[int]] = {}
        for record in facts.file_records:
            path = record["path"]
            commits_by_path[path] = []
        for commit in facts.history.commit_records:
            file_count = int(commit.get("file_count", len(commit.get("files", []))))
            for entry in commit.get("files", []):
                commits_by_path.setdefault(entry["path"], []).append(file_count)
        file_overlays: list[dict[str, Any]] = []
        for record in facts.file_records:
            path = record["path"]
            neighbors = graph.get(path, {})
            weighted_degree = sum(int(edge["support_count"]) for edge in neighbors.values())
            cross_folder_neighbors = [
                neighbor
                for neighbor in neighbors
                if _top_level_root(neighbor) != _top_level_root(path)
            ]
            cross_folder_ratio = len(cross_folder_neighbors) / max(1, len(neighbors))
            avg_changeset = (
                sum(commits_by_path.get(path, [1])) / max(1, len(commits_by_path.get(path, [1])))
            )
            blast_pressure = min(
                1.0,
                (0.40 * min(1.0, len(neighbors) / max(1, len(facts.file_records) - 1)))
                + (0.30 * min(1.0, avg_changeset / 25.0))
                + (0.30 * cross_folder_ratio),
            )
            file_overlays.append(
                {
                    "path": path,
                    "cochange_degree": len(neighbors),
                    "weighted_cochange_degree": weighted_degree,
                    "cochange_pagerank": round(float(pagerank.get(path, 0.0)), 6),
                    "cross_folder_coupling": round(cross_folder_ratio, 6),
                    "average_changeset_size_when_touched": round(avg_changeset, 6),
                    "blast_radius_pressure": round(blast_pressure, 6),
                }
            )
        return {
            "analysis_status": "experimental",
            "analysis_version": 1,
            "files": sorted(file_overlays, key=lambda item: item["path"]),
        }
