#!/usr/bin/env bash
set -euo pipefail

test_root="$(mktemp -d "${TMPDIR:-/tmp}/with-agent-plugins-test.XXXXXX")"
cleanup() {
  chmod -R u+w "$test_root" 2>/dev/null || true
  rm -rf -- "$test_root"
}
trap cleanup EXIT

fixture_repo="${test_root}/repo"
release_dir="${test_root}/release"
fake_bin="${test_root}/fake-bin"
runner_temp="${test_root}/runner"
mkdir -p \
  "${fixture_repo}/scripts" \
  "${fixture_repo}/.agents/plugins" \
  "$release_dir" \
  "$fake_bin" \
  "$runner_temp"
cp "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/with-agent-plugins.sh" \
  "${fixture_repo}/scripts/with-agent-plugins.sh"
chmod 0755 "${fixture_repo}/scripts/with-agent-plugins.sh"

FAKE_EXPECTED_REVISION="0123456789abcdef0123456789abcdef01234567"
FAKE_EXPECTED_VERSION="0.1.0"
FAKE_EXPECTED_EMBEDDED_SHA="dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
runtime_exec_log="${test_root}/runtime-exec.log"

{
  printf '#!/bin/bash\n'
  printf 'set -euo pipefail\n'
  printf 'echo invoked >> %q\n' "$runtime_exec_log"
  cat <<'RUNTIME'
[[ "${PEX_IGNORE_RCFILES-}" == "1" ]]
[[ -n "${SCIE_BASE-}" && -n "${PEX_ROOT-}" ]]

case "${1-}" in
  --version)
    [[ -z "${ROADMAP_GH_TOKEN+x}" && -z "${CUSTOM_SECRET+x}" ]]
    printf 'agent-plugins 0.1.0\n'
    ;;
  --source-revision)
    [[ -z "${ROADMAP_GH_TOKEN+x}" && -z "${CUSTOM_SECRET+x}" ]]
    printf '0123456789abcdef0123456789abcdef01234567\n'
    ;;
  -c)
    [[ "${PEX_INTERPRETER-}" == "1" ]]
    [[ -z "${AGENT_PLUGINS_READ_TOKEN+x}" ]]
    [[ -z "${GH_TOKEN+x}" ]]
    [[ -z "${GITHUB_TOKEN+x}" ]]
    [[ -z "${ROADMAP_GH_TOKEN+x}" && -z "${CUSTOM_SECRET+x}" ]]
    [[ -z "${PYTHONHOME+x}" && -z "${PYTHONPATH+x}" ]]
    [[ "$HOME" == */sessions/runtime.*/home ]]
    [[ ! -e "$HOME/credential-sentinel" ]]
    if [[ "${2-}" == *agent-plugins-standalone-smoke-ok* ]]; then
      [[ "${2-}" == *dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd* ]]
      printf 'agent-plugins-standalone-smoke-ok\n'
    else
      printf 'python-ok\n'
    fi
    ;;
  inspect-env)
    [[ "${GH_TOKEN-}" == "workflow-token" ]]
    [[ -z "${AGENT_PLUGINS_READ_TOKEN+x}" ]]
    [[ "${ROADMAP_GH_TOKEN-}" == "roadmap-token" ]]
    [[ "${CUSTOM_SECRET-}" == "custom-token" ]]
    [[ -z "${PYTHONHOME+x}" && -z "${PYTHONPATH+x}" ]]
    [[ -z "${PEX_INTERPRETER+x}" && -z "${PEX_MODULE+x}" ]]
    [[ -z "${PEX_SCRIPT+x}" && -z "${PEX_TOOLS+x}" ]]
    printf 'direct-ok\n'
    ;;
  *)
    printf 'unexpected fake runtime arguments: %s\n' "$*" >&2
    exit 64
    ;;
esac
RUNTIME
} >"${test_root}/agent-plugins"
chmod 0755 "${test_root}/agent-plugins"

