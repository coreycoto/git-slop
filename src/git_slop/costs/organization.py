from __future__ import annotations

from typing import Any

from git_slop.core.models import RepositoryFacts
from git_slop.costs.protocols import OverlayAnalyzer
from git_slop.organization import (
    build_organization_health as _build_organization_health,
)
from git_slop.organization import (
    clusters_for_path,
    folder_clusters_for_prefix,
    folder_relationships_for_prefix,
    relationships_for_path,
    top_organization_file_overlays,
)


class OrganizationHealthAnalyzer(OverlayAnalyzer):
    id = "organization_health"
    version = "1"
    experimental = True

    def analyze(self, facts: RepositoryFacts) -> dict[str, Any]:
        organization_config = facts.config["organization"]
        candidate_records = [
            record
            for record in facts.file_records
            if int(organization_config["min_file_tokens"])
            <= int(record["tokens"])
            <= int(organization_config["max_file_tokens"])
        ]
        if len(candidate_records) < 2:
            candidate_records = list(facts.file_records)
        candidate_limit = int(organization_config["candidate_file_limit"])
        candidate_records = sorted(
            candidate_records,
            key=lambda item: (-int(item["tokens"]), -float(item["priority_score"]), item["path"]),
        )[:candidate_limit]
        return _build_organization_health(
            facts.repo_root,
            candidate_records,
            facts.history.to_dict(),
            facts.config,
        )


__all__ = [
    "OrganizationHealthAnalyzer",
    "clusters_for_path",
    "folder_clusters_for_prefix",
    "folder_relationships_for_prefix",
    "relationships_for_path",
    "top_organization_file_overlays",
]
