from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any, Final

PLUGIN_NAME: Final = "project-management-workflows"
MARKETPLACE_NAME: Final = "codex-home-local-marketplace"
MARKETPLACE_DISPLAY_NAME: Final = "Codex Home Local Marketplace"
DEFAULT_CATEGORY: Final = "Productivity"
DEFAULT_INSTALLATION: Final = "INSTALLED_BY_DEFAULT"
DEFAULT_AUTHENTICATION: Final = "ON_INSTALL"
PLUGIN_ROOT = Path(__file__).resolve().parents[1]


def _marketplace_path(home: Path) -> Path:
    return home / ".agents" / "plugins" / "marketplace.json"


def _marketplace_root(home: Path) -> Path:
    return _marketplace_path(home).parent


def _plugin_link_path(home: Path) -> Path:
    return _marketplace_root(home) / PLUGIN_NAME


def _seed_marketplace() -> dict[str, Any]:
    return {
        "name": MARKETPLACE_NAME,
        "interface": {"displayName": MARKETPLACE_DISPLAY_NAME},
        "plugins": [],
    }


def _load_marketplace(home: Path) -> tuple[Path, dict[str, Any]]:
    marketplace_path = _marketplace_path(home)
    if not marketplace_path.exists():
        return marketplace_path, _seed_marketplace()
    payload = json.loads(marketplace_path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError(f"{marketplace_path} must contain a JSON object.")
    plugins = payload.get("plugins")
    if not isinstance(plugins, list):
        raise ValueError(f"{marketplace_path} must contain a plugins array.")
    return marketplace_path, payload


def _write_marketplace(marketplace_path: Path, payload: dict[str, Any]) -> None:
    marketplace_path.parent.mkdir(parents=True, exist_ok=True)
    marketplace_path.write_text(
        json.dumps(payload, indent=2, sort_keys=False) + "\n",
        encoding="utf-8",
    )


def _plugin_entry() -> dict[str, Any]:
    return {
        "name": PLUGIN_NAME,
        "source": {
            "source": "local",
            "path": f"./{PLUGIN_NAME}",
        },
        "policy": {
            "installation": DEFAULT_INSTALLATION,
            "authentication": DEFAULT_AUTHENTICATION,
        },
        "category": DEFAULT_CATEGORY,
    }


def get_status(home: Path) -> dict[str, Any]:
    marketplace_path = _marketplace_path(home)
    installed = False
    marketplace_source_path: str | None = None
    source_path: str | None = None
    plugin_link_path = _plugin_link_path(home)
    link_exists = plugin_link_path.exists() or plugin_link_path.is_symlink()
    symlink_target: str | None = None
    if plugin_link_path.is_symlink():
        symlink_target = str(plugin_link_path.resolve())
    if marketplace_path.exists():
        payload = json.loads(marketplace_path.read_text(encoding="utf-8"))
        plugins = payload.get("plugins", []) if isinstance(payload, dict) else []
        if isinstance(plugins, list):
            for plugin in plugins:
                if not isinstance(plugin, dict) or plugin.get("name") != PLUGIN_NAME:
                    continue
                installed = True
                source = plugin.get("source")
                if isinstance(source, dict):
                    path = source.get("path")
                    if isinstance(path, str):
                        marketplace_source_path = path
                        source_path = str((marketplace_path.parent / path).resolve())
                break
    return {
        "plugin_name": PLUGIN_NAME,
        "installed": installed,
        "marketplace_path": str(marketplace_path),
        "marketplace_source_path": marketplace_source_path,
        "source_path": source_path,
        "plugin_link_path": str(plugin_link_path),
        "plugin_link_exists": link_exists,
        "plugin_link_target": symlink_target,
    }


def _ensure_plugin_link(home: Path, plugin_root: Path) -> Path:
    plugin_link_path = _plugin_link_path(home)
    plugin_link_path.parent.mkdir(parents=True, exist_ok=True)
    resolved_plugin_root = plugin_root.resolve()

    if plugin_link_path.is_symlink():
        current_target = plugin_link_path.resolve()
        if current_target == resolved_plugin_root:
            return plugin_link_path
        plugin_link_path.unlink()
    elif plugin_link_path.exists():
        raise ValueError(
            f"{plugin_link_path} already exists and is not a managed symlink."
        )

    plugin_link_path.symlink_to(resolved_plugin_root, target_is_directory=True)
    return plugin_link_path


def install(home: Path, plugin_root: Path) -> dict[str, Any]:
    marketplace_path, payload = _load_marketplace(home)
    _ensure_plugin_link(home, plugin_root)
    plugins = payload["plugins"]
    assert isinstance(plugins, list)
    entry = _plugin_entry()
    updated = False
    for index, plugin in enumerate(plugins):
        if isinstance(plugin, dict) and plugin.get("name") == PLUGIN_NAME:
            plugins[index] = entry
            updated = True
            break
    if not updated:
        plugins.append(entry)
    _write_marketplace(marketplace_path, payload)
    return get_status(home)


def remove(home: Path) -> dict[str, Any]:
    marketplace_path, payload = _load_marketplace(home)
    plugin_link_path = _plugin_link_path(home)
    plugins = payload["plugins"]
    assert isinstance(plugins, list)
    payload["plugins"] = [
        plugin
        for plugin in plugins
        if not (isinstance(plugin, dict) and plugin.get("name") == PLUGIN_NAME)
    ]
    remaining = payload["plugins"]
    assert isinstance(remaining, list)
    if plugin_link_path.is_symlink():
        plugin_link_path.unlink()
    elif plugin_link_path.exists():
        raise ValueError(f"{plugin_link_path} exists but is not a managed symlink.")
    if remaining:
        _write_marketplace(marketplace_path, payload)
    elif marketplace_path.exists():
        marketplace_path.unlink()
        for parent in (marketplace_path.parent, marketplace_path.parent.parent):
            try:
                parent.rmdir()
            except OSError:
                break
    return get_status(home)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Manage a home-local marketplace entry for project-management-workflows."
    )
    parser.add_argument("command", choices=("install", "remove", "status"))
    parser.add_argument(
        "--home",
        help="Target home directory. Defaults to the current home directory.",
    )
    parser.add_argument(
        "--plugin-root",
        default=str(PLUGIN_ROOT),
        help="Plugin root to publish in the home-local marketplace entry.",
    )
    parser.add_argument("--json-out", help="Optional JSON output path.")
    parser.add_argument("--format", choices=("text", "json"), default="text")
    return parser.parse_args()


def _emit_payload(payload: dict[str, Any], args: argparse.Namespace) -> None:
    if args.json_out:
        Path(args.json_out).expanduser().resolve().write_text(
            json.dumps(payload, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    if args.format == "json":
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        state = "installed" if payload["installed"] else "not installed"
        marketplace_path = payload["marketplace_path"]
        source_path = payload["source_path"] or "<none>"
        print(f"{PLUGIN_NAME}: {state}")
        print(f"marketplace: {marketplace_path}")
        print(f"source: {source_path}")


def main() -> int:
    args = parse_args()
    home = Path(args.home).expanduser().resolve() if args.home else Path.home()
    plugin_root = Path(args.plugin_root).expanduser().resolve()

    if args.command == "install":
        payload = install(home, plugin_root)
    elif args.command == "remove":
        payload = remove(home)
    else:
        payload = get_status(home)

    _emit_payload(payload, args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
