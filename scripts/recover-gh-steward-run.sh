#!/usr/bin/env bash
set -euo pipefail

[[ $# -eq 8 && "$1" == "--recover" ]] || {
  printf 'usage: recover-gh-steward-run --recover WORKFLOW REPOSITORY KEY RUN_NAME RUN_ID ATTEMPT DESTINATION\n' >&2
  exit 2
}
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/.." && pwd -P)"
if [[ -n "${GH_STEWARD_TRUSTED_ROOT-}" ]]; then
  repo_root="$(cd -- "$GH_STEWARD_TRUSTED_ROOT" && pwd -P)"
  [[ "$(git -C "$repo_root" rev-parse HEAD)" == "${GITHUB_WORKFLOW_SHA:?trusted workflow SHA is required}" ]] || {
    printf 'recover-gh-steward-run: trusted control checkout source differs\n' >&2
    exit 1
  }
fi
"$repo_root/scripts/with-gh-steward.sh" --verify >&2
exec "${GH_STEWARD_BIN:?verified gh-steward binary is required}" runs recover \
  --repo-root "$repo_root" --repo "${GITHUB_SERVER_URL:?GitHub server URL is required}/$3" \
  --workflow "$2" --recovery-key "$4" --run-name "$5" --run-id "$6" --attempt "$7" \
  --package-root "$8" --runner-temp "${RUNNER_TEMP:?runner scratch is required}" \
  --policy .agents/gh-steward-recovery-policy.json \
  --github-output "${GITHUB_OUTPUT:?workflow output file is required}"
