from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = REPO_ROOT / "scripts" / "smoke_plugin_consumer.py"


def _load_script_module():
    spec = importlib.util.spec_from_file_location("smoke_plugin_consumer", SCRIPT_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError("Unable to load smoke_plugin_consumer.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SMOKE = _load_script_module()


class PluginConsumerSmokeTests(unittest.TestCase):
    def test_metadata_smoke_reports_pinned_marketplace_dependency(self) -> None:
        payload = SMOKE.run_smoke(REPO_ROOT)
        self.assertEqual(payload["marketplace_name"], "agent-plugins-marketplace")
        self.assertEqual(
            payload["source_url"],
            "https://github.com/coreycoto/agent-plugins.git",
        )
        self.assertEqual(payload["required_plugin"], "project-management-workflows")

    def test_skill_detection_requires_available_marker(self) -> None:
        self.assertFalse(
            SMOKE._skill_available_from_output(  # noqa: SLF001
                "The named skill isn't available in this session."
            )
        )
        self.assertFalse(
            SMOKE._skill_available_from_output(  # noqa: SLF001
                "The named skill isn’t available in this session."
            )
        )
        self.assertTrue(SMOKE._skill_available_from_output("codex.skill.injected"))
        self.assertFalse(SMOKE._skill_available_from_output("Completed docs-only audit."))

    def test_marketplace_plugin_source_path_must_stay_inside_marketplace(self) -> None:
        manifest = {"required_plugin": "project-management-workflows"}
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            marketplace_root = root / "marketplace"
            outside_plugin = root / "outside-plugin"
            (outside_plugin / ".codex-plugin").mkdir(parents=True)
            (outside_plugin / ".codex-plugin" / "plugin.json").write_text(
                "{}\n",
                encoding="utf-8",
            )

            marketplace_json = marketplace_root / ".agents" / "plugins" / "marketplace.json"
            marketplace_json.parent.mkdir(parents=True)

            for source_path in (
                str(outside_plugin),
                "../../outside-plugin",
            ):
                with self.subTest(source_path=source_path):
                    marketplace_json.write_text(
                        json.dumps(
                            {
                                "plugins": [
                                    {
                                        "name": "project-management-workflows",
                                        "source": {
                                            "source": "local",
                                            "path": source_path,
                                        },
                                    }
                                ]
                            }
                        ),
                        encoding="utf-8",
                    )
                    with self.assertRaises(RuntimeError):
                        SMOKE.BOOTSTRAP._plugin_source_path(  # noqa: SLF001
                            marketplace_root,
                            manifest,
                        )

    def test_marketplace_plugin_source_path_accepts_in_root_plugin(self) -> None:
        manifest = {"required_plugin": "project-management-workflows"}
        with tempfile.TemporaryDirectory() as tmp_dir:
            marketplace_root = Path(tmp_dir) / "marketplace"
            plugin_root = marketplace_root / "plugins" / "project-management-workflows"
            (plugin_root / ".codex-plugin").mkdir(parents=True)
            (plugin_root / ".codex-plugin" / "plugin.json").write_text(
                "{}\n",
                encoding="utf-8",
            )
            marketplace_json = marketplace_root / ".agents" / "plugins" / "marketplace.json"
            marketplace_json.parent.mkdir(parents=True)
            marketplace_json.write_text(
                json.dumps(
                    {
                        "plugins": [
                            {
                                "name": "project-management-workflows",
                                "source": {
                                    "source": "local",
                                    "path": "./plugins/project-management-workflows",
                                },
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )

            self.assertEqual(
                SMOKE.BOOTSTRAP._plugin_source_path(  # noqa: SLF001
                    marketplace_root,
                    manifest,
                ),
                plugin_root.resolve(),
            )


if __name__ == "__main__":
    unittest.main()
