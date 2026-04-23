from __future__ import annotations

import argparse
import json
import sys
import tempfile
from pathlib import Path
from typing import Any

SCRIPT_ROOT = Path(__file__).resolve().parent
if str(SCRIPT_ROOT) not in sys.path:
    sys.path.insert(0, str(SCRIPT_ROOT))

from manage_home_local_plugin import PLUGIN_ROOT, get_status, install, remove
from preflight_github_surface import EXPECTED_GITHUB_CONNECTOR_ID, validate_github_surface


def _write_official_github_plugin(home: Path, *, connector_id: str) -> None:
    plugin_dir = (
        home
        / ".codex"
        / "plugins"
        / "cache"
        / "openai-curated"
        / "github"
        / "smoke-hash"
    )
    manifest_dir = plugin_dir / ".codex-plugin"
    manifest_dir.mkdir(parents=True, exist_ok=True)
    (manifest_dir / "plugin.json").write_text(
        json.dumps({"name": "github", "version": "0.0.0"}) + "\n",
        encoding="utf-8",
    )
    (plugin_dir / ".app.json").write_text(
        json.dumps({"apps": {"github": {"id": connector_id}}}) + "\n",
        encoding="utf-8",
    )


def _write_codex_config(
    home: Path,
    *,
    connector_id: str,
    plugin_enabled: bool | None = None,
    default_apps_enabled: bool | None = None,
) -> None:
    config_dir = home / ".codex"
    config_dir.mkdir(parents=True, exist_ok=True)
    lines: list[str] = []
    if plugin_enabled is not None:
        lines.extend(
            [
                '[plugins."github@openai-curated"]',
                f"enabled = {'true' if plugin_enabled else 'false'}",
                "",
            ]
        )
    if default_apps_enabled is not None:
        lines.extend(
            [
                "[apps._default]",
                f"enabled = {'true' if default_apps_enabled else 'false'}",
                "",
            ]
        )
    lines.extend(
        [
            f"[apps.{connector_id}]",
            "enabled = true",
            "default_tools_enabled = true",
            "",
            f"[apps.{connector_id}.tools.github_fetch_issue]",
            'approval_mode = "approve"',
            "",
        ]
    )
    (config_dir / "config.toml").write_text("\n".join(lines), encoding="utf-8")


def run_smoke(plugin_root: Path, *, home: Path | None = None) -> dict[str, Any]:
    def _run(target_home: Path) -> dict[str, Any]:
        result: dict[str, Any] = {
            "home": str(target_home),
            "plugin_root": str(plugin_root),
        }
        result["install"] = install(target_home, plugin_root)
        result["status_after_install"] = get_status(target_home)
        result["missing_official_plugin"] = validate_github_surface(plugin_root, home=target_home)

        _write_official_github_plugin(
            target_home,
            connector_id=EXPECTED_GITHUB_CONNECTOR_ID,
        )
        _write_codex_config(
            target_home,
            connector_id=EXPECTED_GITHUB_CONNECTOR_ID,
            plugin_enabled=True,
        )
        result["present_surface"] = validate_github_surface(plugin_root, home=target_home)

        _write_codex_config(
            target_home,
            connector_id=EXPECTED_GITHUB_CONNECTOR_ID,
            plugin_enabled=False,
        )
        result["disabled_plugin"] = validate_github_surface(plugin_root, home=target_home)

        remove(target_home)
        result["status_after_remove"] = get_status(target_home)

        if result["missing_official_plugin"] == []:
            raise RuntimeError("Smoke harness expected the missing-plugin preflight to fail.")
        if result["present_surface"] != []:
            raise RuntimeError(
                "Smoke harness expected the present GitHub surface preflight to pass."
            )
        if result["disabled_plugin"] == []:
            raise RuntimeError("Smoke harness expected the disabled-plugin preflight to fail.")
        if result["status_after_remove"]["installed"]:
            raise RuntimeError("Smoke harness expected the home-local install to be removed.")
        return result

    if home is not None:
        return _run(home)
    with tempfile.TemporaryDirectory(prefix="pmw-home-smoke-") as tmp_dir:
        return _run(Path(tmp_dir))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a temp-home smoke test for the project-management-workflows plugin."
    )
    parser.add_argument(
        "--home",
        help="Optional fixed home directory. Defaults to a temporary home.",
    )
    parser.add_argument(
        "--plugin-root",
        default=str(PLUGIN_ROOT),
        help="Plugin root to publish during the smoke test.",
    )
    parser.add_argument("--json-out", help="Optional JSON output path.")
    parser.add_argument("--format", choices=("text", "json"), default="text")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    plugin_root = Path(args.plugin_root).expanduser().resolve()
    home = Path(args.home).expanduser().resolve() if args.home else None
    payload = run_smoke(plugin_root, home=home)
    if args.json_out:
        Path(args.json_out).expanduser().resolve().write_text(
            json.dumps(payload, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    if args.format == "json":
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        print("Home-local plugin smoke passed.")
        print(f"home: {payload['home']}")
        print(f"plugin_root: {payload['plugin_root']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
