#!/usr/bin/env bash
set -euo pipefail

program_name="with-gh-steward"
usage() {
  printf 'usage: %s --prepare | --verify\n' "$program_name" >&2
  exit 2
}

die() {
  printf '%s: error: %s\n' "$program_name" "$*" >&2
  exit 1
}

[[ $# -eq 1 ]] || usage
mode="$1"
[[ "$mode" == "--prepare" || "$mode" == "--verify" ]] || usage

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/.." && pwd -P)"
lock_path="$repo_root/.agents/gh-steward.lock.json"
readonly tool_repository="coreycoto/gh-steward"
readonly tool_version_expected="0.6.1"
readonly supported_targets='["darwin/amd64","darwin/arm64","linux/amd64","linux/arm64"]'

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

load_lock() {
  require_command jq
  [[ -f "$lock_path" && ! -L "$lock_path" ]] || die "tool lock is missing or unsafe: $lock_path"
  jq -e \
    --arg repository "$tool_repository" \
    --arg version "$tool_version_expected" \
    --argjson targets "$supported_targets" '
      type == "object"
      and (keys == ["asset_sha256", "repository", "schema_version", "source_revision", "version"])
      and .schema_version == 1
      and .repository == $repository
      and .version == $version
      and (.source_revision | type == "string" and test("^[0-9a-f]{40}$"))
      and (.asset_sha256 | type == "object" and (keys | sort) == ($targets | sort))
      and ([.asset_sha256[]] | all(.[]; type == "string" and test("^[0-9a-f]{64}$")))
    ' "$lock_path" >/dev/null || die "tool lock has unqualified, malformed or unsupported acquisition pins"
  source_revision="$(jq -r '.source_revision' "$lock_path")"
  tool_version="$(jq -r '.version' "$lock_path")"
}

current_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os/$arch" in
    Darwin/x86_64) target="darwin/amd64" ;;
    Darwin/arm64|Darwin/aarch64) target="darwin/arm64" ;;
    Linux/x86_64|Linux/amd64) target="linux/amd64" ;;
    Linux/aarch64|Linux/arm64) target="linux/arm64" ;;
    *) die "gh-steward has no qualified asset for $os/$arch" ;;
  esac
}

