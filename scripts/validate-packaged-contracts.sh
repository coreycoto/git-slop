#!/usr/bin/env bash
set -euo pipefail

binary=$(realpath "${1:?usage: validate-packaged-contracts.sh BINARY SCHEMA_DIR WORKTREE}")
schema_dir=$(realpath "${2:?usage: validate-packaged-contracts.sh BINARY SCHEMA_DIR WORKTREE}")
worktree=$(realpath "${3:?usage: validate-packaged-contracts.sh BINARY SCHEMA_DIR WORKTREE}")
contract_root=$(mktemp -d)
trap 'rm -rf "$contract_root"' EXIT
export npm_config_cache="$contract_root/npm-cache"

for schema in report config compare explain plan sarif health check doctor build-info list show prompt-manifest error find-estimate cache-status cache-prune prune compare-ndjson; do
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
  "$binary" check --report "$report" --format json > "$contract_root/check-$profile.json"
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
  npx --yes ajv-cli@5.0.0 validate --spec=draft2020 --strict=false \
    -s "$schema_dir/report-5.json" -d "$report"
  for contract in check compare health show list explain plan; do
    schema_version=1
    if [[ "$contract" == explain || "$contract" == plan ]]; then
      schema_version=2
    fi
    npx --yes ajv-cli@5.0.0 validate --spec=draft2020 --strict=false \
      -s "$schema_dir/${contract}-${schema_version}.json" \
      -d "$contract_root/$contract-$profile.json"
  done
  npx --yes ajv-cli@5.0.0 validate --spec=draft2020 --strict=false \
    -s "$schema_dir/sarif-1.json" -d "$contract_root/report-$profile.sarif.json"
done

package_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
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

npx --yes ajv-cli@5.0.0 validate --spec=draft2020 --strict=false \
  -s "$schema_dir/build-info-1.json" -d "$contract_root/build-info.json"

# The executable and packaged schemas must expose the exact same immutable bytes.
for schema_file in "$schema_dir"/*.json; do
  name=$(basename "$schema_file")
  contract=${name%-1.json}
  contract=${contract%-2.json}
  contract=${contract%-5.json}
  case "$name" in
    report-4.json) continue ;;
  esac
  cmp <(jq -S . "$schema_file") <(jq -S . "$contract_root/$contract.schema.json")
done