archive_root="agent-plugins-v${FAKE_EXPECTED_VERSION}-x86_64-unknown-linux-gnu"
archive="${archive_root}.tar.gz"
member="${archive_root}/agent-plugins"
mkdir -p "${test_root}/payload/${archive_root}"
chmod 0755 "${test_root}/payload/${archive_root}"
cp "${test_root}/agent-plugins" "${test_root}/payload/${member}"
chmod 0755 "${test_root}/payload/${member}"
COPYFILE_DISABLE=1 tar -czf "${release_dir}/${archive}" \
  -C "${test_root}/payload" "$archive_root"

printf 'wheel fixture\n' >"${release_dir}/agent_plugins-0.1.0-py3-none-any.whl"
printf 'sdist fixture\n' >"${release_dir}/agent_plugins-0.1.0.tar.gz"

sha256_file() {
  sha256sum "$1" | awk '{print $1}'
}

file_size() {
  if stat -c '%s' "$1" >/dev/null 2>&1; then
    stat -c '%s' "$1"
  else
    stat -f '%z' "$1"
  fi
}

archive_sha="$(sha256_file "${release_dir}/${archive}")"
archive_size="$(file_size "${release_dir}/${archive}")"
wheel_sha="$(sha256_file "${release_dir}/agent_plugins-0.1.0-py3-none-any.whl")"
wheel_size="$(file_size "${release_dir}/agent_plugins-0.1.0-py3-none-any.whl")"
sdist_sha="$(sha256_file "${release_dir}/agent_plugins-0.1.0.tar.gz")"
sdist_size="$(file_size "${release_dir}/agent_plugins-0.1.0.tar.gz")"

jq -n \
  --arg version "$FAKE_EXPECTED_VERSION" \
  --arg tag "v${FAKE_EXPECTED_VERSION}" \
  --arg target "x86_64-unknown-linux-gnu" \
  --arg revision "$FAKE_EXPECTED_REVISION" \
  --arg archive "$archive" \
  --arg member "$member" \
  --arg archive_sha "$archive_sha" \
  --argjson archive_size "$archive_size" \
  --arg wheel_sha "$wheel_sha" \
  --argjson wheel_size "$wheel_size" \
  --arg sdist_sha "$sdist_sha" \
  --argjson sdist_size "$sdist_size" \
  --arg embedded_sha "$FAKE_EXPECTED_EMBEDDED_SHA" '
  {
    artifacts: ([
      {
        kind: "standalone",
        member: $member,
        name: $archive,
        sha256: $archive_sha,
        size: $archive_size,
        target: $target
      },
      {
        kind: "wheel",
        name: "agent_plugins-0.1.0-py3-none-any.whl",
        sha256: $wheel_sha,
        size: $wheel_size
      },
      {
        kind: "sdist",
        name: "agent_plugins-0.1.0.tar.gz",
        sha256: $sdist_sha,
        size: $sdist_size
      }
    ] | sort_by(.name)),
    runtime: {
      build_backend: "hatchling",
      build_backend_version: "1.31.0",
      dependencies_binary_only: true,
      dependency_lock: "uv.lock",
      dependency_lock_format: "pex-lock-from-hashed-requirements",
      embedded_inventory_format: "path-utf8-nul-size-u64be-bytes-v1",
      embedded_marketplace: true,
      embedded_marketplace_name: "agent-plugins-marketplace",
      embedded_marketplace_sha256: $embedded_sha,
      embedded_required_plugin: "project-management-workflows",
      packager: "pex",
      pbs_release: "20260718",
      pbs_stripped: true,
      pex_version: "2.98.2",
      python_version: "3.13.14",
      scie: "eager",
      scie_platform: "linux-x86_64",
      uv_lock_sha256: "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
    },
    schema_version: 1,
    source_date_epoch: 1700000000,
    source_revision: $revision,
    tag: $tag,
    target: $target,
    version: $version
  }
  ' >"${release_dir}/release-manifest.json"

