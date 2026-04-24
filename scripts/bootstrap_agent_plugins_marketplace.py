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


def _cache_root(home: Path) -> Path:
    return home / ".codex" / "plugins" / "cache"


def _marketplace_cache_root(home: Path, manifest: dict[str, str]) -> Path:
    return home / ".codex" / ".tmp" / "marketplaces" / manifest["marketplace_name"]


def _plugin_key(manifest: dict[str, str]) -> str:
    return f"{manifest['required_plugin']}@{manifest['marketplace_name']}"


def _plugin_cache_path(home: Path, manifest: dict[str, str]) -> Path:
    return (
        _cache_root(home)
        / manifest["marketplace_name"]
        / manifest["required_plugin"]
        / manifest["ref"]
    )


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
    plugin_key = _plugin_key(manifest)
    plugins = config.get("plugins")
    plugin_config = plugins.get(plugin_key) if isinstance(plugins, dict) else None
    plugin_enabled = (
        plugin_config.get("enabled") is True if isinstance(plugin_config, dict) else False
    )
    plugin_cache_path = _plugin_cache_path(target_home, manifest)
    plugin_materialized = (plugin_cache_path / ".codex-plugin" / "plugin.json").is_file()
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
        "plugin_key": plugin_key,
        "plugin_enabled": plugin_enabled,
        "plugin_cache_path": str(plugin_cache_path),
        "plugin_materialized": plugin_materialized,
        "ready": len(exact) == 1 and not conflicts and plugin_enabled and plugin_materialized,
    }


def _strip_sections(config_text: str, section_names: set[str]) -> str:
    lines = config_text.splitlines(keepends=True)
    kept: list[str] = []
    skip = False
    for line in lines:
        match = SECTION_HEADER_RE.match(line.strip())
        if match:
            section = match.group("section")
            skip = section in section_names
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
            _strip_sections(config_text, section_names),
            encoding="utf-8",
        )
    cache_root = target_home / ".codex" / ".tmp" / "marketplaces"
    for name in section_names:
        if name.startswith("marketplaces."):
            marketplace_name = name[len("marketplaces.") :].strip("\"'")
            shutil.rmtree(cache_root / marketplace_name, ignore_errors=True)


def _marketplace_section_name(name: str) -> str:
    return f"marketplaces.{name}"


def _plugin_section_name(plugin_key: str) -> str:
    return f'plugins."{plugin_key}"'


def _write_marketplace_config(target_home: Path, manifest: dict[str, str]) -> None:
    config_path = _config_path(target_home)
    config_path.parent.mkdir(parents=True, exist_ok=True)
    existing = config_path.read_text(encoding="utf-8") if config_path.exists() else ""
    existing = _strip_sections(
        existing,
        {
            _marketplace_section_name(manifest["marketplace_name"]),
            _plugin_section_name(_plugin_key(manifest)),
        },
    ).rstrip()
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
            f'[plugins."{_plugin_key(manifest)}"]',
            "enabled = true",
            "",
        ]
    )
    text = f"{existing}\n\n{block}" if existing else block
    config_path.write_text(text, encoding="utf-8")


def _ensure_plugin_enabled(target_home: Path, manifest: dict[str, str]) -> None:
    config_path = _config_path(target_home)
    config_path.parent.mkdir(parents=True, exist_ok=True)
    existing = config_path.read_text(encoding="utf-8") if config_path.exists() else ""
    plugin_section = _plugin_section_name(_plugin_key(manifest))
    existing = _strip_sections(existing, {plugin_section}).rstrip()
    block = "\n".join([f"[{plugin_section}]", "enabled = true", ""])
    text = f"{existing}\n\n{block}" if existing else block
    config_path.write_text(text, encoding="utf-8")


def _marketplace_root(target_home: Path, manifest: dict[str, str]) -> Path | None:
    config = _load_config(_config_path(target_home))
    marketplaces = config.get("marketplaces")
    if not isinstance(marketplaces, dict):
        return None
    entry = marketplaces.get(manifest["marketplace_name"])
    if not isinstance(entry, dict):
        return None
    source_type = entry.get("source_type")
    if source_type == "git":
        root = _marketplace_cache_root(target_home, manifest)
        return root if root.exists() else None
    if source_type == "local":
        source = entry.get("source")
        if isinstance(source, str):
            root = Path(source).expanduser()
            return root if root.exists() else None
    return None


