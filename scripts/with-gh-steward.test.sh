#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/with-gh-steward-test.XXXXXX")"
trap 'rm -rf -- "$test_root"' EXIT

require_command() {
  command -v "$1" >/dev/null 2>&1 || { printf 'missing test prerequisite: %s\n' "$1" >&2; exit 1; }
}
for tool in jq shasum mktemp; do require_command "$tool"; done

fixture="$test_root/fixture"
mkdir -p "$fixture/scripts" "$fixture/.agents"
cp "$repo_root/scripts/with-gh-steward.sh" "$fixture/scripts/with-gh-steward.sh"
chmod 0755 "$fixture/scripts/with-gh-steward.sh"

revision="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
target="unsupported"
case "$(uname -s)/$(uname -m)" in
  Darwin/x86_64) target=darwin/amd64 ;;
  Darwin/arm64|Darwin/aarch64) target=darwin/arm64 ;;
  Linux/x86_64|Linux/amd64) target=linux/amd64 ;;
  Linux/aarch64|Linux/arm64) target=linux/arm64 ;;
esac
[[ "$target" != unsupported ]] || { echo "unsupported test host" >&2; exit 1; }

fake_bin="$test_root/fake-bin"
mkdir -m 0700 "$fake_bin"
cat > "$test_root/gh-steward" <<'BIN'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1-}" == version && "${2-}" == --json ]] || exit 2
printf '{"schema_version":2,"tool":"gh-steward","tool_version":"%s","source_revision":"%s","source_dirty":false,"target":"%s","go_version":"go-test"}\n' \
  "$FAKE_TOOL_VERSION" "$FAKE_SOURCE_REVISION" "$FAKE_TARGET"
BIN
chmod 0755 "$test_root/gh-steward"
binary_digest="$(shasum -a 256 "$test_root/gh-steward" | awk '{print $1}')"

cat > "$fixture/.agents/gh-steward.lock.json" <<JSON
{
  "schema_version": 1,
  "repository": "coreycoto/gh-steward",
  "version": "0.4.0",
  "source_revision": "$revision",
  "asset_sha256": {
    "darwin/amd64": "$binary_digest",
    "darwin/arm64": "$binary_digest",
    "linux/amd64": "$binary_digest",
    "linux/arm64": "$binary_digest"
  }
}
JSON

cat > "$fake_bin/git" <<'GIT'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$FAKE_GIT_LOG"
if [[ "${1-}" == clone ]]; then
  destination="${@: -1}"
  mkdir -p "$destination/scripts/release"
  cat > "$destination/scripts/release/acquire-gh-steward.sh" <<'ACQUIRE'
#!/usr/bin/env bash
set -euo pipefail
printf 'called %s %s %s %s\n' "$1" "$2" "$3" "$4" >> "$FAKE_ACQUIRE_LOG"
[[ "$1" == "$FAKE_TOOL_VERSION" && "$2" == "$FAKE_SOURCE_REVISION" && "$4" == "$FAKE_ASSET_SHA256" ]]
target=unsupported
case "$(uname -s)/$(uname -m)" in
  Darwin/x86_64) target=darwin/amd64 ;;
  Darwin/arm64|Darwin/aarch64) target=darwin/arm64 ;;
  Linux/x86_64|Linux/amd64) target=linux/amd64 ;;
  Linux/aarch64|Linux/arm64) target=linux/arm64 ;;
esac
[[ "$target" != unsupported ]]
mkdir -p "$3"
cp "$FAKE_BINARY_TEMPLATE" "$3/gh-steward"
chmod 0755 "$3/gh-steward"
asset="gh-steward_v${1}_${target//\//-}"
printf '{"schema_version":1,"tool":"gh-steward","tool_version":"%s","source_revision":"%s","source_dirty":false,"target":"%s","asset":"%s","asset_sha256":"%s","path":"%s/gh-steward"}\n' \
  "$1" "$2" "$target" "$asset" "$FAKE_ASSET_SHA256" "$3"
ACQUIRE
  chmod 0755 "$destination/scripts/release/acquire-gh-steward.sh"
  exit 0
fi
if [[ "${1-}" == -C && "${3-}" == rev-parse && "${4-}" == HEAD ]]; then
  printf '%s\n' "${FAKE_CHECKOUT_SHA:-$FAKE_SOURCE_REVISION}"
  exit 0