require_runner_temp() {
  [[ -n "${RUNNER_TEMP-}" && "$RUNNER_TEMP" == /* && -d "$RUNNER_TEMP" ]] ||
    die "an existing absolute RUNNER_TEMP is required"
  runner_temp="$(cd -- "$RUNNER_TEMP" && pwd -P)"
  [[ "$runner_temp" != "/" ]] || die "RUNNER_TEMP must not be the filesystem root"
}

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    die "sha256sum or shasum is required"
  fi
}

validate_receipt_and_binary() {
  local receipt_path="$runner_temp/gh-steward-acquisition.json"
  [[ -f "$receipt_path" && ! -L "$receipt_path" ]] || die "verified acquisition receipt is missing or unsafe"
  jq -e \
    --arg repository "$tool_repository" \
    --arg version "$tool_version" \
    --arg revision "$source_revision" \
    --arg target "$target" \
    --arg digest "$asset_digest" \
    --arg path "$binary_path" '
      type == "object"
      and .schema_version == 1
      and .tool == "gh-steward"
      and .tool_version == $version
      and .source_revision == $revision
      and .source_dirty == false
      and .target == $target
      and (.asset | type == "string" and length > 0)
      and .asset_sha256 == $digest
      and .path == $path
    ' "$receipt_path" >/dev/null || die "acquisition receipt does not match the pinned source, version, target and asset digest"
  [[ -f "$binary_path" && ! -L "$binary_path" && -x "$binary_path" ]] || die "acquired binary is missing or unsafe"
  [[ "$(sha256_file "$binary_path")" == "$asset_digest" ]] || die "acquired binary changed after its attested receipt was written"
  local version_json
  version_json="$(env -u GH_TOKEN -u GITHUB_TOKEN -u OPENAI_API_KEY "$binary_path" version --json)" || die "acquired binary failed its offline version check"
  jq -e \
    --arg version "$tool_version" \
    --arg revision "$source_revision" \
    --arg target "$target" '
      .schema_version == 2
      and .tool == "gh-steward"
      and .tool_version == $version
      and .source_revision == $revision
      and .source_dirty == false
      and .target == $target
    ' <<< "$version_json" >/dev/null || die "acquired binary version output differs from its verified receipt"
}

require_runner_temp
load_lock
current_target
asset_digest="$(jq -r --arg target "$target" '.asset_sha256[$target]' "$lock_path")"
binary_path=""
source_dir=""
binary_dir=""
cleanup_temporary_acquisition() {
  local status=$?
  trap - EXIT
  if [[ "$status" -ne 0 ]]; then
    [[ -z "$source_dir" || ! -d "$source_dir" ]] || rm -rf -- "$source_dir"
    [[ -z "$binary_dir" || ! -d "$binary_dir" ]] || rm -rf -- "$binary_dir"
  fi
  exit "$status"
}
trap cleanup_temporary_acquisition EXIT

if [[ "$mode" == "--prepare" ]]; then
  require_command git
  for command_name in mktemp mkdir rm; do require_command "$command_name"; done
  readonly source_url="https://github.com/${tool_repository}.git"
  source_dir="$(mktemp -d "$runner_temp/gh-steward-source.XXXXXX")"
  binary_dir="$(mktemp -d "$runner_temp/gh-steward-bin.XXXXXX")"
  acquisition_script="$source_dir/scripts/release/acquire-gh-steward.sh"
  env -u GH_TOKEN -u GITHUB_TOKEN -u OPENAI_API_KEY GIT_TERMINAL_PROMPT=0 GIT_CONFIG_GLOBAL=/dev/null \
    git clone --quiet --filter=blob:none --no-checkout "$source_url" "$source_dir" >&2
  env -u GH_TOKEN -u GITHUB_TOKEN -u OPENAI_API_KEY GIT_TERMINAL_PROMPT=0 GIT_CONFIG_GLOBAL=/dev/null \
    git -C "$source_dir" fetch --quiet --no-tags --depth=1 origin "$source_revision" >&2
  env -u GH_TOKEN -u GITHUB_TOKEN -u OPENAI_API_KEY GIT_TERMINAL_PROMPT=0 GIT_CONFIG_GLOBAL=/dev/null \
    git -C "$source_dir" checkout --quiet --detach "$source_revision" >&2
  resolved_revision="$(env -u GH_TOKEN -u GITHUB_TOKEN -u OPENAI_API_KEY git -C "$source_dir" rev-parse HEAD)"
  [[ "$resolved_revision" == "$source_revision" ]] || die "tool source checkout differs from the pinned revision"
  [[ -f "$acquisition_script" && ! -L "$acquisition_script" ]] || die "pinned tool source has no safe release acquisition helper"
  # GitHub CLI requires the step-scoped job token for attestation reads in Actions.
  receipt="$(env -u GITHUB_TOKEN -u OPENAI_API_KEY bash "$acquisition_script" \
    "$tool_version" "$source_revision" "$binary_dir" "$asset_digest")" || die "attested gh-steward acquisition failed"
  binary_path="$binary_dir/gh-steward"
  printf '%s\n' "$receipt" > "$runner_temp/gh-steward-acquisition.json"
  validate_receipt_and_binary
  if [[ -n "${GITHUB_ENV-}" ]]; then
    {
      printf 'GH_STEWARD_BIN=%s\n' "$binary_path"
      printf 'GH_STEWARD_RECEIPT=%s\n' "$runner_temp/gh-steward-acquisition.json"
    } >> "$GITHUB_ENV"
  fi
  if [[ -n "${GITHUB_OUTPUT-}" ]]; then
    {
      printf 'binary=%s\n' "$binary_path"
      printf 'receipt=%s\n' "$runner_temp/gh-steward-acquisition.json"
      printf 'source_revision=%s\n' "$source_revision"
      printf 'target=%s\n' "$target"
    } >> "$GITHUB_OUTPUT"
  fi
  rm -rf -- "$source_dir"
  source_dir=""
  printf 'gh-steward %s verified from %s for %s\n' "$tool_version" "$source_revision" "$target"
else
  binary_path="${GH_STEWARD_BIN-}"
  [[ -n "$binary_path" ]] || die "GH_STEWARD_BIN is required for offline verification"
  validate_receipt_and_binary
  printf 'gh-steward receipt and staged binary verified\n'
fi
