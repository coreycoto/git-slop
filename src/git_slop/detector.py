from __future__ import annotations

from typing import Any

from .config import ensure_state_dirs, load_config
from .core.pipeline import build_repository_facts, run_analyzers
from .reporting import (
    build_action_queue,
    build_report,
    render_terminal_output,
    utc_timestamp_slug,
    write_report_bundle,
)


def run_detector(repo_root, *, print_table: bool = True) -> dict[str, Any]:
    ensure_state_dirs(repo_root)
    config = load_config(repo_root)
    facts = build_repository_facts(repo_root, config)
    analysis = run_analyzers(facts)
    action_queue = build_action_queue(facts.file_records)
    generated_at = facts.repo.get("head_commit_timestamp") or utc_timestamp_slug()
    report = build_report(
        repo=facts.repo,
        config=config,
        file_records=facts.file_records,
        folder_records=facts.folder_records,
        action_queue=action_queue,
        stable_costs=analysis["costs"],
        overlay_results=analysis["overlays"],
        skipped=facts.inventory.skipped,
        generated_at=generated_at,
    )
    artifact_paths = write_report_bundle(repo_root=repo_root, report=report, run_slug=generated_at)
    return {
        "report": report,
        "artifact_paths": artifact_paths,
        "table": render_terminal_output(report) if print_table else "",
    }
