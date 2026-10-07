#!/usr/bin/env bash
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT
repo="$scratch/trusted checkout"
mkdir -p "$repo/scripts"
cp "$script_dir/recover-gh-steward-run.sh" "$script_dir/finalize-gh-steward-run.sh" "$repo/scripts/"
cat > "$repo/scripts/with-gh-steward.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "$*" == "--verify" ]]
printf 'verified\n' >> "$VERIFY_LOG"
SH
cat > "$scratch/native tool" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\0' "$@" > "$NATIVE_ARGS"
exit 7
SH
chmod +x "$repo/scripts/with-gh-steward.sh" "$scratch/native tool"
git -C "$repo" init --quiet
git -C "$repo" -c user.name='Test Fixture' -c user.email='fixture@example.invalid' add scripts
git -C "$repo" -c user.name='Test Fixture' -c user.email='fixture@example.invalid' commit --quiet -m 'trusted control fixture'
export GH_STEWARD_BIN="$scratch/native tool" GITHUB_SERVER_URL=https://github.com
GITHUB_WORKFLOW_SHA="$(git -C "$repo" rev-parse HEAD)"
export GITHUB_WORKFLOW_SHA
export RUNNER_TEMP="$scratch" GITHUB_OUTPUT="$scratch/output"
export VERIFY_LOG="$scratch/verify" NATIVE_ARGS="$scratch/args"
touch "$GITHUB_OUTPUT"
# Literal metacharacters probe argument safety.
# shellcheck disable=SC2016
title='A title with spaces and $(touch unexpected)'

set +e
bash "$repo/scripts/recover-gh-steward-run.sh" --recover task.yml example/widgets key "$title" 17 2 "$scratch/package"
status=$?
set -e
[[ "$status" == 7 && "$(wc -l < "$VERIFY_LOG" | tr -d '[:space:]')" == 1 ]]
[[ ! -e unexpected ]]
args=()
while IFS= read -r -d '' arg; do args+=("$arg"); done < "$NATIVE_ARGS"
[[ "${args[0]}" == runs && "${args[1]}" == recover ]]
[[ "${args[3]}" == "$repo" && "${args[5]}" == https://github.com/example/widgets ]]
[[ "${args[11]}" == "$title" && "${args[17]}" == "$scratch/package" ]]
set +e
bash "$repo/scripts/finalize-gh-steward-run.sh" --repository example/widgets --workflow task.yml \
  --run-id 17 --attempt 2 --package "$scratch/package" --checkpoint "$scratch/checkpoint" \
  --artifact-id 9 --artifact-digest "sha256:$(printf '%064d' 0)"
status=$?
set -e
[[ "$status" == 7 && "$(wc -l < "$VERIFY_LOG" | tr -d '[:space:]')" == 2 ]]
args=()
while IFS= read -r -d '' arg; do args+=("$arg"); done < "$NATIVE_ARGS"
[[ "${args[0]}" == runs && "${args[1]}" == finalize && "${args[3]}" == "$repo" ]]
native_arg_value() {
  local expected="$1" index
  for ((index = 0; index + 1 < ${#args[@]}; index++)); do
    if [[ "${args[index]}" == "$expected" ]]; then
      printf '%s' "${args[index + 1]}"
      return 0
    fi
  done
  return 1
}
[[ "$(native_arg_value --workflow-sha)" == "$GITHUB_WORKFLOW_SHA" ]]
[[ "$(native_arg_value --policy)" == .agents/gh-steward-recovery-policy.json ]]
[[ "$(native_arg_value --repo-root)" == "$repo" ]]
cp "$repo/scripts/finalize-gh-steward-run.sh" "$scratch/finalizer copied for source verification"
set +e
GH_STEWARD_TRUSTED_ROOT="$repo" bash "$scratch/finalizer copied for source verification" \
  --repository example/widgets --workflow task.yml --run-id 17 --attempt 2 \
  --package "$scratch/package" --checkpoint "$scratch/checkpoint" \
  --artifact-id 9 --artifact-digest "sha256:$(printf '%064d' 0)"
status=$?
set -e
[[ "$status" == 7 && "$(wc -l < "$VERIFY_LOG" | tr -d '[:space:]')" == 3 ]]
set +e
GH_STEWARD_TRUSTED_ROOT="$repo" GITHUB_WORKFLOW_SHA="$(printf '%040d' 0)" \
  bash "$scratch/finalizer copied for source verification" \
  --repository example/widgets --workflow task.yml --run-id 17 --attempt 2 \
  --package "$scratch/package" --checkpoint "$scratch/checkpoint" \
  --artifact-id 9 --artifact-digest "sha256:$(printf '%064d' 0)"
status=$?
set -e
[[ "$status" == 1 && "$(wc -l < "$VERIFY_LOG" | tr -d '[:space:]')" == 3 ]]
printf 'native recovery adapters preserved acquisition order, exact argument boundaries and failure status\n'
