from __future__ import annotations

from collections import defaultdict
from typing import Any

from git_slop.core.models import RepositoryFacts
from git_slop.costs.protocols import CostAnalyzer


class LoadCostAnalyzer(CostAnalyzer):
    id = "load"
    version = "1"
    experimental = False

    def analyze(self, facts: RepositoryFacts) -> dict[str, dict[str, Any]]:
        records = facts.file_records or []
        total_tokens = max(1, sum(int(record["tokens"]) for record in records))
        folder_tokens: dict[str, int] = defaultdict(int)
        folder_children: dict[str, list[int]] = defaultdict(list)
        for record in records:
            path = record["path"]
            current = path.rsplit("/", 1)[0] if "/" in path else "."
            folder_tokens[current] += int(record["tokens"])
            folder_children[current].append(int(record["tokens"]))
        per_file: dict[str, dict[str, Any]] = {}
        for record in records:
            folder_path = record["path"].rsplit("/", 1)[0] if "/" in record["path"] else "."
            child_tokens = sorted(
                folder_children.get(folder_path, [int(record["tokens"])]),
                reverse=True,
            )
            top_file_share = int(record["tokens"]) / max(1, folder_tokens[folder_path])
            top_3_file_share = sum(child_tokens[:3]) / max(1, folder_tokens[folder_path])
            per_file[record["path"]] = {
                "file_token_count": int(record["tokens"]),
                "folder_token_count": int(folder_tokens[folder_path]),
                "top_file_share": round(top_file_share, 6),
                "top_3_file_share": round(top_3_file_share, 6),
                "token_concentration_ratio": round(int(record["tokens"]) / total_tokens, 6),
                "context_band": record["context_band"],
                "load_pressure": round(float(record["context_pressure"]), 6),
            }
        return per_file
