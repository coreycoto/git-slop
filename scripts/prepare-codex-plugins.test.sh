#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/prepare-codex-plugins-test.XXXXXX")"
trap 'rm -rf -- "$test_root"' EXIT
command -v jq >/dev/null 2>&1 || { echo "jq is required" >&2; exit 1; }

fixture="$test_root/fixture"
mkdir -p "$fixture/scripts" "$fixture/.agents/plugins"
cp "$repo_root/scripts/prepare-codex-plugins.sh" "$fixture/scripts/prepare-codex-plugins.sh"
chmod 0755 "$fixture/scripts/prepare-codex-plugins.sh"
cp "$repo_root/.agents/plugins/marketplace-source.json" "$fixture/.agents/plugins/marketplace-source.json"
jq -n \
  --arg revision "$(printf 'b%.0s' {1..40})" \
  --arg digest "$(printf 'a%.0s' {1..64})" \
  '{schema_version:1,repository:"coreycoto/gh-steward",version:"0.2.0",source_revision:$revision,asset_sha256:{"darwin/amd64":$digest,"darwin/arm64":$digest,"linux/amd64":$digest,"linux/arm64":$digest}}' \
  > "$fixture/.agents/gh-steward.lock.json"

fake_bin="$test_root/fake-bin"
mkdir -m 0700 "$fake_bin"
cat > "$fake_bin/npm" <<'NPM'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$FAKE_NPM_LOG"
[[ "${1-}" == install ]]
prefix=""
for ((index=1; index<=$#; index++)); do
  if [[ "${!index}" == --prefix ]]; then next=$((index+1)); prefix="${!next}"; fi
done
[[ -n "$prefix" && "${@: -1}" == @openai/codex@0.160.0 ]]
mkdir -p "$prefix/node_modules/.bin" "$prefix/node_modules/@openai/codex/bin"
cat > "$prefix/node_modules/@openai/codex/bin/codex.js" <<'CODEX'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$FAKE_CODEX_LOG"
[[ "${CODEX_HOME-}" == "$FAKE_CODEX_HOME" ]]
case "${1-}" in
  --version) echo 'codex-cli 0.160.0' ;;
  plugin)
    case "${2-}" in
      marketplace)
        [[ "${3-}" == add ]]
        repository="${4-}"
        shift 4
        [[ "${1-}" == --ref ]]
        if [[ "$repository" == coreycoto/agent-plugins ]]; then
          [[ "${2-}" == df46d47e1fb0ae29424a93943be2ab55be24400a ]]
        else
          [[ "$repository" == coreycoto/gh-steward ]]
          [[ "${2-}" == "$(printf 'b%.0s' {1..40})" ]]
        fi
        [[ "${*: -1}" == --json ]]
        echo '{"added":true}'
        ;;
      add)
        [[ "${1-}" == plugin && "${2-}" == add ]]
        [[ "${4-}" == --marketplace && "${6-}" == --json ]]
        case "${3-}:${5-}" in
          project-management:agent-plugins|product-development:agent-plugins|gh-steward:gh-steward-marketplace) ;;
          *) exit 3 ;;
        esac
        echo '{"installed":true}'
        ;;
      *) exit 2 ;;
    esac
    ;;
  *) exit 2 ;;
esac
CODEX
chmod 0755 "$prefix/node_modules/@openai/codex/bin/codex.js"
ln -s ../@openai/codex/bin/codex.js "$prefix/node_modules/.bin/codex"
if [[ "${FAKE_UNSAFE_BIN-}" == true ]]; then
  rm "$prefix/node_modules/.bin/codex"
  ln -s /bin/false "$prefix/node_modules/.bin/codex"
fi
NPM
chmod 0755 "$fake_bin/npm"

runner_temp="$test_root/runner-temp"
mkdir -m 0700 "$runner_temp"
runner_temp="$(cd -- "$runner_temp" && pwd -P)"
github_path="$test_root/github-path"
github_env="$test_root/github-env"
codex_home="$runner_temp/codex-runtime/.codex"
npm_log="$test_root/npm.log"
codex_log="$test_root/codex.log"
env_base=(
  "PATH=$fake_bin:$PATH"
  "RUNNER_TEMP=$runner_temp"
  "GITHUB_PATH=$github_path"
  "GITHUB_ENV=$github_env"
  "FAKE_NPM_LOG=$npm_log"
  "FAKE_CODEX_LOG=$codex_log"
  "FAKE_CODEX_HOME=$codex_home"
)

