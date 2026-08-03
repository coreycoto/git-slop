from __future__ import annotations

import hashlib
import importlib.util
import inspect
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


def _write_release_archives(dist_dir: Path, tag: str = "v0.9.0") -> dict[str, bytes]:
    payloads: dict[str, bytes] = {}
    for target, metadata in BUILD_MANIFEST.TARGETS.items():
        name = f"git-slop-{tag}-{target}.{metadata['archive']}"
        payload = f"{target}\n".encode()
        (dist_dir / name).write_bytes(payload)
        payloads[name] = payload
    return payloads


def _release_manifest(revision: str = "b" * 40) -> dict[str, object]:
    return {
        "schema_version": 2,
        "project": "git-slop",
        "repository": "coreycoto/git-slop",
        "version": "0.9.0",
        "tag": "v0.9.0",
        "revision": revision,
        "homebrew_source": {
            "url": "https://github.com/coreycoto/git-slop.git",
            "tag": "v0.9.0",
            "revision": revision,
        },
    }


class DistributionTests(unittest.TestCase):
    def test_release_manifest_records_cross_platform_artifacts_and_checksums(self) -> None:
        expected_targets = {
            "x86_64-unknown-linux-gnu": {
                "os": "linux",
                "arch": "x86_64",
                "archive": "tar.gz",
            },
            "aarch64-unknown-linux-gnu": {
                "os": "linux",
                "arch": "aarch64",
                "archive": "tar.gz",
            },
            "aarch64-apple-darwin": {
                "os": "macos",
                "arch": "aarch64",
                "archive": "tar.gz",
            },
            "x86_64-pc-windows-msvc": {
                "os": "windows",
                "arch": "x86_64",
                "archive": "zip",
            },
            "aarch64-pc-windows-msvc": {
                "os": "windows",
                "arch": "aarch64",
                "archive": "zip",
            },
        }
        self.assertEqual(BUILD_MANIFEST.TARGETS, expected_targets)

        with tempfile.TemporaryDirectory() as tmp_dir:
            dist_dir = Path(tmp_dir) / "dist"
            dist_dir.mkdir()
            payloads = _write_release_archives(dist_dir)

            with mock.patch.object(BUILD_MANIFEST, "_git_revision", return_value="a" * 40):
                manifest = BUILD_MANIFEST.build_manifest(
                    project_root=REPO_ROOT,
                    dist_dir=dist_dir,
                    tag="v0.9.0",
                )

        self.assertEqual(manifest["schema_version"], 2)
        self.assertEqual(manifest["project"], "git-slop")
        self.assertEqual(manifest["version"], "0.9.0")
        self.assertEqual(manifest["tag"], "v0.9.0")
        self.assertEqual(manifest["revision"], "a" * 40)
        self.assertEqual(
            {artifact["target"] for artifact in manifest["artifacts"]},
            set(BUILD_MANIFEST.TARGETS),
        )
        for artifact in manifest["artifacts"]:
            name = artifact["name"]
            self.assertEqual(
                artifact["sha256"],
                hashlib.sha256(payloads[name]).hexdigest(),
            )
            self.assertEqual(artifact["size_bytes"], len(payloads[name]))
            self.assertEqual(
                artifact["url"],
                f"https://github.com/coreycoto/git-slop/releases/download/v0.9.0/{name}",
            )

        checksums = BUILD_MANIFEST.checksum_lines(manifest["artifacts"])
        self.assertEqual(checksums, "".join(sorted(checksums.splitlines(keepends=True))))
        for name, payload in payloads.items():
            self.assertIn(f"{hashlib.sha256(payload).hexdigest()}  {name}\n", checksums)

        self.assertEqual(manifest["checksums"]["name"], "SHA256SUMS")
        self.assertEqual(
            manifest["homebrew_source"],
            {
                "url": "https://github.com/coreycoto/git-slop.git",
                "tag": "v0.9.0",
                "revision": "a" * 40,
            },
        )
        self.assertIn("homebrew_tap", manifest["install"])
        self.assertIn("github_release", manifest["install"])

    def test_release_manifest_has_no_partial_target_escape_hatch(self) -> None:
        self.assertNotIn(
            "required_targets",
            inspect.signature(BUILD_MANIFEST.build_manifest).parameters,
        )

        with tempfile.TemporaryDirectory() as tmp_dir:
            dist_dir = Path(tmp_dir)
            _write_release_archives(dist_dir)
            extra = dist_dir / "git-slop-v0.9.0-riscv64gc-unknown-linux-gnu.tar.gz"
            extra.write_bytes(b"unsupported\n")

            with self.assertRaisesRegex(ValueError, "unexpected release artifact"):
                BUILD_MANIFEST.build_manifest(
                    project_root=REPO_ROOT,
                    dist_dir=dist_dir,
                    tag="v0.9.0",
                )

    def test_release_manifest_rejects_missing_or_mismatched_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            dist_dir = Path(tmp_dir)
            _write_release_archives(dist_dir)
            (dist_dir / "git-slop-v0.9.0-aarch64-pc-windows-msvc.zip").unlink()

            with self.assertRaisesRegex(ValueError, "missing required release artifact"):
                BUILD_MANIFEST.build_manifest(
                    project_root=REPO_ROOT,
                    dist_dir=dist_dir,
                    tag="v0.9.0",
                )

            with self.assertRaisesRegex(ValueError, "release tag v0.9.1"):
                BUILD_MANIFEST.build_manifest(
                    project_root=REPO_ROOT,
                    dist_dir=dist_dir,
                    tag="v0.9.1",
                )

    def test_homebrew_formula_renders_native_rust_source_build(self) -> None:
        manifest = _release_manifest()

        formula = UPDATE_FORMULA.render_formula(manifest)

        self.assertIn("class GitSlop < Formula", formula)
        self.assertIn('url "https://github.com/coreycoto/git-slop.git"', formula)
        self.assertIn('tag:      "v0.9.0"', formula)
        self.assertIn(f'revision: "{"b" * 40}"', formula)
        self.assertIn('depends_on "rust" => :build', formula)
        self.assertIn('system "cargo", "install", *std_cargo_args', formula)
        self.assertIn('man1.install "man/git-slop.1"', formula)
        self.assertIn('assert_match "git-slop 0.9.0"', formula)
        self.assertNotIn("Python", formula)
        self.assertNotIn("python@", formula)
        self.assertNotIn("libyaml", formula)
        self.assertNotIn("resource ", formula)
        self.assertNotIn("depends_on arch:", formula)

    def test_homebrew_formula_requires_source_payload(self) -> None:
        with self.assertRaisesRegex(ValueError, "schema_version"):
            UPDATE_FORMULA.render_formula({"version": "0.9.0"})

    def test_homebrew_formula_rejects_manifest_identity_drift(self) -> None:
        invalid_manifests = [
            ({**_release_manifest(), "schema_version": 1}, "schema_version"),
            ({**_release_manifest(), "tag": "v0.9.1"}, "tag must agree"),
            ({**_release_manifest(), "revision": "c" * 40}, "revision must agree"),
            (
                {
                    **_release_manifest(),
                    "homebrew_source": {
                        **_release_manifest()["homebrew_source"],
                        "tag": "v0.9.1",
                    },
                },
                "tag must agree",
            ),
        ]
        for manifest, error in invalid_manifests:
            with self.subTest(error=error):
                with self.assertRaisesRegex(ValueError, error):
                    UPDATE_FORMULA.render_formula(manifest)

    def test_tag_resolution_requires_an_exact_tag_ref(self) -> None:
        expected = [
            "git",
            "rev-parse",
            "--verify",
            "refs/tags/v0.9.0^{commit}",
        ]
        with mock.patch.object(
            BUILD_MANIFEST.subprocess,
            "check_output",
            return_value=f"{'a' * 40}\n",
        ) as build_git:
            self.assertEqual(
                BUILD_MANIFEST._git_revision(REPO_ROOT, "v0.9.0"),
                "a" * 40,
            )
        self.assertEqual(build_git.call_args.args[0], expected)

        with mock.patch.object(
            RELEASE_PREPARE.subprocess,
            "check_output",
            return_value=f"{'b' * 40}\n",
        ) as prepare_git:
            self.assertEqual(
                RELEASE_PREPARE.tag_revision(REPO_ROOT, "v0.9.0"),
                "b" * 40,
            )
        self.assertEqual(prepare_git.call_args.args[0], expected)

        with mock.patch.object(
            UPDATE_FORMULA.subprocess,
            "check_output",
            return_value=f"{'c' * 40}\n",
        ) as formula_git:
            self.assertEqual(UPDATE_FORMULA._git_revision("v0.9.0"), "c" * 40)
        self.assertEqual(formula_git.call_args.args[0], expected)

    def test_release_prepare_requires_matching_cargo_version(self) -> None:
        with self.assertRaisesRegex(ValueError, "Cargo.toml version"):
            RELEASE_PREPARE.validate_project_version(REPO_ROOT, "9.9.9")

    def test_release_prepare_requires_tagged_head(self) -> None:
        with (
            mock.patch.object(RELEASE_PREPARE, "tag_revision", return_value="a" * 40),
            mock.patch.object(RELEASE_PREPARE, "head_revision", return_value="b" * 40),
        ):
            with self.assertRaisesRegex(ValueError, "exact tagged commit"):
                RELEASE_PREPARE.validate_release_state(
                    project_root=REPO_ROOT,
                    version="0.9.0",
                )

    def test_release_prepare_runs_rust_validation_and_formula_steps(self) -> None:
        calls: list[tuple[str, ...]] = []

        def runner(command: list[str], cwd: Path) -> None:
            self.assertEqual(cwd, REPO_ROOT)
            calls.append(tuple(command))

        with (
            mock.patch.object(RELEASE_PREPARE, "tag_revision", return_value="c" * 40),
            mock.patch.object(RELEASE_PREPARE, "head_revision", return_value="c" * 40),
        ):
            messages = RELEASE_PREPARE.prepare_release(
                version="0.9.0",
                tap=Path("../homebrew-tap"),
                project_root=REPO_ROOT,
                runner=runner,
            )

        self.assertIn(("cargo", "fmt", "--all", "--", "--check"), calls)
        self.assertIn(
            (
                "cargo",
                "clippy",
                "--all-targets",
                "--all-features",
                "--locked",
                "--",
                "-D",
                "warnings",
            ),
            calls,
        )
        self.assertIn(("cargo", "test", "--all-targets", "--all-features", "--locked"), calls)
        self.assertIn(("cargo", "package", "--locked"), calls)
        self.assertIn(("cargo", "publish", "--dry-run", "--locked"), calls)
        formula_calls = [call for call in calls if "scripts/update_homebrew_formula.py" in call]
        self.assertEqual(len(formula_calls), 1)
        self.assertIn("v0.9.0", formula_calls[0])
        self.assertIn("git push origin v0.9.0", "\n".join(messages))
        self.assertIn("brew upgrade coreycoto/tap/git-slop", "\n".join(messages))
        self.assertIn("separate clean runner", "\n".join(messages))

    def test_release_workflow_never_clobbers_a_published_release(self) -> None:
        workflow = (REPO_ROOT / ".github" / "workflows" / "release-publish.yml").read_text(
            encoding="utf-8"
        )

        self.assertNotIn("--clobber", workflow)
        self.assertIn("Published release already exists and exactly verifies", workflow)
        self.assertIn("steps.release-state.outputs.published != 'true'", workflow)
        self.assertIn("release-verification/regenerated/release-manifest.json", workflow)
        self.assertIn("Create or refresh draft release assets", workflow)
        self.assertIn("Verify the Action installer against published or draft assets", workflow)
        self.assertIn("node action/install.mjs", workflow)
        self.assertIn('gh release delete-asset "$release_tag" "$asset_name" --yes', workflow)
        self.assertIn('test "$(gh api "$endpoint" --jq \'.draft\')" = "true"', workflow)
        self.assertEqual(workflow.count("os: ubuntu-22.04"), 2)


if __name__ == "__main__":
    unittest.main()