manifest_sha="$(sha256_file "${release_dir}/release-manifest.json")"
{
  printf '%s  %s\n' "$archive_sha" "$archive"
  printf '%s  %s\n' "$wheel_sha" "agent_plugins-0.1.0-py3-none-any.whl"
  printf '%s  %s\n' "$sdist_sha" "agent_plugins-0.1.0.tar.gz"
  printf '%s  %s\n' "$manifest_sha" "release-manifest.json"
} | LC_ALL=C sort >"${release_dir}/SHA256SUMS"

jq -n \
  --arg revision "$FAKE_EXPECTED_REVISION" \
  --arg archive "$archive" \
  --arg member "$member" \
  --arg sha256 "$archive_sha" \
  --argjson size "$archive_size" '
  {
    marketplace_name: "agent-plugins-marketplace",
    source_url: "https://github.com/coreycoto/agent-plugins.git",
    ref: $revision,
    required_plugin: "project-management-workflows",
    runtime_release: {
      repository: "coreycoto/agent-plugins",
      tag: "v0.1.0",
      version: "0.1.0",
      target: "x86_64-unknown-linux-gnu",
      archive: $archive,
      member: $member,
      sha256: $sha256,
      size: $size,
      release_manifest: "release-manifest.json",
      checksums: "SHA256SUMS"
    }
  }
  ' >"${fixture_repo}/.agents/plugins/marketplace-source.json"
base_consumer_manifest="${test_root}/marketplace-source.base.json"
base_release_dir="${test_root}/release-base"
cp "${fixture_repo}/.agents/plugins/marketplace-source.json" "$base_consumer_manifest"
mkdir -p "$base_release_dir"
cp "${release_dir}"/* "$base_release_dir/"

cat >"${fake_bin}/uname" <<'UNAME'
#!/usr/bin/env bash
case "${1-}" in
  -s) printf 'Linux\n' ;;
  -m) printf 'x86_64\n' ;;
  *) printf 'Linux\n' ;;
esac
UNAME
chmod 0755 "${fake_bin}/uname"

export FAKE_RELEASE_DIR="$release_dir"
export FAKE_GH_LOG="${test_root}/gh.log"
cat >"${fake_bin}/gh" <<'GH'
#!/usr/bin/env bash
set -euo pipefail
[[ "${AGENT_PLUGINS_READ_TOKEN+x}" != "x" ]]
[[ "${GH_TOKEN-}" == "read-token" ]]
[[ "${GH_PROMPT_DISABLED-}" == "1" ]]
[[ -n "${GH_CONFIG_DIR-}" && -d "$GH_CONFIG_DIR" ]]
[[ "${1-}" == "release" && "${2-}" == "download" && "${3-}" == "v0.1.0" ]]
shift 3
destination=""
repository=""
patterns=()
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --repo) repository="$2"; shift 2 ;;
    --dir) destination="$2"; shift 2 ;;
    --pattern) patterns+=("$2"); shift 2 ;;
    *) exit 65 ;;
  esac
done
[[ "$repository" == "coreycoto/agent-plugins" ]]
[[ -n "$destination" && "${#patterns[@]}" == "3" ]]
for pattern in "${patterns[@]}"; do
  cp "${FAKE_RELEASE_DIR}/${pattern}" "${destination}/${pattern}"
done
printf 'download\n' >>"$FAKE_GH_LOG"
GH
chmod 0755 "${fake_bin}/gh"

cat >"${fake_bin}/cp" <<'CP'
#!/bin/bash
set -euo pipefail
destination="${!#}"
/bin/cp "$@"
if [[ "${FAKE_MUTATE_STAGED_ARCHIVE-}" == "1" ]]; then
  case "$destination" in
    */.install.*/artifacts/|*/.install.*/artifacts)
      printf 'tampered after initial validation\n' \
        >>"${destination%/}/${FAKE_ARCHIVE_NAME}"
      printf 'mutated\n' >>"$FAKE_MUTATION_LOG"
      ;;
  esac
