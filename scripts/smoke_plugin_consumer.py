from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any

from git_slop.integrations.agents.codex_surface import validate_codex_surface

REPO_ROOT = Path(__file__).resolve().parents[1]
PLUGIN_URL = "https://github.com/coreycoto/agent-plugins.git"
DOCS_TAXONOMY_SKILL = "$project-management-workflows:docs-taxonomy"
SCRIPT_ROOT = Path(__file__).resolve().parent
BOOTSTRAP_PATH = SCRIPT_ROOT / "bootstrap_agent_plugins_marketplace.py"


def _load_bootstrap_module():
    spec = importlib.util.spec_from_file_location(
        "bootstrap_agent_plugins_marketplace",
        BOOTSTRAP_PATH,
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("Unable to load bootstrap_agent_plugins_marketplace.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BOOTSTRAP = _load_bootstrap_module()
REAL_CODEX_HOME = Path.home() / ".codex"


def _skill_available_from_output(combined_output: str) -> bool:
    normalized = combined_output.lower().replace("’", "'")
    if not normalized.strip():
        return False
    available_markers = (
        "codex.skill.injected",
        "skills/docs-taxonomy/skill.md",
    )
    unavailable_markers = (
        "not available in this session",
        "isn't available in this session",
        "is not available in this session",
    )
    if any(marker in normalized for marker in unavailable_markers):
        return False
    return any(marker in normalized for marker in available_markers)


def _copy_codex_auth_state(target_home: Path) -> bool:
    target_codex_home = target_home / ".codex"
    copied = False
    for relative_path in ("auth.json", ".credentials.json", "installation_id"):
        source_path = REAL_CODEX_HOME / relative_path
        if not source_path.exists():
            continue
        destination = target_codex_home / relative_path
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source_path, destination)
        copied = True
    return copied


def _write_clean_room_repo(root: Path) -> Path:
    repo = root / "clean-room-docs"
    (repo / ".codex").mkdir(parents=True, exist_ok=True)
    (repo / "notes").mkdir(parents=True, exist_ok=True)
    (repo / "AGENTS.md").write_text(
        "# Repository Agent Policy\n\nKeep docs and guidance in the right buckets.\n",
        encoding="utf-8",
    )
    (repo / ".codex" / "README.md").write_text(
        "# Codex Runtime\n\nThis clean-room repo only exists for smoke coverage.\n",
        encoding="utf-8",
    )
    (repo / "README.md").write_text(
        "# Clean-room Docs Taxonomy Smoke\n\nUse the installed plugin skill to audit docs drift.\n",
        encoding="utf-8",
    )
    (repo / "notes" / "guidance.md").write_text(
        "# Guidance\n\nThis file is intentionally simple. "
        "The smoke only verifies that the installed plugin skill can load "
        "and return a response without the GitHub runtime.\n",
        encoding="utf-8",
    )
    subprocess.run(["git", "init", "-q"], cwd=repo, check=True)
    subprocess.run(["git", "config", "user.name", "Codex Smoke"], cwd=repo, check=True)
    subprocess.run(
        ["git", "config", "user.email", "codex-smoke@example.invalid"],
        cwd=repo,
        check=True,
    )
    subprocess.run(["git", "add", "."], cwd=repo, check=True)
    subprocess.run(
        ["git", "commit", "-qm", "Initial clean-room docs taxonomy fixture"],
        cwd=repo,
        check=True,
    )
    return repo


def _write_git_config(path: Path, local_agent_plugins_repo: Path) -> None:
    repo_git_dir = (local_agent_plugins_repo.resolve() / ".git").as_posix()
    path.write_text(
        "\n".join(
            [
                f"[url \"file://{repo_git_dir}\"]",
                f"\tinsteadOf = {PLUGIN_URL}",
                "",
            ]
        ),
        encoding="utf-8",
    )


def run_smoke(
    repo_root: Path = REPO_ROOT,
    *,
    run_codex: bool = False,
    local_agent_plugins_repo: Path | None = None,
) -> dict[str, Any]:
    errors = validate_codex_surface(repo_root)
    if errors:
        raise RuntimeError("Consumer Codex surface validation failed:\n" + "\n".join(errors))

    manifest = BOOTSTRAP.load_manifest(
        repo_root / ".agents" / "plugins" / "marketplace-source.json"
    )

    result: dict[str, Any] = {
        "repo_root": str(repo_root),
        "marketplace_name": manifest["marketplace_name"],
        "source_url": manifest["source_url"],
        "ref": manifest["ref"],
        "required_plugin": manifest["required_plugin"],
        "run_codex": run_codex,
    }

    if not run_codex:
        return result

    with tempfile.TemporaryDirectory(prefix="git-slop-plugin-smoke-") as tmp_dir:
        temp_root = Path(tmp_dir)
        clean_repo = _write_clean_room_repo(temp_root)
        env = os.environ.copy()
        env["HOME"] = str(temp_root)
        git_config = temp_root / "gitconfig"
        if local_agent_plugins_repo is not None:
            _write_git_config(git_config, local_agent_plugins_repo)
            env["GIT_CONFIG_GLOBAL"] = str(git_config)
        previous_git_config = os.environ.get("GIT_CONFIG_GLOBAL")
        if "GIT_CONFIG_GLOBAL" in env:
            os.environ["GIT_CONFIG_GLOBAL"] = env["GIT_CONFIG_GLOBAL"]
        elif "GIT_CONFIG_GLOBAL" in os.environ:
            del os.environ["GIT_CONFIG_GLOBAL"]
        install_payload = BOOTSTRAP.install_marketplace(manifest, home=str(temp_root))
        status_payload = BOOTSTRAP.status_marketplace(manifest, home=str(temp_root))
        if previous_git_config is None:
            os.environ.pop("GIT_CONFIG_GLOBAL", None)
        else:
            os.environ["GIT_CONFIG_GLOBAL"] = previous_git_config
        copied_auth_state = _copy_codex_auth_state(temp_root)
        output_path = temp_root / "codex-last-message.txt"
        completed = subprocess.run(
            [
                "codex",
                "exec",
                "-c",
                'approval_policy="never"',
                "--cd",
                str(clean_repo),
                "--sandbox",
                "workspace-write",
                "--ephemeral",
                "-o",
                str(output_path),
                (
                    f"Use {DOCS_TAXONOMY_SKILL} to audit this clean-room repository. "
                "Keep the work docs-only and summarize the result."
                ),
            ],
            check=False,
            capture_output=True,
            text=True,
            env=env,
        )
        if completed.returncode != 0:
            raise RuntimeError(
                "codex exec docs-taxonomy smoke failed: "
                f"{completed.stderr.strip() or completed.stdout.strip()}"
            )
        result["bootstrap_install"] = install_payload
        result["bootstrap_status"] = status_payload
        result["copied_auth_state"] = copied_auth_state
        result["codex_stdout"] = completed.stdout.strip()
        result["codex_last_message"] = output_path.read_text(encoding="utf-8").strip()
        combined_output = "\n".join(
            value
            for value in (
                completed.stdout.strip(),
                completed.stderr.strip(),
                result["codex_last_message"],
            )
            if value
        )
        result["skill_available"] = _skill_available_from_output(combined_output)
        if not result["skill_available"]:
            result["skill_diagnostic"] = combined_output
        result["clean_repo"] = str(clean_repo)
        result["clean_repo_status"] = subprocess.run(
            ["git", "status", "--short"],
            cwd=clean_repo,
            check=False,
            capture_output=True,
            text=True,
        ).stdout.strip()
        return result


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Smoke the git-slop plugin consumer runtime.")
    parser.add_argument("--repo-root", default=str(REPO_ROOT), help="Consumer repo root.")
    parser.add_argument(
        "--run-codex",
        action="store_true",
        help="Run a clean-room docs-taxonomy Codex pass after the metadata smoke.",
    )
    parser.add_argument(
        "--local-agent-plugins-repo",
        help=(
            "Optional local agent-plugins repo path for Git insteadOf rewriting "
            "during Codex smoke."
        ),
    )
    parser.add_argument("--json-out", help="Optional JSON output path.")
    parser.add_argument("--format", choices=("text", "json"), default="text")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    payload = run_smoke(
        Path(args.repo_root).expanduser().resolve(),
        run_codex=args.run_codex,
        local_agent_plugins_repo=(
            Path(args.local_agent_plugins_repo).expanduser().resolve()
            if args.local_agent_plugins_repo
            else None
        ),
    )
    if args.json_out:
        Path(args.json_out).expanduser().resolve().write_text(
            json.dumps(payload, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    if args.format == "json":
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        print("Plugin consumer smoke passed.")
        print(f"repo_root: {payload['repo_root']}")
        print(f"marketplace: {payload['marketplace_name']}")
        print(f"source_url: {payload['source_url']}")
        if payload["run_codex"]:
            print(f"clean-room docs-taxonomy skill_available: {payload['skill_available']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