env "${env_base[@]}" bash "$fixture/scripts/prepare-codex-plugins.sh" "$codex_home" >/dev/null
grep -q -- '--prefix ' "$npm_log"
! grep -Eq '(^| )(-g|--global)( |$)' "$npm_log"
grep -q -- '--ref df46d47e1fb0ae29424a93943be2ab55be24400a' "$codex_log"
grep -q -- '--ref '"$(printf 'b%.0s' {1..40})" "$codex_log"
grep -q 'plugin add project-management --marketplace agent-plugins --json' "$codex_log"
grep -q 'plugin add product-development --marketplace agent-plugins --json' "$codex_log"
grep -q 'plugin add gh-steward --marketplace gh-steward-marketplace --json' "$codex_log"
grep -Eq 'codex-cli-0\.160\.0\.[^/]+/node_modules/\.bin' "$github_path"
grep -q "CODEX_HOME=$codex_home" "$github_env"

# A legitimate npm bin link is accepted; a link outside the package is never run.
before_codex_calls="$(wc -l < "$codex_log" | tr -d ' ')"
if env "${env_base[@]}" FAKE_UNSAFE_BIN=true bash "$fixture/scripts/prepare-codex-plugins.sh" "$codex_home" >/dev/null 2>&1; then
  echo "external Codex executable link was accepted" >&2
  exit 1
fi
[[ "$(wc -l < "$codex_log" | tr -d ' ')" == "$before_codex_calls" ]]

# Reject path traversal and symlinked scratch homes before installation or mkdir.
before_calls="$(wc -l < "$npm_log" | tr -d ' ')"
if env "${env_base[@]}" bash "$fixture/scripts/prepare-codex-plugins.sh" "$runner_temp/../escaped/new-home" >/dev/null 2>&1; then
  echo "Codex scratch traversal was accepted" >&2
  exit 1
fi
[[ ! -e "$test_root/escaped" ]]
ln -s "$test_root" "$runner_temp/linked-home"
if env "${env_base[@]}" bash "$fixture/scripts/prepare-codex-plugins.sh" "$runner_temp/linked-home/new-home" >/dev/null 2>&1; then
  echo "symlinked Codex scratch parent was accepted" >&2
  exit 1
fi
[[ ! -e "$test_root/new-home" ]]
[[ "$(wc -l < "$npm_log" | tr -d ' ')" == "$before_calls" ]]

cp "$fixture/.agents/plugins/marketplace-source.json" "$test_root/marketplace.json"
jq '.ref = "main"' "$test_root/marketplace.json" > "$fixture/.agents/plugins/marketplace-source.json"
before_calls="$(wc -l < "$npm_log" | tr -d ' ')"
if env "${env_base[@]}" bash "$fixture/scripts/prepare-codex-plugins.sh" "$codex_home" >/dev/null 2>&1; then
  echo "mutable marketplace ref was accepted" >&2
  exit 1
fi
[[ "$(wc -l < "$npm_log" | tr -d ' ')" == "$before_calls" ]]

if env "${env_base[@]}" bash "$fixture/scripts/prepare-codex-plugins.sh" "$test_root/outside-codex-home" >/dev/null 2>&1; then
  echo "Codex plugin setup outside RUNNER_TEMP was accepted" >&2
  exit 1
fi

cp "$fixture/.agents/gh-steward.lock.json" "$test_root/gh-steward.json"
jq '.source_revision = "UNQUALIFIED_SOURCE_COMMIT"' "$test_root/gh-steward.json" > "$fixture/.agents/gh-steward.lock.json"
before_calls="$(wc -l < "$npm_log" | tr -d ' ')"
if env "${env_base[@]}" bash "$fixture/scripts/prepare-codex-plugins.sh" "$codex_home" >/dev/null 2>&1; then
  echo "unqualified gh-steward source revision was accepted" >&2
  exit 1
fi
[[ "$(wc -l < "$npm_log" | tr -d ' ')" == "$before_calls" ]]

printf 'Codex public plugin runtime fixtures: ok\n'