fi
CP
chmod 0755 "${fake_bin}/cp"

wrapper="${fixture_repo}/scripts/with-agent-plugins.sh"
export PATH="${fake_bin}:${PATH}"
export RUNNER_TEMP="$runner_temp"

refresh_release_chain() {
  local scenario_release="$1"
  local scenario_archive_sha scenario_archive_size scenario_manifest_sha temporary
  scenario_archive_sha="$(sha256_file "${scenario_release}/${archive}")"
  scenario_archive_size="$(file_size "${scenario_release}/${archive}")"

  temporary="${scenario_release}/.release-manifest.tmp"
  jq \
    --arg archive "$archive" \
    --arg sha256 "$scenario_archive_sha" \
    --argjson size "$scenario_archive_size" '
      (.artifacts[] | select(.kind == "standalone" and .name == $archive) | .sha256) = $sha256
      | (.artifacts[] | select(.kind == "standalone" and .name == $archive) | .size) = $size
    ' "${scenario_release}/release-manifest.json" >"$temporary"
  mv "$temporary" "${scenario_release}/release-manifest.json"
  scenario_manifest_sha="$(sha256_file "${scenario_release}/release-manifest.json")"
  {
    jq -r '.artifacts[] | "\(.sha256)  \(.name)"' \
      "${scenario_release}/release-manifest.json"
    printf '%s  %s\n' "$scenario_manifest_sha" "release-manifest.json"
  } | LC_ALL=C sort >"${scenario_release}/SHA256SUMS"

  temporary="${fixture_repo}/.agents/plugins/.marketplace-source.tmp"
  jq \
    --arg sha256 "$scenario_archive_sha" \
    --argjson size "$scenario_archive_size" '
      .runtime_release.sha256 = $sha256
      | .runtime_release.size = $size
    ' "$base_consumer_manifest" >"$temporary"
  mv "$temporary" "${fixture_repo}/.agents/plugins/marketplace-source.json"
}

