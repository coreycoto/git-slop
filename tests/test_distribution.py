from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]


def _load_script(name: str):
    path = REPO_ROOT / "scripts" / name
    spec = importlib.util.spec_from_file_location(name.removesuffix(".py"), path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Unable to load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BUILD_MANIFEST = _load_script("build_release_manifest.py")
UPDATE_FORMULA = _load_script("update_homebrew_formula.py")


class DistributionTests(unittest.TestCase):
    def test_release_manifest_records_artifact_hashes_and_install_commands(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            dist_dir = Path(tmp_dir) / "dist"
            dist_dir.mkdir()
            wheel = dist_dir / "git_slop-0.7.2-py3-none-any.whl"
            sdist = dist_dir / "git_slop-0.7.2.tar.gz"
            wheel.write_bytes(b"wheel")
            sdist.write_bytes(b"sdist")

            manifest = BUILD_MANIFEST.build_manifest(
                project_root=REPO_ROOT,
                dist_dir=dist_dir,
                tag="v0.7.2",
            )

        self.assertEqual(manifest["schema_version"], 1)
        self.assertEqual(manifest["project"], "git-slop")
        self.assertEqual(manifest["tag"], "v0.7.2")
        self.assertEqual(
            manifest["homebrew_source"]["url"],
            "ssh://git@github.com/coreycoto/git-slop.git",
        )
        self.assertEqual(manifest["homebrew_source"]["tag"], "v0.7.2")
        self.assertTrue(manifest["homebrew_source"]["revision"])
        self.assertEqual(manifest["wheel"]["name"], "git_slop-0.7.2-py3-none-any.whl")
        self.assertTrue(manifest["wheel"]["sha256"])
        self.assertIn("uv_release_wheel", manifest["install"])
        self.assertIn("homebrew_private_tap", manifest["install"])
        self.assertNotIn(
            "HOMEBREW_GITHUB_API_TOKEN",
            "\n".join(manifest["install"]["homebrew_private_tap"]),
        )
        self.assertEqual(
            {artifact["name"] for artifact in manifest["artifacts"]},
            {wheel.name, sdist.name},
        )

    def test_homebrew_formula_renders_pinned_private_git_source(self) -> None:
        manifest = {
            "homebrew_source": {
                "url": "ssh://git@github.com/coreycoto/git-slop.git",
                "tag": "v0.7.2",
                "revision": "405cc8928c3adf891a75e17ed438aa2c4b2dbcd2",
            },
            "version": "0.7.2",
            "wheel": {
                "url": "https://github.com/coreycoto/git-slop/releases/download/v0.7.2/git_slop-0.7.2-py3-none-any.whl",
                "sha256": "0" * 64,
            }
        }

        formula = UPDATE_FORMULA.render_formula(manifest)

        self.assertIn("class GitSlop < Formula", formula)
        self.assertNotIn("HOMEBREW_GITHUB_API_TOKEN", formula)
        self.assertIn('url "ssh://git@github.com/coreycoto/git-slop.git"', formula)
        self.assertIn('tag:      "v0.7.2"', formula)
        self.assertIn('revision: "405cc8928c3adf891a75e17ed438aa2c4b2dbcd2"', formula)
        self.assertIn('version "0.7.2"', formula)
        self.assertIn('include Language::Python::Virtualenv', formula)
        self.assertIn('depends_on "python@3.13"', formula)
        self.assertIn('depends_on "rust" => :build', formula)
        self.assertIn('resource "tiktoken"', formula)
        self.assertIn('virtualenv_install_with_resources using: "python3.13"', formula)
        self.assertIn('assert_match "git-slop"', formula)

    def test_homebrew_formula_requires_wheel_payload(self) -> None:
        with self.assertRaises(ValueError):
            UPDATE_FORMULA.render_formula({"wheel": {"url": None, "sha256": None}})


if __name__ == "__main__":
    unittest.main()
