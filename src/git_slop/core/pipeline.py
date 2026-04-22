from __future__ import annotations

from pathlib import Path
from typing import Any

from git_slop.core.inventory import build_inventory
from git_slop.core.models import (
    BaselineFacts,
    ChangeSetFacts,
    FileFacts,
    HistoryFacts,
    InventoryFacts,
    RepositoryFacts,
)
from git_slop.core.repository import list_tracked_files, repo_metadata
from git_slop.core.token_facts import build_token_facts, content_fingerprint_for_text
from git_slop.costs.blast_radius import BlastRadiusOverlayAnalyzer
from git_slop.costs.coordination import CoordinationCostAnalyzer
from git_slop.costs.load import LoadCostAnalyzer
from git_slop.costs.navigation import NavigationOverlayAnalyzer
from git_slop.costs.organization import OrganizationHealthAnalyzer
from git_slop.costs.semantic_drift import SemanticDriftOverlayAnalyzer
from git_slop.costs.stewardship import StewardshipOverlayAnalyzer
from git_slop.costs.verification import VerificationOverlayAnalyzer
from git_slop.costs.volatility import VolatilityCostAnalyzer
from git_slop.history import build_history_snapshot
from git_slop.scoring import apply_scoring


def _build_inventory_facts(
    repo_root: Path,
    config: dict[str, Any],
) -> tuple[list[dict[str, Any]], InventoryFacts]:
    tracked_paths = list_tracked_files(repo_root)
    inventory_records, skipped = build_inventory(
        repo_root,
        tracked_paths,
        ignore_globs=list(config["inventory"]["ignore_globs"]),
    )
    inventory_facts = InventoryFacts(
        tracked_paths=tracked_paths,
        skipped=skipped,
        files=[
            FileFacts(
                path=record["path"],
                bytes=int(record["bytes"]),
                lines=int(record["lines"]),
                text=str(record["text"]),
                content_fingerprint=content_fingerprint_for_text(str(record["text"])),
            )
            for record in inventory_records
        ],
    )
    return inventory_records, inventory_facts


def build_repository_facts(repo_root: Path, config: dict[str, Any]) -> RepositoryFacts:
    inventory_records, inventory_facts = _build_inventory_facts(repo_root, config)
    token_facts = build_token_facts(repo_root, inventory_facts, config)
    token_facts_by_path = token_facts.by_path()
    tokenized_records: list[dict[str, Any]] = []
    for record in inventory_records:
        file_token_facts = token_facts_by_path[record["path"]]
        tokenized_record = dict(record)
        tokenized_record["tokens"] = file_token_facts.context_token_count
        tokenized_record["context_band"] = file_token_facts.context_band
        tokenized_record["context_pressure"] = file_token_facts.context_pressure
        tokenized_record["structural_tokens"] = list(file_token_facts.structural_tokens)
        tokenized_record["structural_token_count"] = file_token_facts.structural_token_count
        tokenized_record["top_structural_terms"] = list(file_token_facts.top_structural_terms)
        tokenized_records.append(tokenized_record)

    history_snapshot = build_history_snapshot(repo_root, tokenized_records, config)
    history_metrics = history_snapshot["file_metrics"]
    merged_records: list[dict[str, Any]] = []
    for record in tokenized_records:
        merged_record = dict(record)
        merged_record.update(history_metrics[record["path"]])
        merged_records.append(merged_record)
    scored_records = apply_scoring(merged_records, config)

    facts = RepositoryFacts(
        repo_root=repo_root,
        config=config,
        repo=repo_metadata(repo_root),
        inventory=inventory_facts,
        tokens=token_facts,
        history=HistoryFacts(
            file_metrics=history_snapshot["file_metrics"],
            commit_records=history_snapshot["commit_records"],
            repo_baselines=history_snapshot["repo_baselines"],
        ),
        changesets=ChangeSetFacts(commit_records=history_snapshot["commit_records"]),
        baselines=BaselineFacts(repo_baselines=history_snapshot["repo_baselines"]),
        file_records=scored_records,
    )
    return facts


def run_analyzers(facts: RepositoryFacts) -> dict[str, Any]:
    stable_analyzers = [
        LoadCostAnalyzer(),
        VolatilityCostAnalyzer(),
        CoordinationCostAnalyzer(),
    ]
    overlay_analyzers = [
        OrganizationHealthAnalyzer(),
        VerificationOverlayAnalyzer(),
        NavigationOverlayAnalyzer(),
        BlastRadiusOverlayAnalyzer(),
        StewardshipOverlayAnalyzer(),
        SemanticDriftOverlayAnalyzer(),
    ]
    costs = {analyzer.id: analyzer.analyze(facts) for analyzer in stable_analyzers}
    overlays = {analyzer.id: analyzer.analyze(facts) for analyzer in overlay_analyzers}
    return {
        "costs": costs,
        "overlays": overlays,
    }
