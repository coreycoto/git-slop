#!/usr/bin/env bash
set -euo pipefail

binary=$(realpath "${1:?usage: validate-packaged-contracts.sh BINARY SCHEMA_DIR WORKTREE}")
schema_dir=$(realpath "${2:?usage: validate-packaged-contracts.sh BINARY SCHEMA_DIR WORKTREE}")
source_worktree=$(realpath "${3:?usage: validate-packaged-contracts.sh BINARY SCHEMA_DIR WORKTREE}")
if [[ "$(git -C "$source_worktree" rev-parse --is-shallow-repository)" == true ]]; then
  echo "packaged-contract validation requires complete history; use actions/checkout with fetch-depth: 0 or run git fetch --unshallow" >&2
  exit 1
fi
contract_root=$(mktemp -d)
trap 'rm -rf "$contract_root"' EXIT
mkdir -p "$contract_root/npm-cache" "$contract_root/validator"
export NPM_CONFIG_CACHE="$contract_root/npm-cache"
export NPM_CONFIG_AUDIT=false
export NPM_CONFIG_FUND=false
export NPM_CONFIG_UPDATE_NOTIFIER=false

package_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cp "$package_root/tools/schema-validator/package.json" "$package_root/tools/schema-validator/package-lock.json" "$contract_root/validator/"
(
  cd "$contract_root/validator"
  npm ci --ignore-scripts --no-audit --no-fund --loglevel=error
)
ajv="$contract_root/validator/node_modules/.bin/ajv"

validate_json() {
  "$ajv" validate --spec=draft2020 --strict=false -c ajv-formats "$@"
}

# Candidate assembly deliberately creates staging files in the workflow checkout.
# Analyze a clean clone of the exact checked-out revision so those release-owned
# artifacts cannot contaminate report completeness or comparison semantics.
worktree="$contract_root/worktree"
git clone --quiet --no-hardlinks --no-tags "$source_worktree" "$worktree"
test -z "$(git -C "$worktree" status --short --untracked-files=all)"

for schema in report config compare explain plan sarif health check doctor build-info release-manifest list show prompt-manifest error find-estimate cache-status cache-prune baseline prune compare-ndjson; do
  "$binary" schema "$schema" > "$contract_root/$schema.schema.json"
done

"$binary" build-info --format json > "$contract_root/build-info.json"
for profile in compact standard full-evidence; do
  output="$contract_root/output-$profile"
  "$binary" --repo "$worktree" find --quiet --no-progress --no-cache \
    --report-profile "$profile" --state-dir "$contract_root/state-$profile" --output-dir "$output"
  report="$output/latest/report.json"
  "$binary" report validate "$report"
  path=$(jq -er '.files[0].path' "$report")
  "$binary" check --report "$report" --format json --evaluate-only > "$contract_root/check-$profile.json"
  "$binary" compare --base "$report" --head "$report" --format json > "$contract_root/compare-$profile.json"
  "$binary" compare --base "$report" --head "$report" --format ndjson > "$contract_root/compare-$profile.ndjson"
  jq -ce . "$contract_root/compare-$profile.ndjson" >/dev/null
  "$binary" health --report "$report" --format json > "$contract_root/health-$profile.json"
  "$binary" show --report "$report" --format json "$path" > "$contract_root/show-$profile.json"
  "$binary" list findings --report "$report" --format json > "$contract_root/list-$profile.json"
  "$binary" explain --report "$report" --path "$path" --format json > "$contract_root/explain-$profile.json"
  "$binary" plan --report "$report" --path "$path" --format json > "$contract_root/plan-$profile.json"
  "$binary" html --report "$report" --output "$contract_root/report-$profile.html"
  "$binary" sarif --report "$report" --output "$contract_root/report-$profile.sarif.json"
  validate_json -s "$schema_dir/report-5.json" -d "$report"
  for contract in check compare health show list explain plan; do
    schema_version=1
    if [[ "$contract" == explain || "$contract" == plan ]]; then
      schema_version=2
    fi
    validate_json \
      -s "$schema_dir/${contract}-${schema_version}.json" \
      -d "$contract_root/$contract-$profile.json"
  done
  validate_json -s "$schema_dir/sarif-1.json" -d "$contract_root/report-$profile.sarif.json"

  if [[ "$profile" == standard ]]; then
    mutations=(
      'report-profile|.analyzer.report_profile = "invented"'
      'profile|.files[0].profile = "invented"'
      'context-band|.files[0].context_band = "invented"'
      'slop-band|.files[0].slop_band = "invented"'
      'analysis-status|.files[0].analysis_status = "invented"'
      'slop-score-high|.files[0].slop_score = 100.001'
      'context-pressure-low|.files[0].context_pressure = -0.001'
      'head-sha|.repo.head_sha = "not-a-sha"'
      'worktree-digest|.repo.worktree_state_digest = "abcd"'
      'content-sha|.files[0].content_sha256 = "abcd"'
      'fingerprint|.files[0].content_fingerprint = "abcd"'
      'scope-digest|.scope.selected_path_digest = "abcd"'
    )
    for mutation in "${mutations[@]}"; do
      invalid=${mutation%%|*}
      expression=${mutation#*|}
      jq "$expression" "$report" > "$contract_root/$invalid.json"
      if "$binary" report validate "$contract_root/$invalid.json" >/dev/null 2>&1; then
        echo "runtime validator accepted negative fixture $invalid" >&2
        exit 1
      fi
      if validate_json -s "$schema_dir/report-5.json" -d "$contract_root/$invalid.json" >/dev/null 2>&1; then
        echo "published schema accepted negative fixture $invalid" >&2
        exit 1
      fi
    done
  fi
done

for profile in compact standard full-evidence; do
  : > "$contract_root/github-output"
  : > "$contract_root/github-env"
  : > "$contract_root/step-summary"
  RUNNER_TEMP="$contract_root/action-$profile" \
  GITHUB_OUTPUT="$contract_root/github-output" \
  GITHUB_ENV="$contract_root/github-env" \
  GITHUB_STEP_SUMMARY="$contract_root/step-summary" \
  GIT_SLOP_BINARY="$binary" \
  GIT_SLOP_WORKING_DIRECTORY="$worktree" \
  GIT_SLOP_REPORT_PROFILE="$profile" \
  GIT_SLOP_COMPRESSION=gzip \
    node "$package_root/action/runner.mjs" analyze
done

validate_json -s "$schema_dir/build-info-2.json" -d "$contract_root/build-info.json"

# The executable and each current published schema must expose the exact same
# immutable bytes. Historical schemas can remain packaged without being
# mistaken for the current contract.
for generated_schema in "$contract_root"/*.schema.json; do
  published_name=$(jq -er '."$id" | split("/") | last' "$generated_schema")
  published_schema="$schema_dir/$published_name"
  test -f "$published_schema"
  if ! cmp -s <(jq -S . "$published_schema") <(jq -S . "$generated_schema"); then
    echo "runtime schema differs from packaged $published_name" >&2
    exit 1
  fi
done
