from __future__ import annotations

import importlib.util
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

    def test_skill_detection_treats_unavailable_wording_as_missing(self) -> None:
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
        self.assertTrue(SMOKE._skill_available_from_output("Completed docs-only audit."))


if __name__ == "__main__":
    unittest.main()
