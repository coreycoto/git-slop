from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest import mock

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
RELEASE_PREPARE = _load_script("release_prepare.py")


class DistributionTests(unittest.TestCase):
    def test_release_manifest_records_artifact_hashes_and_install_commands(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            dist_dir = Path(tmp_dir) / "dist"
            dist_dir.mkdir()
            wheel = dist_dir / "git_slop-0.8.0-py3-none-any.whl"
            sdist = dist_dir / "git_slop-0.8.0.tar.gz"
            wheel.write_bytes(b"wheel")
            sdist.write_bytes(b"sdist")

            with mock.patch.object(BUILD_MANIFEST, "_git_revision", return_value="a" * 40):
                manifest = BUILD_MANIFEST.build_manifest(
                    project_root=REPO_ROOT,
                    dist_dir=dist_dir,
                    tag="v0.8.0",
                )

        self.assertEqual(manifest["schema_version"], 1)
        self.assertEqual(manifest["project"], "git-slop")
        self.assertEqual(manifest["tag"], "v0.8.0")
        self.assertEqual(
            manifest["homebrew_source"]["url"],
            "ssh://git@github.com/coreycoto/git-slop.git",
        )
        self.assertEqual(manifest["homebrew_source"]["tag"], "v0.8.0")
        self.assertEqual(manifest["homebrew_source"]["revision"], "a" * 40)
        self.assertEqual(manifest["wheel"]["name"], "git_slop-0.8.0-py3-none-any.whl")
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
                "tag": "v0.8.0",
                "revision": "b" * 40,
            },
            "version": "0.8.0",
            "wheel": {
                "url": "https://github.com/coreycoto/git-slop/releases/download/v0.8.0/git_slop-0.8.0-py3-none-any.whl",
                "sha256": "0" * 64,
            }
        }

        formula = UPDATE_FORMULA.render_formula(manifest)

        self.assertIn("class GitSlop < Formula", formula)
        self.assertNotIn("HOMEBREW_GITHUB_API_TOKEN", formula)
        self.assertIn('url "ssh://git@github.com/coreycoto/git-slop.git"', formula)
        self.assertIn('tag:      "v0.8.0"', formula)
        self.assertIn(f'revision: "{"b" * 40}"', formula)
        self.assertIn('version "0.8.0"', formula)
        self.assertIn('include Language::Python::Virtualenv', formula)
        self.assertIn('depends_on "python@3.13"', formula)
        self.assertIn('depends_on "rust" => :build', formula)
        self.assertIn('resource "tiktoken"', formula)
        self.assertIn('virtualenv_install_with_resources using: "python3.13"', formula)
        self.assertIn('man1.install "man/git-slop.1"', formula)
        self.assertNotIn("(man1/\"git-slop.1\").write", formula)
        self.assertNotIn(".TH GIT-SLOP 1", formula)
        self.assertIn('assert_match "git-slop"', formula)

    def test_homebrew_formula_requires_wheel_payload(self) -> None:
        with self.assertRaises(ValueError):
            UPDATE_FORMULA.render_formula({"wheel": {"url": None, "sha256": None}})

    def test_release_prepare_requires_matching_project_version(self) -> None:
        with self.assertRaisesRegex(ValueError, "pyproject.toml version"):
            RELEASE_PREPARE.validate_project_version(REPO_ROOT, "9.9.9")

    def test_release_prepare_runs_mechanical_steps(self) -> None:
        calls: list[tuple[str, ...]] = []

        def runner(command: list[str], cwd: Path) -> None:
            calls.append(tuple(command))

        with mock.patch.object(RELEASE_PREPARE, "tag_revision", return_value="c" * 40):
            messages = RELEASE_PREPARE.prepare_release(
                version="0.8.0",
                tap=Path("../homebrew-tap"),
                project_root=REPO_ROOT,
                runner=runner,
            )

        self.assertIn(("uv", "build"), calls)
        self.assertIn(
            (
                "uv",
                "run",
                "python",
                "scripts/build_release_manifest.py",
                "--dist-dir",
                "dist",
                "--output",
                ".artifacts/releases/release-manifest.json",
                "--tag",
                "v0.8.0",
            ),
            calls,
        )
        self.assertIn("git push origin v0.8.0", "\n".join(messages))


if __name__ == "__main__":
    unittest.main()
