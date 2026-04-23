from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import tomllib
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = REPO_ROOT / ".agents" / "plugins" / "marketplace-source.json"
SECTION_HEADER_RE = re.compile(r"^\[(?P<section>[^\]]+)\]\s*$")
SHA_RE = re.compile(r"[0-9a-f]{40}")
MarketplaceEntry = tuple[str, dict[str, Any]]


def load_manifest(path: Path = MANIFEST_PATH) -> dict[str, str]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    for key in ("marketplace_name", "source_url", "ref", "required_plugin"):
        value = payload.get(key)
        if not isinstance(value, str) or not value.strip():
            raise ValueError(f"{path} must define a non-empty string for {key}.")
    if not SHA_RE.fullmatch(payload["ref"]):
        raise ValueError(f"{path} ref must be a 40-character lowercase SHA.")
    return payload


def _target_home(home: str | None = None) -> Path:
    if home:
        return Path(home).expanduser().resolve()
    return Path(os.environ.get("HOME", str(Path.home()))).expanduser().resolve()


def _config_path(home: Path) -> Path:
    return home / ".codex" / "config.toml"


def _load_config(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    return tomllib.loads(path.read_text(encoding="utf-8"))


def _matching_marketplaces(
    config: dict[str, Any],
    manifest: dict[str, str],
) -> list[MarketplaceEntry]:
    marketplaces = config.get("marketplaces")
    if not isinstance(marketplaces, dict):
        return []
    matches: list[tuple[str, dict[str, Any]]] = []
    for name, entry in marketplaces.items():
        if not isinstance(name, str) or not isinstance(entry, dict):
            continue
        source_type = entry.get("source_type")
        source = entry.get("source")
        if source_type == "git" and source == manifest["source_url"]:
            matches.append((name, entry))
            continue
        if name == manifest["marketplace_name"]:
            matches.append((name, entry))
    return matches


def status_marketplace(manifest: dict[str, str], *, home: str | None = None) -> dict[str, Any]:
    target_home = _target_home(home)
    config_path = _config_path(target_home)
    config = _load_config(config_path)
    matches = _matching_marketplaces(config, manifest)
    exact = [
        name
        for name, entry in matches
        if entry.get("source_type") == "git"
        and entry.get("source") == manifest["source_url"]
        and entry.get("ref") == manifest["ref"]
    ]
    conflicts = [
        {
            "name": name,
            "source_type": entry.get("source_type"),
            "source": entry.get("source"),
            "ref": entry.get("ref"),
        }
        for name, entry in matches
        if name not in exact
    ]
    cache_root = target_home / ".codex" / ".tmp" / "marketplaces"
    return {
        "home": str(target_home),
        "config_path": str(config_path),
        "marketplace_name": manifest["marketplace_name"],
        "source_url": manifest["source_url"],
        "ref": manifest["ref"],
        "required_plugin": manifest["required_plugin"],
        "installed": len(exact) == 1,
        "matching_marketplace_names": exact,
        "conflicts": conflicts,
        "cache_root": str(cache_root),
        "installed_root_exists": any((cache_root / name).exists() for name in exact),
    }


def _strip_marketplace_sections(config_text: str, section_names: set[str]) -> str:
    lines = config_text.splitlines(keepends=True)
    kept: list[str] = []
    skip = False
    for line in lines:
        match = SECTION_HEADER_RE.match(line.strip())
        if match:
            section = match.group("section")
            if section.startswith("marketplaces."):
                name = section[len("marketplaces.") :].strip("\"'")
                skip = name in section_names
            else:
                skip = False
        if not skip:
            kept.append(line)
    text = "".join(kept).rstrip()
    return f"{text}\n" if text else ""


def _remove_sections(target_home: Path, section_names: set[str]) -> None:
    if not section_names:
        return
    config_path = _config_path(target_home)
    if config_path.exists():
        config_text = config_path.read_text(encoding="utf-8")
        config_path.write_text(
            _strip_marketplace_sections(config_text, section_names),
            encoding="utf-8",
        )
    cache_root = target_home / ".codex" / ".tmp" / "marketplaces"
    for name in section_names:
        shutil.rmtree(cache_root / name, ignore_errors=True)


def _write_marketplace_config(target_home: Path, manifest: dict[str, str]) -> None:
    config_path = _config_path(target_home)
    config_path.parent.mkdir(parents=True, exist_ok=True)
    existing = config_path.read_text(encoding="utf-8").rstrip() if config_path.exists() else ""
    block = "\n".join(
        [
            f"[marketplaces.{manifest['marketplace_name']}]",
            'last_updated = "'
            + datetime.now(timezone.utc)
            .isoformat(timespec="seconds")
            .replace("+00:00", "Z")
            + '"',
            'source_type = "git"',
            f'source = "{manifest["source_url"]}"',
            f'ref = "{manifest["ref"]}"',
            "",
        ]
    )
    text = f"{existing}\n\n{block}" if existing else block
    config_path.write_text(text, encoding="utf-8")


def install_marketplace(manifest: dict[str, str], *, home: str | None = None) -> dict[str, Any]:
    target_home = _target_home(home)
    before = status_marketplace(manifest, home=str(target_home))
    existing_sections = set(before["matching_marketplace_names"])
    existing_sections.update(
        conflict["name"] for conflict in before["conflicts"] if isinstance(conflict, dict)
    )
    if before["installed"] and not before["conflicts"]:
        return {**before, "action": "noop"}
    _remove_sections(target_home, existing_sections)
    if shutil.which("codex") is None:
        _write_marketplace_config(target_home, manifest)
        after = status_marketplace(manifest, home=str(target_home))
        return {**after, "action": "config_write"}
    env = os.environ.copy()
    env["HOME"] = str(target_home)
    completed = subprocess.run(
        [
            "codex",
            "marketplace",
            "add",
            manifest["source_url"],
            "--ref",
            manifest["ref"],
        ],
        check=False,
        capture_output=True,
        text=True,
        env=env,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            "codex marketplace add failed: "
            f"{completed.stderr.strip() or completed.stdout.strip()}"
        )
    after = status_marketplace(manifest, home=str(target_home))
    return {
        **after,
        "action": "codex_marketplace_add",
        "stdout": completed.stdout.strip(),
    }


def remove_marketplace(manifest: dict[str, str], *, home: str | None = None) -> dict[str, Any]:
    target_home = _target_home(home)
    before = status_marketplace(manifest, home=str(target_home))
    section_names = set(before["matching_marketplace_names"])
    section_names.update(
        conflict["name"] for conflict in before["conflicts"] if isinstance(conflict, dict)
    )
    _remove_sections(target_home, section_names)
    after = status_marketplace(manifest, home=str(target_home))
    return {**after, "action": "remove"}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Bootstrap the pinned agent-plugins marketplace source."
    )
    parser.add_argument("action", choices=("install", "remove", "status"))
    parser.add_argument("--home", help="Override the target home directory.")
    parser.add_argument(
        "--format",
        choices=("text", "json"),
        default="text",
        help="Output format.",
    )
    return parser.parse_args()


def _render_text(payload: dict[str, Any]) -> str:
    lines = [
        f"action: {payload.get('action', 'status')}",
        f"installed: {payload['installed']}",
        f"marketplace: {payload['marketplace_name']}",
        f"source: {payload['source_url']}",
        f"ref: {payload['ref']}",
        f"home: {payload['home']}",
    ]
    if payload.get("matching_marketplace_names"):
        lines.append(
            "matching_marketplace_names: "
            + ", ".join(payload["matching_marketplace_names"])
        )
    if payload.get("conflicts"):
        lines.append(f"conflicts: {len(payload['conflicts'])}")
    if payload.get("stdout"):
        lines.append(f"stdout: {payload['stdout']}")
    return "\n".join(lines)


def main() -> int:
    args = parse_args()
    manifest = load_manifest()
    if args.action == "install":
        payload = install_marketplace(manifest, home=args.home)
    elif args.action == "remove":
        payload = remove_marketplace(manifest, home=args.home)
    else:
        payload = status_marketplace(manifest, home=args.home)
        payload["action"] = "status"
    if args.format == "json":
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        print(_render_text(payload))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
