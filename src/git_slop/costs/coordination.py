from __future__ import annotations

import math
from collections import defaultdict
from pathlib import PurePosixPath
from typing import Any

from git_slop.core.models import RepositoryFacts
from git_slop.costs.protocols import CostAnalyzer
from git_slop.graphs.cochange import build_cochange_graph, pagerank_for_cochange_graph


def _top_level_root(path: str) -> str:
    parts = PurePosixPath(path).parts
    return parts[0] if parts else "."


def _estimated_hunks(line_delta: int) -> int:
    if line_delta <= 0:
        return 1
    return max(1, math.ceil(line_delta / 20))


class CoordinationCostAnalyzer(CostAnalyzer):
    id = "coordination"
    version = "1"
    experimental = False

    def analyze(self, facts: RepositoryFacts) -> dict[str, dict[str, Any]]:
        commits_by_path: dict[str, list[dict[str, Any]]] = defaultdict(list)
        cochange_graph = build_cochange_graph(facts.history.commit_records)
        pagerank = pagerank_for_cochange_graph(cochange_graph)
        for commit in facts.history.commit_records:
            files = commit.get("files", [])
            file_count = max(1, int(commit.get("file_count", len(files))))
            root_count = max(1, int(commit.get("top_level_root_count", 1)))
            diffusion = (
                0.35 * min(1.0, math.log1p(file_count) / math.log(25))
                + 0.25
                * min(
                    1.0,
                    math.log1p(
                        sum(
                            _estimated_hunks(int(item.get("line_delta", 0)))
                            for item in files
                        )
                    )
                    / math.log(50),
                )
                + 0.20 * min(1.0, math.log1p(root_count) / math.log(10))
                + 0.20 * min(1.0, float(commit.get("change_entropy", 0.0)) / 3.0)
            )
            for entry in files:
                commits_by_path[entry["path"]].append(
                    {
                        "files_touched": file_count,
                        "folders_touched": root_count,
                        "edit_hunks": _estimated_hunks(int(entry.get("line_delta", 0))),
                        "diffusion": diffusion,
                    }
                )
        results: dict[str, dict[str, Any]] = {}
        for record in facts.file_records:
            path = record["path"]
            commit_records = commits_by_path.get(path, [])
            neighbors = cochange_graph.get(path, {})
            files_touched = [int(item["files_touched"]) for item in commit_records] or [1]
            folders_touched = [int(item["folders_touched"]) for item in commit_records] or [1]
            edit_hunks = [int(item["edit_hunks"]) for item in commit_records] or [1]
            diffusion_values = [float(item["diffusion"]) for item in commit_records] or [0.0]
            cross_folder_neighbors = [
                neighbor
                for neighbor in neighbors
                if _top_level_root(neighbor) != _top_level_root(path)
            ]
            cross_folder_ratio = len(cross_folder_neighbors) / max(1, len(neighbors))
            cochange_degree = len(neighbors)
            cochange_centrality = cochange_degree / max(1, len(facts.file_records) - 1)
            coordination_pressure = min(
                1.0,
                (0.5 * (sum(diffusion_values) / max(1, len(diffusion_values))))
                + (0.3 * cochange_centrality)
                + (0.2 * cross_folder_ratio),
            )
            results[path] = {
                "files_touched_per_change": round(sum(files_touched) / len(files_touched), 6),
                "folders_touched_per_change": round(sum(folders_touched) / len(folders_touched), 6),
                "edit_hunks_per_change": round(sum(edit_hunks) / len(edit_hunks), 6),
                "cochange_degree": cochange_degree,
                "cochange_centrality": round(cochange_centrality, 6),
                "cross_folder_cochange_ratio": round(cross_folder_ratio, 6),
                "change_diffusion": round(sum(diffusion_values) / len(diffusion_values), 6),
                "coordination_pressure": round(coordination_pressure, 6),
                "cochange_pagerank": round(float(pagerank.get(path, 0.0)), 6),
            }
        return results
