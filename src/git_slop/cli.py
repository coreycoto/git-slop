from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Sequence

import yaml

from . import __version__
from .config import config_path, latest_dir, load_config, slop_gitignore_path, write_default_files
from .detector import run_detector
from .git import resolve_repo_root
from .reporting import failing_records, find_report_record, load_report

PROJECT_NAME = "git-slop"


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog=PROJECT_NAME,
        description="Find the files that cost too much context.",
    )
    subparsers = parser.add_subparsers(dest="command", metavar="command", required=True)

    init_parser = subparsers.add_parser(
        "init",
        help="Scaffold .slop/ config, ignore rules, and state directories.",
    )
    init_parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite generated config files.",
    )
    init_parser.set_defaults(handler=_run_init)

    find_parser = subparsers.add_parser(
        "find",
        help="Scan the repository and generate hotspot reports.",
    )
    find_parser.set_defaults(handler=_run_find)

    show_parser = subparsers.add_parser(
        "show",
        help="Show metrics and reasons for one file or folder.",
    )
    show_parser.add_argument("target_path", help="Repo-relative file or folder path.")
    show_parser.add_argument("--report", help="Report path. Defaults to .slop/latest/report.json.")
    show_parser.add_argument("--format", choices=("text", "json"), default="text")
    show_parser.set_defaults(handler=_run_show)

    check_parser = subparsers.add_parser(
        "check",
        help="Evaluate an existing report against CI thresholds.",
    )
    check_parser.add_argument("--report", help="Report path. Defaults to .slop/latest/report.json.")
    check_parser.add_argument(
        "--fail-on-context-band",
        choices=("compact", "healthy", "warning", "critical"),
        default=None,
        help="Override the config default fail threshold for context_band.",
    )
    check_parser.add_argument(
        "--fail-on-priority-band",
        choices=("watchlist", "needs_refactor", "should_refactor", "must_refactor"),
        default=None,
        help="Override the config default fail threshold for priority_band.",
    )
    check_parser.set_defaults(handler=_run_check)

    version_parser = subparsers.add_parser("version", help="Print version information.")
    version_parser.set_defaults(handler=_run_version)

    return parser


def _report_path(repo_root) -> str:
    return str(latest_dir(repo_root) / "report.json")


def _run_init(args: argparse.Namespace) -> int:
    repo_root = resolve_repo_root()
    results = write_default_files(repo_root, force=bool(args.force))
    print(f"Initialized {config_path(repo_root).relative_to(repo_root)} ({results['config']}).")
    print(
        f"Initialized {slop_gitignore_path(repo_root).relative_to(repo_root)} "
        f"({results['gitignore']})."
    )
    print("Ensured .slop/latest/, .slop/runs/, and .slop/cache/ exist.")
    return 0


def _run_find(_args: argparse.Namespace) -> int:
    repo_root = resolve_repo_root()
    result = run_detector(repo_root)
    print(result["table"])
    print(f"Wrote report to {result['artifact_paths']['report_json']}.")
    print(f"Wrote YAML report to {result['artifact_paths']['report_yaml']}.")
    print(f"Wrote summary to {result['artifact_paths']['summary_md']}.")
    return 0


def _load_default_report(
    repo_root,
    explicit_report: str | None,
) -> tuple[dict[str, object] | None, str]:
    report_path = explicit_report or _report_path(repo_root)
    try:
        return load_report(Path(report_path)), report_path
    except FileNotFoundError:
        return None, report_path


def _run_show(args: argparse.Namespace) -> int:
    repo_root = resolve_repo_root()
    report, report_path = _load_default_report(repo_root, args.report)
    if report is None:
        print(f"Report not found: {report_path}")
        return 2
    candidate = (repo_root / args.target_path).resolve()
    try:
        target_path = candidate.relative_to(repo_root).as_posix()
    except ValueError:
        target_path = args.target_path.strip() or "."
    record = find_report_record(report, target_path)
    if record is None and target_path != ".":
        record = find_report_record(report, target_path.rstrip("/"))
    if record is None:
        print(f"No record found for '{args.target_path}' in {report_path}.")
        return 2
    if args.format == "json":
        print(json.dumps(record, indent=2, sort_keys=True))
    else:
        print(yaml.safe_dump(record, sort_keys=False), end="")
    return 0


def _run_check(args: argparse.Namespace) -> int:
    repo_root = resolve_repo_root()
    report, report_path = _load_default_report(repo_root, args.report)
    if report is None:
        print(f"Report not found: {report_path}")
        return 2
    config = load_config(repo_root)
    fail_on_context_band = args.fail_on_context_band or config["check"]["fail_on_context_band"]
    fail_on_priority_band = args.fail_on_priority_band or config["check"]["fail_on_priority_band"]
    failures = failing_records(
        report,
        fail_on_context_band=fail_on_context_band,
        fail_on_priority_band=fail_on_priority_band,
    )
    if not failures:
        print(
            "Check passed: "
            f"no file records met or exceeded context={fail_on_context_band} "
            f"or priority={fail_on_priority_band}."
        )
        return 0
    print(
        "Check failed: "
        f"{len(failures)} file records met or exceeded context={fail_on_context_band} "
        f"or priority={fail_on_priority_band}."
    )
    for failure in failures[:10]:
        print(
            f"- {failure['path']} "
            f"(priority={failure['priority_band']}, "
            f"context={failure['context_band']}, "
            f"score={failure['priority_score']})"
        )
    return 1


def _run_version(_args: argparse.Namespace) -> int:
    print(f"{PROJECT_NAME} {__version__}")
    return 0


def main(argv: Sequence[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    handler = getattr(args, "handler", None)
    if handler is None:
        parser.print_help()
        return 2
    return int(handler(args))
