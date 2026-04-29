#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any
from urllib.parse import quote

PROJECT_ROOT = Path(__file__).resolve().parents[1]

RESOURCES = [
    (
        "certifi",
        "https://files.pythonhosted.org/packages/af/2d/7bf41579a8986e348fa033a31cdd0e4121114f6bce2457e8876010b092dd/certifi-2026.2.25.tar.gz",
        "e887ab5cee78ea814d3472169153c2d12cd43b14bd03329a39a9c6e2e80bfba7",
    ),
    (
        "charset-normalizer",
        "https://files.pythonhosted.org/packages/e7/a1/67fe25fac3c7642725500a3f6cfe5821ad557c3abb11c9d20d12c7008d3e/charset_normalizer-3.4.7.tar.gz",
        "ae89db9e5f98a11a4bf50407d4363e7b09b31e55bc117b4f7d80aab97ba009e5",
    ),
    (
        "idna",
        "https://files.pythonhosted.org/packages/22/12/2948fbe5513d062169bd91f7d7b1cd97bc8894f32946b71fa39f6e63ca0c/idna-3.12.tar.gz",
        "724e9952cc9e2bd7550ea784adb098d837ab5267ef67a1ab9cf7846bdbdd8254",
    ),
    (
        "urllib3",
        "https://files.pythonhosted.org/packages/c7/24/5f1b3bdffd70275f6661c76461e25f024d5a38a46f04aaca912426a2b1d3/urllib3-2.6.3.tar.gz",
        "1b62b6884944a57dbe321509ab94fd4d3b307075e0c2eae991ac71ee15ad38ed",
    ),
    (
        "requests",
        "https://files.pythonhosted.org/packages/5f/a4/98b9c7c6428a668bf7e42ebb7c79d576a1c3c1e3ae2d47e674b468388871/requests-2.33.1.tar.gz",
        "18817f8c57c6263968bc123d237e3b8b08ac046f5456bd1e307ee8f4250d3517",
    ),
    (
        "regex",
        "https://files.pythonhosted.org/packages/cb/0e/3a246dbf05666918bd3664d9d787f84a9108f6f43cc953a077e4a7dfdb7e/regex-2026.4.4.tar.gz",
        "e08270659717f6973523ce3afbafa53515c4dc5dcad637dc215b6fd50f689423",
    ),
    (
        "pyyaml",
        "https://files.pythonhosted.org/packages/05/8e/961c0007c59b8dd7729d542c61a4d537767a59645b82a0b521206e1e25c2/pyyaml-6.0.3.tar.gz",
        "d76623373421df22fb4cf8817020cbb7ef15c725b9d5e45f17e189bfc384190f",
    ),
    (
        "tiktoken",
        "https://files.pythonhosted.org/packages/7d/ab/4d017d0f76ec3171d469d80fc03dfbb4e48a4bcaddaa831b31d526f05edc/tiktoken-0.12.0.tar.gz",
        "b18ba7ee2b093863978fcb14f74b3707cdc8d4d4d3836853ce7ec60772139931",
    ),
]