def _plugin_source_path(marketplace_root: Path, manifest: dict[str, str]) -> Path:
    marketplace_path = marketplace_root / ".agents" / "plugins" / "marketplace.json"
    payload = json.loads(marketplace_path.read_text(encoding="utf-8"))
    for plugin in payload.get("plugins", []):
        if not isinstance(plugin, dict) or plugin.get("name") != manifest["required_plugin"]:
            continue
        source = plugin.get("source")
        if not isinstance(source, dict):
            break
        if source.get("source") != "local":
            raise RuntimeError(
                f"{manifest['required_plugin']} must resolve to a local path inside "
                f"{marketplace_path}."
            )
        relative_path = source.get("path")
        if not isinstance(relative_path, str) or not relative_path.strip():
            break
        plugin_root = (marketplace_root / relative_path).resolve()
        if not (plugin_root / ".codex-plugin" / "plugin.json").is_file():
            raise RuntimeError(f"Plugin manifest not found at {plugin_root}.")
        return plugin_root
    raise RuntimeError(
        f"{marketplace_path} does not define plugin {manifest['required_plugin']}."
    )


def _materialize_plugin_cache(target_home: Path, manifest: dict[str, str]) -> bool:
    marketplace_root = _marketplace_root(target_home, manifest)
    if marketplace_root is None:
        return False
    plugin_root = _plugin_source_path(marketplace_root, manifest)
    cache_path = _plugin_cache_path(target_home, manifest)
    shutil.rmtree(cache_path, ignore_errors=True)
    cache_path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(plugin_root, cache_path)
    return True


def _run_marketplace_add(
    manifest: dict[str, str],
    *,
    target_home: Path,
) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["HOME"] = str(target_home)
    commands = (
        ["codex", "marketplace", "add", manifest["source_url"], "--ref", manifest["ref"]],
        [
            "codex",
            "plugin",
            "marketplace",
            "add",
            manifest["source_url"],
            "--ref",
            manifest["ref"],
        ],
    )
    last_completed: subprocess.CompletedProcess[str] | None = None
    for command in commands:
        completed = subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            env=env,
        )
        if completed.returncode == 0:
            return completed
        last_completed = completed
    if last_completed is None:
        raise RuntimeError("No marketplace add command was attempted.")
    return last_completed


def install_marketplace(manifest: dict[str, str], *, home: str | None = None) -> dict[str, Any]:
    target_home = _target_home(home)
    before = status_marketplace(manifest, home=str(target_home))
    existing_sections = {
        _marketplace_section_name(name) for name in before["matching_marketplace_names"]
    }
    existing_sections.update(
        _marketplace_section_name(conflict["name"])
        for conflict in before["conflicts"]
        if isinstance(conflict, dict)
    )
    existing_sections.add(_plugin_section_name(_plugin_key(manifest)))
    if before["ready"]:
        return {**before, "action": "noop"}
    if before["installed"] and not before["conflicts"] and before["installed_root_exists"]:
        _ensure_plugin_enabled(target_home, manifest)
        plugin_materialized = _materialize_plugin_cache(target_home, manifest)
        after = status_marketplace(manifest, home=str(target_home))
        if after["ready"]:
            return {
                **after,
                "action": "materialize",
                "materialize_action": "copy" if plugin_materialized else "skipped",
            }
    _remove_sections(target_home, existing_sections)
    shutil.rmtree(
        _cache_root(target_home) / manifest["marketplace_name"] / manifest["required_plugin"],
        ignore_errors=True,
    )
    if shutil.which("codex") is None:
        _write_marketplace_config(target_home, manifest)
        plugin_materialized = _materialize_plugin_cache(target_home, manifest)
        after = status_marketplace(manifest, home=str(target_home))
        return {
            **after,
            "action": "config_write",
            "materialize_action": "copy" if plugin_materialized else "skipped",
        }
    completed = _run_marketplace_add(manifest, target_home=target_home)
    if completed.returncode != 0:
        raise RuntimeError(
            "codex marketplace add failed: "
            f"{completed.stderr.strip() or completed.stdout.strip()}"
        )
    _ensure_plugin_enabled(target_home, manifest)
    plugin_materialized = _materialize_plugin_cache(target_home, manifest)
    after = status_marketplace(manifest, home=str(target_home))
    return {
        **after,
        "action": "codex_marketplace_add",
        "materialize_action": "copy" if plugin_materialized else "skipped",
        "stdout": completed.stdout.strip(),
    }


def remove_marketplace(manifest: dict[str, str], *, home: str | None = None) -> dict[str, Any]:
    target_home = _target_home(home)
    before = status_marketplace(manifest, home=str(target_home))
    section_names = {
        _marketplace_section_name(name) for name in before["matching_marketplace_names"]
    }
    section_names.update(
        _marketplace_section_name(conflict["name"])
        for conflict in before["conflicts"]
        if isinstance(conflict, dict)
    )
    section_names.add(_plugin_section_name(_plugin_key(manifest)))
    _remove_sections(target_home, section_names)
    shutil.rmtree(
        _cache_root(target_home) / manifest["marketplace_name"] / manifest["required_plugin"],
        ignore_errors=True,
    )
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
        f"plugin_enabled: {payload['plugin_enabled']}",
        f"plugin_materialized: {payload['plugin_materialized']}",
        f"ready: {payload['ready']}",
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
