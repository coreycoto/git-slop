from __future__ import annotations

from typing import Any

from .config import ensure_state_dirs, load_config
from .git import list_tracked_files, repo_metadata
from .history import build_history_metrics
from .inventory import build_inventory
from .reporting import (
    build_action_queue,
    build_folder_records,
    build_report,
    render_terminal_table,
    utc_timestamp_slug,
    write_report_bundle,
)
from .scoring import apply_scoring
from .tokenization import apply_token_metrics


def run_detector(repo_root, *, print_table: bool = True) -> dict[str, Any]:
    ensure_state_dirs(repo_root)
    config = load_config(repo_root)
    tracked_paths = list_tracked_files(repo_root)
    inventory_records, skipped = build_inventory(
        repo_root,
        tracked_paths,
        ignore_globs=list(config["inventory"]["ignore_globs"]),
    )
    tokenized_records = apply_token_metrics(inventory_records, config)
    history_metrics = build_history_metrics(repo_root, tokenized_records, config)

    merged_records: list[dict[str, Any]] = []
    for record in tokenized_records:
        merged_record = dict(record)
        merged_record.update(history_metrics[record["path"]])
        merged_records.append(merged_record)

    scored_records = apply_scoring(merged_records, config)
    folder_records = build_folder_records(scored_records, config)
    action_queue = build_action_queue(scored_records)
    generated_at = utc_timestamp_slug()
    report = build_report(
        repo=repo_metadata(repo_root),
        config=config,
        file_records=scored_records,
        folder_records=folder_records,
        action_queue=action_queue,
        skipped=skipped,
        generated_at=generated_at,
    )
    artifact_paths = write_report_bundle(repo_root=repo_root, report=report, run_slug=generated_at)
    return {
        "report": report,
        "artifact_paths": artifact_paths,
        "table": render_terminal_table(action_queue) if print_table else "",
    }