def render_formula(manifest: dict[str, Any]) -> str:
    wheel = manifest["wheel"]
    if not wheel.get("url") or not wheel.get("sha256"):
        raise ValueError("release manifest must include wheel.url and wheel.sha256")
    source = _source_artifact(manifest)
    release_tag = manifest.get("tag") or _release_tag_from_wheel_url(wheel["url"])
    version = manifest.get("version") or release_tag.removeprefix("v")
    asset_name = source["name"]
    release_api_url = (
        f"https://api.github.com/repos/coreycoto/git-slop/releases/tags/{release_tag}"
        f"?asset={quote(asset_name)}"
    )
    resource_blocks = "\n\n".join(
        "\n".join(
            [
                f'  resource "{name}" do',
                f'    url "{url}"',
                f'    sha256 "{sha256}"',
                "  end",
            ]
        )
        for name, url, sha256 in RESOURCES
    )
    return (
        "require \"json\"\n"
        "require \"net/http\"\n"
        "require \"uri\"\n\n"
        "class GitSlopPrivateReleaseDownloadStrategy < CurlDownloadStrategy\n"
        "  def initialize(url, name, version, **meta)\n"
        '    @asset_name = meta.delete(:asset_name)\n'
        '    @github_token = ENV["HOMEBREW_GITHUB_API_TOKEN"] || ENV["GITHUB_TOKEN"] || '
        'ENV["GH_TOKEN"]\n'
        "    if @github_token.blank?\n"
        '      odie "Set HOMEBREW_GITHUB_API_TOKEN, GITHUB_TOKEN, or GH_TOKEN with access '
        'to coreycoto/git-slop."\n'
        "    end\n\n"
        "    meta[:headers] ||= []\n"
        '    meta[:headers] << "Authorization: Bearer #{@github_token}"\n'
        '    meta[:headers] << "Accept: application/octet-stream"\n'
        "    super\n"
        "  end\n"
        "\n"
        "  private\n"
        "\n"
        "  def resolve_url_basename_time_file_size(url, timeout: nil)\n"
        "    super(resolve_asset_api_url(url), timeout: timeout)\n"
        "  end\n"
        "\n"
        "  def _fetch(url:, resolved_url:, timeout:)\n"
        "    super(url: resolve_asset_api_url(url), resolved_url: resolved_url, timeout: timeout)\n"
        "  end\n"
        "\n"
        "  def resolve_asset_api_url(release_api_url)\n"
        '    return release_api_url if release_api_url.include?("/releases/assets/")\n'
        "\n"
        "    @resolve_asset_api_url ||= begin\n"
        "      uri = URI(release_api_url)\n"
        "      uri.query = nil\n"
        "      request = Net::HTTP::Get.new(uri)\n"
        '      request["Authorization"] = "Bearer #{@github_token}"\n'
        '      request["Accept"] = "application/vnd.github+json"\n'
        "      response = Net::HTTP.start(uri.hostname, uri.port, use_ssl: true) do |http|\n"
        "        http.request(request)\n"
        "      end\n"
        "      unless response.is_a?(Net::HTTPSuccess)\n"
        '        odie "Unable to read git-slop release metadata from #{release_api_url}: '
        'HTTP #{response.code}"\n'
        "      end\n"
        "      asset = JSON.parse(response.body).fetch(\"assets\").find do |candidate|\n"
        "        candidate.fetch(\"name\") == @asset_name\n"
        "      end\n"
        '      odie "Release asset #{@asset_name} was not found in #{release_api_url}." '
        "if asset.nil?\n"
        "      asset.fetch(\"url\")\n"
        "    end\n"
        "  end\n"
        "end\n\n"
        'class GitSlop < Formula\n'
        '  include Language::Python::Virtualenv\n\n'
        '  desc "Local-first hotspot detection for AI-era repositories"\n'
        '  homepage "https://github.com/coreycoto/git-slop"\n'
        f'  url "{release_api_url}",\n'
        f'      using:      GitSlopPrivateReleaseDownloadStrategy,\n'
        f'      asset_name: "{asset_name}"\n'
        f'  version "{version}"\n'
        f'  sha256 "{source["sha256"]}"\n'
        '  license "MIT"\n\n'
        '  depends_on "rust" => :build\n\n'
        '  depends_on "libyaml"\n'
        '  depends_on "python@3.13"\n\n'
        f"{resource_blocks}\n\n"
        "  def install\n"
        '    virtualenv_install_with_resources using: "python3.13"\n'
        "  end\n\n"
        "  test do\n"
        '    assert_match "git-slop", shell_output("#{bin}/git-slop version")\n'
        "  end\n"
        "end\n"
    )


def _release_tag_from_wheel_url(url: str) -> str:
    match = re.search(r"/releases/download/([^/]+)/", url)
    if not match:
        raise ValueError("release manifest must include tag or a GitHub release wheel URL")
    return match.group(1)


def _source_artifact(manifest: dict[str, Any]) -> dict[str, str]:
    for artifact in manifest.get("artifacts", []):
        if artifact.get("name", "").endswith(".tar.gz") and artifact.get("sha256"):
            return {"name": artifact["name"], "sha256": artifact["sha256"]}
    raise ValueError("release manifest must include a source distribution artifact")


def main() -> int:
    parser = argparse.ArgumentParser(description="Update the Homebrew formula for git-slop.")
    parser.add_argument(
        "--manifest",
        default=".artifacts/releases/release-manifest.json",
        help="Release manifest generated by build_release_manifest.py.",
    )
    parser.add_argument(
        "--formula",
        default="../homebrew-tap/Formula/git-slop.rb",
        help="Formula path to write.",
    )
    args = parser.parse_args()

    manifest = json.loads((PROJECT_ROOT / args.manifest).read_text(encoding="utf-8"))
    formula_path = (PROJECT_ROOT / args.formula).resolve()
    formula_path.parent.mkdir(parents=True, exist_ok=True)
    formula_path.write_text(render_formula(manifest), encoding="utf-8")
    print(formula_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