fi
if [[ "${1-}" == -C && ( "${3-}" == fetch || "${3-}" == checkout ) ]]; then
  [[ "${@: -1}" == "$FAKE_SOURCE_REVISION" ]]
  exit 0
fi
printf 'unexpected git invocation: %s\n' "$*" >&2
exit 2
GIT
chmod 0755 "$fake_bin/git"

cat > "$fake_bin/gh" <<'GH'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$FAKE_GH_LOG"
exit 99
GH
chmod 0755 "$fake_bin/gh"

runner_temp="$test_root/runner-temp"
mkdir -m 0700 "$runner_temp"
github_env="$test_root/github-env"
github_output="$test_root/github-output"
github_path="$test_root/github-path"
mkdir -p "$test_root/empty-gh-config" "$test_root/empty-xdg-data"
git_log="$test_root/git.log"
acquire_log="$test_root/acquire.log"
gh_log="$test_root/gh.log"
fake_env=(
  "PATH=$fake_bin:$PATH"
  "RUNNER_TEMP=$runner_temp"
  "GITHUB_ENV=$github_env"
  "GITHUB_OUTPUT=$github_output"
  "GITHUB_PATH=$github_path"
  "GH_CONFIG_DIR=$test_root/empty-gh-config"
  "XDG_DATA_HOME=$test_root/empty-xdg-data"
  "FAKE_GIT_LOG=$git_log"
  "FAKE_ACQUIRE_LOG=$acquire_log"
  "FAKE_GH_LOG=$gh_log"
  "FAKE_BINARY_TEMPLATE=$test_root/gh-steward"
  "FAKE_SOURCE_REVISION=$revision"
  "FAKE_TOOL_VERSION=0.4.0"
  "FAKE_TARGET=$target"
  "FAKE_ASSET_SHA256=$binary_digest"
)

env "${fake_env[@]}" bash "$fixture/scripts/with-gh-steward.sh" --prepare >/dev/null
grep -q "source_revision=$revision" "$github_output"
grep -q "target=$target" "$github_output"
grep -q "gh-steward-acquisition.json" "$github_env"
[[ "$(wc -l < "$acquire_log" | tr -d ' ')" == 1 ]]
[[ ! -e "$gh_log" ]]

binary_path="$(sed -n 's/^GH_STEWARD_BIN=//p' "$github_env")"
env "${fake_env[@]}" GH_STEWARD_BIN="$binary_path" bash "$fixture/scripts/with-gh-steward.sh" --verify >/dev/null
# The caller must use the receipt-bound path, without a host extension or PATH edit.
env -u GH_TOKEN -u GITHUB_TOKEN "${fake_env[@]}" "$binary_path" version --json \
  | jq -e --arg revision "$revision" ' .source_revision == $revision and .tool_version == "0.4.0"' >/dev/null
[[ ! -e "$gh_log" && ! -e "$github_path" ]]

cp "$fixture/.agents/gh-steward.lock.json" "$test_root/valid-lock.json"
jq '.source_revision = "UNQUALIFIED_SOURCE_COMMIT"' "$test_root/valid-lock.json" > "$fixture/.agents/gh-steward.lock.json"
rm -f "$git_log"
if env "${fake_env[@]}" bash "$fixture/scripts/with-gh-steward.sh" --prepare > /dev/null 2>&1; then
  echo "unqualified source lock was accepted" >&2
  exit 1
fi
[[ ! -e "$git_log" ]]
cp "$test_root/valid-lock.json" "$fixture/.agents/gh-steward.lock.json"

rm -f "$git_log" "$acquire_log"
if env "${fake_env[@]}" FAKE_CHECKOUT_SHA="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" \
  bash "$fixture/scripts/with-gh-steward.sh" --prepare > /dev/null 2>&1; then
  echo "mismatched source checkout was accepted" >&2
  exit 1
fi
[[ ! -e "$acquire_log" ]]

printf 'tamper' >> "$binary_path"
if env "${fake_env[@]}" GH_STEWARD_BIN="$binary_path" \
  bash "$fixture/scripts/with-gh-steward.sh" --verify > /dev/null 2>&1; then
  echo "tampered binary was accepted" >&2
  exit 1
fi

printf 'with-gh-steward offline acquisition fixtures: ok\n'