copy_release_scenario() {
  local destination="$1"
  mkdir -p "$destination"
  cp "${base_release_dir}"/* "$destination/"
}

ROADMAP_GH_TOKEN="roadmap-token" \
CUSTOM_SECRET="custom-token" \
AGENT_PLUGINS_READ_TOKEN="read-token" \
"$wrapper" --prepare
ROADMAP_GH_TOKEN="roadmap-token" CUSTOM_SECRET="custom-token" "$wrapper" --verify

direct_output="$(
  GH_TOKEN="workflow-token" \
  ROADMAP_GH_TOKEN="roadmap-token" \
  CUSTOM_SECRET="custom-token" \
  AGENT_PLUGINS_READ_TOKEN="must-not-leak" \
  PYTHONHOME="/unsafe/python-home" \
  PYTHONPATH="/unsafe/python-path" \
  PEX_INTERPRETER="unsafe" \
  PEX_MODULE="unsafe" \
  PEX_SCRIPT="unsafe" \
  PEX_TOOLS="unsafe" \
  "$wrapper" inspect-env
)"
[[ "$direct_output" == "direct-ok" ]]

mkdir -p "${test_root}/caller-home"
printf 'credential sentinel\n' >"${test_root}/caller-home/credential-sentinel"
python_output="$(
  HOME="${test_root}/caller-home" \
  GH_TOKEN="must-not-reach-interpreter" \
  ROADMAP_GH_TOKEN="must-not-reach-interpreter" \
  CUSTOM_SECRET="must-not-reach-interpreter" \
  AGENT_PLUGINS_READ_TOKEN="must-not-leak" \
  PYTHONHOME="/unsafe/python-home" \
  PYTHONPATH="/unsafe/python-path" \
  "$wrapper" python -c 'print("python-ok")'
)"
[[ "$python_output" == "python-ok" ]]
[[ "$(wc -l <"$FAKE_GH_LOG" | tr -d ' ')" == "1" ]]

empty_runner="${test_root}/empty-runner"
mkdir -p "$empty_runner"
if RUNNER_TEMP="$empty_runner" "$wrapper" inspect-env >"${test_root}/missing.out" 2>&1; then
  printf 'normal invocation unexpectedly acquired a missing runtime\n' >&2
  exit 1
fi
grep -q -- '--prepare' "${test_root}/missing.out"
[[ "$(wc -l <"$FAKE_GH_LOG" | tr -d ' ')" == "1" ]]

# GitHub Actions may use only the physical per-job RUNNER_TEMP tree, even when
# an explicit runtime-root override points at another absolute writable path.
actions_runner="${test_root}/actions-runner"
mkdir -p "$actions_runner"
if GITHUB_ACTIONS="true" \
  RUNNER_TEMP="$actions_runner" \
  AGENT_PLUGINS_RUNTIME_ROOT="${test_root}/persistent-runtime" \
  AGENT_PLUGINS_READ_TOKEN="read-token" \
  "$wrapper" --prepare >"${test_root}/actions-root.out" 2>&1; then
  printf 'Actions runtime root unexpectedly escaped RUNNER_TEMP\n' >&2
  exit 1
fi
grep -q 'must remain under physical RUNNER_TEMP' "${test_root}/actions-root.out"
[[ "$(wc -l <"$FAKE_GH_LOG" | tr -d ' ')" == "1" ]]

# A consumer size pin must remain a positive JSON integer; a quoted byte count
# fails before acquisition and therefore cannot reach gh.
jq '.runtime_release.size |= tostring' "$base_consumer_manifest" \
  >"${fixture_repo}/.agents/plugins/marketplace-source.json"
if RUNNER_TEMP="${test_root}/string-size-runner" \
  AGENT_PLUGINS_READ_TOKEN="read-token" \
  "$wrapper" --prepare >"${test_root}/string-size.out" 2>&1; then
  printf 'quoted runtime size unexpectedly passed consumer validation\n' >&2
  exit 1
fi
grep -q 'invalid ref or runtime_release contract' "${test_root}/string-size.out"
[[ "$(wc -l <"$FAKE_GH_LOG" | tr -d ' ')" == "1" ]]
cp "$base_consumer_manifest" "${fixture_repo}/.agents/plugins/marketplace-source.json"

# A downloaded checksum file must agree exactly with the release manifest.
checksum_release="${test_root}/checksum-release"
copy_release_scenario "$checksum_release"
printf '\n' >>"${checksum_release}/SHA256SUMS"
mkdir -p "${test_root}/checksum-runner"
if RUNNER_TEMP="${test_root}/checksum-runner" \
  FAKE_RELEASE_DIR="$checksum_release" \
  AGENT_PLUGINS_READ_TOKEN="read-token" \
  "$wrapper" --prepare >"${test_root}/checksum.out" 2>&1; then
  printf 'checksum disagreement unexpectedly passed release validation\n' >&2
  exit 1
fi
grep -q 'exactly four newline-terminated entries' "${test_root}/checksum.out"

# Mutating only the staged archive copy after the pristine download has passed
# validation must fail before extraction or any publisher executable invocation.
toctou_release="${test_root}/toctou-release"
toctou_runner="${test_root}/toctou-runner"
toctou_mutation_log="${test_root}/toctou-mutation.log"
copy_release_scenario "$toctou_release"
mkdir -p "$toctou_runner"
runtime_count_before="$(wc -l <"$runtime_exec_log" | tr -d ' ')"
if RUNNER_TEMP="$toctou_runner" \
  FAKE_RELEASE_DIR="$toctou_release" \
  FAKE_MUTATE_STAGED_ARCHIVE="1" \
  FAKE_ARCHIVE_NAME="$archive" \
  FAKE_MUTATION_LOG="$toctou_mutation_log" \
  AGENT_PLUGINS_READ_TOKEN="read-token" \
  "$wrapper" --prepare >"${test_root}/toctou.out" 2>&1; then
  printf 'mutated staged archive unexpectedly reached publisher execution\n' >&2
  exit 1
fi
grep -q 'runtime archive SHA-256 mismatch' "${test_root}/toctou.out"
[[ "$(wc -l <"$toctou_mutation_log" | tr -d ' ')" == "1" ]]
runtime_count_after="$(wc -l <"$runtime_exec_log" | tr -d ' ')"
[[ "$runtime_count_after" == "$runtime_count_before" ]]
[[ -z "$(find "$toctou_runner" -path '*/bin/agent-plugins' -type f -print -quit)" ]]

# Even when every digest and size pin agrees, a traversal member is rejected by
# the exact two-member archive contract before extraction.
traversal_release="${test_root}/traversal-release"
traversal_payload="${test_root}/traversal-payload"
copy_release_scenario "$traversal_release"
mkdir -p "${traversal_payload}/${archive_root}"
chmod 0755 "${traversal_payload}/${archive_root}"
cp "${test_root}/agent-plugins" "${traversal_payload}/${member}"
chmod 0755 "${traversal_payload}/${member}"
printf 'escape\n' >"${traversal_payload}/escape"
if tar --version 2>/dev/null | grep -q 'GNU tar'; then
  COPYFILE_DISABLE=1 tar -czf "${traversal_release}/${archive}" \
    --transform='s,^escape$,../escape,' \
    -C "$traversal_payload" "$archive_root" escape
else
  COPYFILE_DISABLE=1 tar -czf "${traversal_release}/${archive}" \
    -s ',^escape$,../escape,' \
    -C "$traversal_payload" "$archive_root" escape
fi
refresh_release_chain "$traversal_release"
mkdir -p "${test_root}/traversal-runner"
if RUNNER_TEMP="${test_root}/traversal-runner" \
  FAKE_RELEASE_DIR="$traversal_release" \
  AGENT_PLUGINS_READ_TOKEN="read-token" \
  "$wrapper" --prepare >"${test_root}/traversal.out" 2>&1; then
  printf 'traversal archive unexpectedly passed validation\n' >&2
  exit 1
fi
grep -q 'runtime archive must contain exactly' "${test_root}/traversal.out"

# Exact member names are insufficient: the executable must also be a regular
# mode-0755 file, never a symlink or another special tar entry.
linked_release="${test_root}/linked-release"
linked_payload="${test_root}/linked-payload"
copy_release_scenario "$linked_release"
mkdir -p "${linked_payload}/${archive_root}"
chmod 0755 "${linked_payload}/${archive_root}"
ln -s 'not-an-executable' "${linked_payload}/${member}"
COPYFILE_DISABLE=1 tar -czf "${linked_release}/${archive}" \
  -C "$linked_payload" "$archive_root"
refresh_release_chain "$linked_release"
mkdir -p "${test_root}/linked-runner"
if RUNNER_TEMP="${test_root}/linked-runner" \
  FAKE_RELEASE_DIR="$linked_release" \
  AGENT_PLUGINS_READ_TOKEN="read-token" \
  "$wrapper" --prepare >"${test_root}/linked.out" 2>&1; then
  printf 'linked runtime member unexpectedly passed validation\n' >&2
  exit 1
fi
grep -q 'mode-0755 regular file' "${test_root}/linked.out"

cp "$base_consumer_manifest" "${fixture_repo}/.agents/plugins/marketplace-source.json"

installed_runtime="$(find "$runner_temp" -path '*/bin/agent-plugins' -type f -print)"
[[ -n "$installed_runtime" ]]
chmod 0755 "$installed_runtime"
printf 'tampered\n' >>"$installed_runtime"
if "$wrapper" --verify >"${test_root}/tamper.out" 2>&1; then
  printf 'offline verification unexpectedly accepted a tampered runtime\n' >&2
  exit 1
fi
grep -q 'installed executable bytes do not match' "${test_root}/tamper.out"

printf 'with-agent-plugins fixture: ok\n'
