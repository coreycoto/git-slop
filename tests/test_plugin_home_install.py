from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPTS_ROOT = REPO_ROOT / "plugins" / "project-management-workflows" / "scripts"
if str(SCRIPTS_ROOT) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_ROOT))


def _load_script_module(module_name: str, path: Path):
    spec = importlib.util.spec_from_file_location(module_name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Unable to load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


HOME_INSTALL = _load_script_module(
    "manage_home_local_plugin",
    SCRIPTS_ROOT / "manage_home_local_plugin.py",
)
SMOKE = _load_script_module(
    "smoke_home_install",
    SCRIPTS_ROOT / "smoke_home_install.py",
)


class HomeLocalPluginInstallTests(unittest.TestCase):
    def test_install_status_and_remove_round_trip(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            home = Path(tmp_dir)
            install_payload = HOME_INSTALL.install(home, HOME_INSTALL.PLUGIN_ROOT)
            self.assertTrue(install_payload["installed"])
            self.assertEqual(
                install_payload["marketplace_source_path"],
                f"./{HOME_INSTALL.PLUGIN_NAME}",
            )
            self.assertEqual(
                install_payload["source_path"],
                str(HOME_INSTALL.PLUGIN_ROOT.resolve()),
            )
            self.assertTrue(install_payload["plugin_link_exists"])
            self.assertEqual(
                install_payload["plugin_link_target"],
                str(HOME_INSTALL.PLUGIN_ROOT.resolve()),
            )

            status_payload = HOME_INSTALL.get_status(home)
            self.assertTrue(status_payload["installed"])

            remove_payload = HOME_INSTALL.remove(home)
            self.assertFalse(remove_payload["installed"])
            self.assertFalse((home / ".agents" / "plugins" / "marketplace.json").exists())
            self.assertFalse((home / ".agents" / "plugins" / HOME_INSTALL.PLUGIN_NAME).exists())

    def test_smoke_harness_covers_missing_present_and_disabled_surface(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            home = Path(tmp_dir)
            payload = SMOKE.run_smoke(HOME_INSTALL.PLUGIN_ROOT, home=home)
            self.assertTrue(payload["status_after_install"]["installed"])
            self.assertFalse(payload["status_after_remove"]["installed"])
            self.assertNotEqual(payload["missing_official_plugin"], [])
            self.assertEqual(payload["present_surface"], [])
            self.assertNotEqual(payload["disabled_plugin"], [])


if __name__ == "__main__":
    unittest.main()
