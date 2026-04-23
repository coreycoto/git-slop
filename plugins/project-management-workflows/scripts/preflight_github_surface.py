from __future__ import annotations

import json
import sys
import tomllib
from pathlib import Path

OFFICIAL_GITHUB_PLUGIN = "github@openai-curated"
EXPECTED_GITHUB_CONNECTOR_ID = "connector_76869538009648d5b282a4bb21c3d157"
PLUGIN_ROOT = Path(__file__).resolve().parents[1]


def _load_json(path: Path) -> dict[str, object]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError(f"{path} must contain a JSON object.")
    return payload


def _load_toml(path: Path) -> dict[str, object]:
    if not path.exists():
        return {}
    payload = tomllib.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError(f"{path} must contain a TOML mapping.")
    return payload


def _extract_connector_id(app_payload: dict[str, object], source_path: Path) -> str | None:
    apps = app_payload.get("apps")
    if not isinstance(apps, dict):
        raise ValueError(f"{source_path} must define an apps mapping.")
    github = apps.get("github")
    if not isinstance(github, dict):
        raise ValueError(f"{source_path} must define apps.github.")
    connector_id = github.get("id")
    if not isinstance(connector_id, str) or not connector_id:
        raise ValueError(f"{source_path} must define apps.github.id.")
    return connector_id


def _find_cached_official_github_plugin(home: Path) -> tuple[Path, Path] | None:
    plugin_root = home / ".codex" / "plugins" / "cache" / "openai-curated" / "github"
    if not plugin_root.exists():
        return None
    for plugin_dir in sorted(plugin_root.iterdir()):
        manifest_path = plugin_dir / ".codex-plugin" / "plugin.json"
        app_path = plugin_dir / ".app.json"
        if not manifest_path.exists():
            continue
        _load_json(manifest_path)
        if app_path.exists():
            _load_json(app_path)
        return manifest_path, app_path
    return None


def validate_github_surface(plugin_root: Path, *, home: Path | None = None) -> list[str]:
    home = home or Path.home()
    cached_plugin = _find_cached_official_github_plugin(home)
    if cached_plugin is None:
        return [
            "Official GitHub Codex plugin not detected in "
            "~/.codex/plugins/cache/openai-curated/github. Install "
            f"{OFFICIAL_GITHUB_PLUGIN} before using GitHub-touching local "
            "interactive workflows from project-management-workflows."
        ]
    _, official_app_path = cached_plugin

    config_path = home / ".codex" / "config.toml"
    config = _load_toml(config_path)
    plugins = config.get("plugins")
    if isinstance(plugins, dict):
        github_plugin = plugins.get(OFFICIAL_GITHUB_PLUGIN)
        if isinstance(github_plugin, dict) and github_plugin.get("enabled") is False:
            return [
                f"Official GitHub Codex plugin {OFFICIAL_GITHUB_PLUGIN} is installed "
                "but disabled in ~/.codex/config.toml. Re-enable it before using "
                "GitHub-touching local interactive workflows."
            ]

    plugin_app_path = plugin_root / ".app.json"
    if not plugin_app_path.exists():
        return [
            "Repo-local project-management-workflows plugin is missing .app.json. "
            "Restore the bundled GitHub connector mapping before using GitHub-touching skills."
        ]
    try:
        plugin_app_payload = _load_json(plugin_app_path)
        connector_id = _extract_connector_id(plugin_app_payload, plugin_app_path)
    except ValueError as exc:
        return [str(exc)]

    if connector_id != EXPECTED_GITHUB_CONNECTOR_ID:
        return [
            f"{plugin_app_path} must map apps.github.id to "
            f"{EXPECTED_GITHUB_CONNECTOR_ID}, found {connector_id}."
        ]

    if official_app_path.exists():
        try:
            official_connector_id = _extract_connector_id(
                _load_json(official_app_path),
                official_app_path,
            )
        except ValueError as exc:
            return [str(exc)]
        if connector_id != official_connector_id:
            return [
                "Repo-local project-management-workflows .app.json does not match the "
                "official GitHub plugin connector mapping. "
                f"Expected {official_connector_id}, found {connector_id}."
            ]

    apps = config.get("apps")
    if not isinstance(apps, dict):
        return [
            f"GitHub connector {connector_id} is not discoverable in ~/.codex/config.toml "
            f"under [apps.{connector_id}] or [apps.{connector_id}.tools.*]."
        ]

    default_app_config = apps.get("_default")
    if isinstance(default_app_config, dict) and default_app_config.get("enabled") is False:
        return [
            f"GitHub connector {connector_id} is configured, but the default app surface "
            "is disabled via [apps._default] in ~/.codex/config.toml."
        ]

    connector_config = apps.get(connector_id)
    if not isinstance(connector_config, dict):
        return [
            f"GitHub connector {connector_id} is not discoverable in ~/.codex/config.toml "
            f"under [apps.{connector_id}] or [apps.{connector_id}.tools.*]."
        ]

    if connector_config.get("enabled") is False:
        return [
            f"GitHub connector {connector_id} is present but disabled via "
            f"[apps.{connector_id}] in ~/.codex/config.toml."
        ]

    return []


def main() -> int:
    errors = validate_github_surface(PLUGIN_ROOT)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print(
        "Official GitHub Codex plugin and bundled GitHub connector mapping detected."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
