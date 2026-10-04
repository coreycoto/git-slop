#!/usr/bin/env bash
set -euo pipefail

die() {
  printf 'prepare-codex-plugins: error: %s\n' "$*" >&2
  exit 1
}

[[ $# -eq 1 ]] || die "usage: scripts/prepare-codex-plugins.sh CODEX_HOME"
codex_home="$1"
[[ "$codex_home" == /* && "$codex_home" != "/" ]] || die "CODEX_HOME must be an explicit absolute scratch path"
[[ -n "${RUNNER_TEMP-}" && "$RUNNER_TEMP" == /* && -d "$RUNNER_TEMP" ]] || die "an existing absolute RUNNER_TEMP is required"
runner_temp="$(cd -- "$RUNNER_TEMP" && pwd -P)"
case "$codex_home" in
  "${RUNNER_TEMP%/}/"*) relative_home="${codex_home#"${RUNNER_TEMP%/}/"}" ;;
  "${runner_temp%/}/"*) relative_home="${codex_home#"${runner_temp%/}/"}" ;;
  *) die "Codex plugin installation must remain under RUNNER_TEMP" ;;
esac
[[ -n "$relative_home" && "$relative_home" != *$'\n'* && "$relative_home" != *$'\r'* ]] || die "Codex scratch path is unsafe"
case "/$relative_home/" in
  *'/../'*|*'/./'*|*'//'*) die "Codex scratch path must not contain traversal or empty components" ;;
esac
codex_home="$runner_temp/$relative_home"
ancestor="$runner_temp"
IFS='/' read -r -a home_components <<< "$relative_home"
for component in "${home_components[@]}"; do
  ancestor="$ancestor/$component"
  [[ ! -L "$ancestor" ]] || die "Codex scratch path must not traverse a symlink"
  [[ ! -e "$ancestor" || -d "$ancestor" ]] || die "Codex scratch path has a non-directory component"
done

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/.." && pwd -P)"
manifest="$repo_root/.agents/plugins/marketplace-source.json"
gh_steward_lock="$repo_root/.agents/gh-steward.lock.json"
readonly expected_marketplace="agent-plugins"
readonly expected_repository="coreycoto/agent-plugins"
readonly expected_ref="df46d47e1fb0ae29424a93943be2ab55be24400a"
readonly expected_version="0.160.0"
readonly expected_plugins='["project-management","product-development"]'

command -v jq >/dev/null 2>&1 || die "jq is required"
command -v npm >/dev/null 2>&1 || die "npm is required"
[[ -f "$manifest" && ! -L "$manifest" ]] || die "public Agent Plugins source manifest is missing or unsafe"
[[ -f "$gh_steward_lock" && ! -L "$gh_steward_lock" ]] || die "gh-steward source lock is missing or unsafe"
jq -e \
  --arg marketplace "$expected_marketplace" \
  --arg repository "$expected_repository" \
  --arg ref "$expected_ref" \
  --arg version "$expected_version" \
  --argjson plugins "$expected_plugins" '
    type == "object"
    and (keys == ["codex_cli_version", "marketplace_name", "plugins", "ref", "repository", "schema_version"])
    and .schema_version == 1
    and .marketplace_name == $marketplace
    and .repository == $repository
    and .ref == $ref
    and .codex_cli_version == $version
    and .plugins == $plugins
  ' "$manifest" >/dev/null || die "public plugin source must use the reviewed immutable repository, commit and package set"
gh_steward_repo="$(jq -er '.repository' "$gh_steward_lock")" || die "gh-steward source repository is missing"
gh_steward_version="$(jq -er '.version' "$gh_steward_lock")" || die "gh-steward plugin version is missing"
gh_steward_revision="$(jq -er '.source_revision' "$gh_steward_lock")" || die "gh-steward source revision is missing"
jq -e '
  type == "object"
  and (keys == ["asset_sha256", "repository", "schema_version", "source_revision", "version"])
  and .schema_version == 1
  and .repository == "coreycoto/gh-steward"
  and .version == "0.1.0"
  and (.source_revision | type == "string" and test("^[0-9a-f]{40}$"))
  and (.asset_sha256 | type == "object" and keys == ["darwin/amd64", "darwin/arm64", "linux/amd64", "linux/arm64"] and all(.[]; type == "string" and test("^[0-9a-f]{64}$")))
' "$gh_steward_lock" >/dev/null || die "gh-steward plugin source lock is unqualified or malformed"
[[ "$gh_steward_repo" == "coreycoto/gh-steward" && "$gh_steward_version" == "0.1.0" ]] || die "gh-steward plugin source identity differs from the reviewed tool lock"

require_runner_temp_path() {
  local candidate="$1"
  case "${candidate%/}/" in
    "${runner_temp%/}/"*) ;;
    *) die "Codex CLI scratch installation must remain under RUNNER_TEMP" ;;
  esac
}

install_root="$(mktemp -d "$runner_temp/codex-cli-0.160.0.XXXXXX")"
require_runner_temp_path "$install_root"
mkdir -m 0700 -p -- "$codex_home"
export CODEX_HOME="$codex_home"
npm install --prefix "$install_root" --cache "$install_root/npm-cache" --no-save --no-audit --no-fund --package-lock=false \
  "@openai/codex@${expected_version}" >&2
codex_bin="$install_root/node_modules/.bin/codex"
for package_dir in node_modules node_modules/.bin node_modules/@openai node_modules/@openai/codex node_modules/@openai/codex/bin; do
  [[ -d "$install_root/$package_dir" && ! -L "$install_root/$package_dir" ]] || die "pinned Codex CLI package path is unsafe"
done
package_bin="$install_root/node_modules/@openai/codex/bin/codex.js"
[[ -f "$package_bin" && ! -L "$package_bin" && -x "$package_bin" ]] || die "pinned Codex CLI package entrypoint was not installed"
[[ -L "$codex_bin" && "$(readlink "$codex_bin")" == ../@openai/codex/bin/codex.js ]] || die "Codex executable link must target the pinned package entrypoint"
version_output="$("$codex_bin" --version 2>&1)" || die "pinned Codex CLI version check failed"
[[ "$version_output" =~ (^|[^0-9])0\.160\.0([^0-9]|$) ]] || die "Codex CLI must be exactly version 0.160.0 (found: $version_output)"

"$codex_bin" plugin marketplace add "$expected_repository" \
  --ref "$expected_ref" \
  --sparse .agents/plugins \
  --sparse plugins/project-management \
  --sparse plugins/product-development \
  --json
"$codex_bin" plugin marketplace add "$gh_steward_repo" \
  --ref "$gh_steward_revision" \
  --sparse .agents/plugins \
  --sparse plugins/gh-steward \
  --json
"$codex_bin" plugin add project-management --marketplace "$expected_marketplace" --json
"$codex_bin" plugin add product-development --marketplace "$expected_marketplace" --json
"$codex_bin" plugin add gh-steward --marketplace gh-steward-marketplace --json

if [[ -n "${GITHUB_PATH-}" ]]; then
  printf '%s\n' "$install_root/node_modules/.bin" >> "$GITHUB_PATH"
fi
if [[ -n "${GITHUB_ENV-}" ]]; then
  printf 'CODEX_HOME=%s\n' "$codex_home" >> "$GITHUB_ENV"
fi
printf 'Installed project-management and product-development from %s at %s plus gh-steward %s at %s with Codex CLI %s\n' \
  "$expected_repository" "$expected_ref" "$gh_steward_repo" "$gh_steward_revision" "$expected_version"
