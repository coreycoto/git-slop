from __future__ import annotations

from collections import defaultdict
from datetime import datetime, timezone
from typing import Any

from git_slop.core.models import RepositoryFacts
from git_slop.costs.protocols import CostAnalyzer


class VolatilityCostAnalyzer(CostAnalyzer):
    id = "volatility"
    version = "1"
    experimental = False

    def analyze(self, facts: RepositoryFacts) -> dict[str, dict[str, Any]]:
        token_counts = {
            record["path"]: int(record["tokens"])
            for record in facts.file_records
        }
        commits_by_path: dict[str, list[dict[str, Any]]] = defaultdict(list)
        token_churn: dict[str, int] = defaultdict(int)
        now = datetime.now(timezone.utc)
        for commit in facts.history.commit_records:
            timestamp = int(commit.get("timestamp", 0))
            age_days = max(0.0, (now.timestamp() - timestamp) / 86400) if timestamp else 0.0
            recency_weight = 1.0 / (1.0 + (age_days / 30.0))
            for entry in commit.get("files", []):
                path = entry["path"]
                commits_by_path[path].append(
                    {
                        "timestamp": timestamp,
                        "recency_weight": recency_weight,
                        "token_delta": int(entry.get("token_delta", 0)),
                    }
                )
                token_churn[path] += abs(int(entry.get("token_delta", 0)))
        per_file: dict[str, dict[str, Any]] = {}
        for record in facts.file_records:
            path = record["path"]
            recent_commits = commits_by_path.get(path, [])
            token_churn_window = token_churn.get(path, 0)
            late_window = [
                item
                for item in recent_commits
                if item["timestamp"] and (now.timestamp() - item["timestamp"]) <= (30 * 86400)
            ]
            late_token_churn = sum(abs(int(item["token_delta"])) for item in late_window)
            relative_token_churn = token_churn_window / max(1, token_counts.get(path, 0))
            late_churn_spike = late_token_churn / max(1, token_churn_window)
            per_file[path] = {
                "commit_count_window": int(record["revisions_window"]),
                "recency_weighted_commits": round(
                    sum(float(item["recency_weight"]) for item in recent_commits),
                    6,
                ),
                "line_churn_window": int(record["churn_lines_window"]),
                "token_churn_window": int(token_churn_window),
                "relative_token_churn": round(relative_token_churn, 6),
                "late_churn_spike": round(late_churn_spike, 6),
                "volatility_pressure": round(float(record["churn_pressure"]), 6),
            }
        return per_file
