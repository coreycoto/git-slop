from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = (
    REPO_ROOT
    / "plugins"
    / "project-management-workflows"
    / "scripts"
    / "preflight_github_surface.py"
)


def _load_preflight_module():
    spec = importlib.util.spec_from_file_location("preflight_github_surface", SCRIPT_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError("Unable to load preflight_github_surface.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PREFLIGHT = _load_preflight_module()


class GithubSurfacePreflightTests(unittest.TestCase):
    def _write_official_plugin(
        self,
        home: Path,
        *,
        connector_id: str = PREFLIGHT.EXPECTED_GITHUB_CONNECTOR_ID,
    ) -> None:
        plugin_dir = (
            home
            / ".codex"
            / "plugins"
            / "cache"
            / "openai-curated"
            / "github"
            / "test-hash"
        )
        manifest_dir = plugin_dir / ".codex-plugin"
        manifest_dir.mkdir(parents=True, exist_ok=True)
        (manifest_dir / "plugin.json").write_text(
            json.dumps({"name": "github", "version": "0.0.0"}),
            encoding="utf-8",
        )
        (plugin_dir / ".app.json").write_text(
            json.dumps({"apps": {"github": {"id": connector_id}}}),
            encoding="utf-8",
        )

    def _write_plugin_app(
        self,
        plugin_root: Path,
        *,
        connector_id: str = PREFLIGHT.EXPECTED_GITHUB_CONNECTOR_ID,
    ) -> None:
        plugin_root.mkdir(parents=True, exist_ok=True)
        (plugin_root / ".app.json").write_text(
            json.dumps({"apps": {"github": {"id": connector_id}}}),
            encoding="utf-8",
        )

    def _write_config(
        self,
        home: Path,
        *,
        plugin_enabled: bool | None = None,
        connector_id: str | None = None,
        connector_enabled: bool | None = None,
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
        if connector_id is not None:
            if connector_enabled is not None:
                lines.extend(
                    [
                        f"[apps.{connector_id}]",
                        f"enabled = {'true' if connector_enabled else 'false'}",
                        "",
                    ]
                )
            lines.extend(
                [
                    f"[apps.{connector_id}.tools.github_fetch_issue]",
                    'approval_mode = "approve"',
                    "",
                ]
            )

        (config_dir / "config.toml").write_text("\n".join(lines), encoding="utf-8")

    def test_passes_when_plugin_and_connector_are_available(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            home = root / "home"
            plugin_root = root / "plugin"
            self._write_official_plugin(home)
            self._write_plugin_app(plugin_root)
            self._write_config(
                home,
                plugin_enabled=True,
                connector_id=PREFLIGHT.EXPECTED_GITHUB_CONNECTOR_ID,
            )

            self.assertEqual(PREFLIGHT.validate_github_surface(plugin_root, home=home), [])

    def test_fails_when_official_github_plugin_is_missing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            home = root / "home"
            plugin_root = root / "plugin"
            self._write_plugin_app(plugin_root)
            self._write_config(home, connector_id=PREFLIGHT.EXPECTED_GITHUB_CONNECTOR_ID)

            errors = PREFLIGHT.validate_github_surface(plugin_root, home=home)
            self.assertEqual(len(errors), 1)
            self.assertIn("Official GitHub Codex plugin not detected", errors[0])

    def test_fails_when_official_github_plugin_is_disabled(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            home = root / "home"
            plugin_root = root / "plugin"
            self._write_official_plugin(home)
            self._write_plugin_app(plugin_root)
            self._write_config(
                home,
                plugin_enabled=False,
                connector_id=PREFLIGHT.EXPECTED_GITHUB_CONNECTOR_ID,
            )

            errors = PREFLIGHT.validate_github_surface(plugin_root, home=home)
            self.assertEqual(len(errors), 1)
            self.assertIn("installed but disabled", errors[0])

    def test_fails_when_connector_mapping_is_missing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            home = root / "home"
            plugin_root = root / "plugin"
            self._write_official_plugin(home)
            plugin_root.mkdir(parents=True, exist_ok=True)
            self._write_config(home, connector_id=PREFLIGHT.EXPECTED_GITHUB_CONNECTOR_ID)

            errors = PREFLIGHT.validate_github_surface(plugin_root, home=home)
            self.assertEqual(len(errors), 1)
            self.assertIn("missing .app.json", errors[0])

    def test_fails_when_connector_mapping_is_mismatched(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            home = root / "home"
            plugin_root = root / "plugin"
            self._write_official_plugin(home)
            self._write_plugin_app(plugin_root, connector_id="connector_wrong")
            self._write_config(home, connector_id=PREFLIGHT.EXPECTED_GITHUB_CONNECTOR_ID)

            errors = PREFLIGHT.validate_github_surface(plugin_root, home=home)
            self.assertEqual(len(errors), 1)
            self.assertIn("must map apps.github.id", errors[0])

    def test_fails_when_connector_surface_is_disabled(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            home = root / "home"
            plugin_root = root / "plugin"
            self._write_official_plugin(home)
            self._write_plugin_app(plugin_root)
            self._write_config(
                home,
                connector_id=PREFLIGHT.EXPECTED_GITHUB_CONNECTOR_ID,
                default_apps_enabled=False,
            )

            errors = PREFLIGHT.validate_github_surface(plugin_root, home=home)
            self.assertEqual(len(errors), 1)
            self.assertIn("default app surface is disabled", errors[0])


if __name__ == "__main__":
    unittest.main()
