from __future__ import annotations

import math
from collections import defaultdict
from itertools import combinations
from pathlib import PurePosixPath
from typing import Any


def top_level_root(path: str) -> str:
    parts = PurePosixPath(path).parts
    return parts[0] if parts else "."


def build_cochange_graph(
    commit_records: list[dict[str, Any]],
) -> dict[str, dict[str, dict[str, float | int | bool]]]:
    graph: dict[str, dict[str, dict[str, float | int | bool]]] = defaultdict(dict)
    for commit in commit_records:
        files = sorted({entry["path"] for entry in commit.get("files", [])})
        if len(files) < 2:
            continue
        for left, right in combinations(files, 2):
            edge = graph[left].setdefault(
                right,
                {
                    "support_count": 0,
                    "cross_folder": top_level_root(left) != top_level_root(right),
                    "total_files_touched": 0,
                },
            )
            edge["support_count"] = int(edge["support_count"]) + 1
            edge["total_files_touched"] = int(edge["total_files_touched"]) + int(
                commit.get("file_count", len(files))
            )
            reverse = graph[right].setdefault(left, dict(edge))
            reverse["support_count"] = edge["support_count"]
            reverse["total_files_touched"] = edge["total_files_touched"]
    return {path: dict(neighbors) for path, neighbors in graph.items()}


def pagerank_for_cochange_graph(
    graph: dict[str, dict[str, dict[str, float | int | bool]]],
    *,
    iterations: int = 20,
    damping: float = 0.85,
) -> dict[str, float]:
    nodes = sorted(graph)
    if not nodes:
        return {}
    ranks = {node: 1.0 / len(nodes) for node in nodes}
    for _ in range(iterations):
        next_ranks = {node: (1.0 - damping) / len(nodes) for node in nodes}
        for node, neighbors in graph.items():
            total_weight = sum(float(edge["support_count"]) for edge in neighbors.values())
            if total_weight <= 0:
                continue
            for neighbor, edge in neighbors.items():
                weight = float(edge["support_count"]) / total_weight
                next_ranks[neighbor] += damping * ranks[node] * weight
        ranks = next_ranks
    return ranks


def support_and_lift(
    graph: dict[str, dict[str, dict[str, float | int | bool]]],
    commit_records: list[dict[str, Any]],
) -> dict[tuple[str, str], dict[str, float]]:
    file_commit_counts: dict[str, int] = defaultdict(int)
    commit_total = max(1, len(commit_records))
    for commit in commit_records:
        for entry in commit.get("files", []):
            file_commit_counts[entry["path"]] += 1
    results: dict[tuple[str, str], dict[str, float]] = {}
    for source, neighbors in graph.items():
        for target, edge in neighbors.items():
            pair = (source, target) if source <= target else (target, source)
            if pair in results:
                continue
            support = float(edge["support_count"])
            expected = (
                (file_commit_counts[source] / commit_total)
                * (file_commit_counts[target] / commit_total)
            )
            observed = support / commit_total
            lift = observed / expected if expected > 0 else 0.0
            pmi = math.log2(lift) if lift > 0 else 0.0
            results[pair] = {
                "support_count": support,
                "lift_score": lift,
                "pmi_score": pmi,
            }
    return results
