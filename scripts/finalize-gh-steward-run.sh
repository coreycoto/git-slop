#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/.." && pwd -P)"
if [[ -n "${GH_STEWARD_TRUSTED_ROOT-}" ]]; then
  repo_root="$(cd -- "$GH_STEWARD_TRUSTED_ROOT" && pwd -P)"
  [[ "$(git -C "$repo_root" rev-parse HEAD)" == "${GITHUB_WORKFLOW_SHA:?trusted workflow SHA is required}" ]] || {
    printf 'finalize-gh-steward-run: trusted control checkout source differs\n' >&2
    exit 1
  }
fi
args=()
repository=""
while [[ $# -gt 0 ]]; do
  [[ $# -ge 2 ]] || { printf 'finalize-gh-steward-run: every option requires a value\n' >&2; exit 2; }
  case "$1" in
    --repository) repository="$2" ;;
    --package) args+=(--package-root "$2") ;;
    --workflow|--run-id|--attempt|--checkpoint|--artifact-id|--artifact-digest|--recovery-source|--publication-proof)
      args+=("$1" "$2") ;;
    *) printf 'finalize-gh-steward-run: unsupported option %s\n' "$1" >&2; exit 2 ;;
  esac
  shift 2
done
[[ -n "$repository" ]] || { printf 'finalize-gh-steward-run: repository is required\n' >&2; exit 2; }
"$repo_root/scripts/with-gh-steward.sh" --verify >&2
exec "${GH_STEWARD_BIN:?verified gh-steward binary is required}" runs finalize \
  --repo-root "$repo_root" --repo "${GITHUB_SERVER_URL:?GitHub server URL is required}/$repository" \
  --runner-temp "${RUNNER_TEMP:?runner scratch is required}" \
  --policy .agents/gh-steward-recovery-policy.json \
  --workflow-sha "${GITHUB_WORKFLOW_SHA:?trusted workflow SHA is required}" \
  --github-output "${GITHUB_OUTPUT:?workflow output file is required}" "${args[@]}"
