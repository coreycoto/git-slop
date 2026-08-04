#!/usr/bin/env bash
set -euo pipefail

umask 077

readonly program_name="with-agent-plugins"
readonly expected_target="x86_64-unknown-linux-gnu"
readonly expected_repository="coreycoto/agent-plugins"
readonly expected_source_url="https://github.com/coreycoto/agent-plugins.git"
readonly expected_marketplace_name="agent-plugins-marketplace"
readonly expected_required_plugin="project-management-workflows"
readonly expected_release_manifest="release-manifest.json"
readonly expected_checksums="SHA256SUMS"
readonly expected_pex_version="2.98.2"
readonly expected_python_version="3.13.14"
readonly expected_pbs_release="20260718"
readonly expected_build_backend="hatchling"
readonly expected_build_backend_version="1.31.0"
readonly expected_inventory_format="path-utf8-nul-size-u64be-bytes-v1"
readonly smoke_sentinel="agent-plugins-standalone-smoke-ok"

prepare_download_dir=""
prepare_staging_dir=""
prepare_lock_path=""
prepare_lock_owner=""

cleanup_prepare_paths() {
  local status=$?
  trap - EXIT
  release_prepare_lock
  discard_staging_tree
  discard_download_tree
  exit "$status"
}

trap cleanup_prepare_paths EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

die() {
  printf '%s: error: %s\n' "$program_name" "$*" >&2
  exit 1
}

note() {
  printf '%s: %s\n' "$program_name" "$*" >&2
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

reject_actions_cache_root() {
  local tool_cache
  [[ -n "${RUNNER_TOOL_CACHE-}" ]] || return 0
  tool_cache="$RUNNER_TOOL_CACHE"
  if [[ -d "$tool_cache" ]]; then
    tool_cache="$(cd -- "$tool_cache" && pwd -P)"
  fi
  case "${runtime_root%/}/" in
    "${tool_cache%/}/"*)
      die "agent-plugins runtime root must not use RUNNER_TOOL_CACHE or an Actions cache"
      ;;
  esac
}

require_actions_ephemeral_root() {
  local candidate parent leaf runner_temp_physical
  [[ "${GITHUB_ACTIONS-}" == "true" ]] || return 0
  [[ -n "${RUNNER_TEMP-}" && "$RUNNER_TEMP" == /* && -d "$RUNNER_TEMP" ]] ||
    die "GitHub Actions requires an existing absolute RUNNER_TEMP"
  runner_temp_physical="$(cd -- "$RUNNER_TEMP" && pwd -P)"

  if [[ -d "$runtime_root" && ! -L "$runtime_root" ]]; then
    candidate="$(cd -- "$runtime_root" && pwd -P)"
  else
    parent="$(dirname -- "$runtime_root")"
    leaf="$(basename -- "$runtime_root")"
    [[ "$leaf" != "." && "$leaf" != ".." && -d "$parent" ]] ||
      die "GitHub Actions runtime root must have an existing safe parent under RUNNER_TEMP"
    candidate="$(cd -- "$parent" && pwd -P)/${leaf}"
  fi

  [[ "$candidate" != "$runner_temp_physical" ]] ||
    die "GitHub Actions runtime root must be a child directory of RUNNER_TEMP"
  case "${candidate%/}/" in
    "${runner_temp_physical%/}/"*) ;;
    *) die "GitHub Actions runtime root must remain under physical RUNNER_TEMP" ;;
  esac
}

release_prepare_lock() {
  if [[ -n "$prepare_lock_owner" && -e "$prepare_lock_owner" ]]; then
    if [[ -n "$prepare_lock_path" && -e "$prepare_lock_path" &&
      "$prepare_lock_path" -ef "$prepare_lock_owner" ]]; then
      rm -f -- "$prepare_lock_path"
    fi
    rm -f -- "$prepare_lock_owner"
  fi
  prepare_lock_path=""
  prepare_lock_owner=""
}

discard_download_tree() {
  if [[ -n "$prepare_download_dir" && -d "$prepare_download_dir" ]]; then
    chmod -R u+w "$prepare_download_dir" 2>/dev/null || true
    rm -rf -- "$prepare_download_dir"
  fi
  prepare_download_dir=""
}

discard_staging_tree() {
  if [[ -n "$prepare_staging_dir" && -d "$prepare_staging_dir" ]]; then
    chmod -R u+w "$prepare_staging_dir" 2>/dev/null || true
    rm -rf -- "$prepare_staging_dir"
  fi
  prepare_staging_dir=""
}

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

file_mode() {
  if stat -c '%a' "$1" >/dev/null 2>&1; then
    stat -c '%a' "$1"
  else
    stat -f '%Lp' "$1"
  fi
}

require_supported_host() {
  local kernel machine
  kernel="$(uname -s)"
  machine="$(uname -m)"
  [[ "$kernel" == "Linux" && ( "$machine" == "x86_64" || "$machine" == "amd64" ) ]] ||
    die "agent-plugins runtime supports Linux x86_64 only (found ${kernel}/${machine})"
}

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "${script_dir}/.." && pwd -P)"
readonly manifest_path="${repo_root}/.agents/plugins/marketplace-source.json"

# Capture the acquisition credential in a non-exported shell variable, then remove
# it before jq, tar, or any publisher executable can inherit it.
acquisition_token="${AGENT_PLUGINS_READ_TOKEN-}"
unset AGENT_PLUGINS_READ_TOKEN

load_consumer_contract() {
  require_command jq
  [[ -f "$manifest_path" && ! -L "$manifest_path" ]] ||
    die "trusted consumer manifest is missing or is not a regular file: $manifest_path"

  jq -e \
    --arg expected_marketplace_name "$expected_marketplace_name" \
    --arg expected_source_url "$expected_source_url" \
    --arg expected_required_plugin "$expected_required_plugin" '
    type == "object"
    and (.ref | type == "string" and test("^[0-9a-f]{40}$"))
    and .marketplace_name == $expected_marketplace_name
    and .source_url == $expected_source_url
    and .required_plugin == $expected_required_plugin
    and (.runtime_release | type == "object")
    and (.runtime_release.repository |
      type == "string" and test("^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$"))
    and (.runtime_release.tag | type == "string" and length > 1)
    and (.runtime_release.version |
      type == "string" and test("^[0-9]+\\.[0-9]+\\.[0-9]+([+.-][0-9A-Za-z.-]+)?$"))
    and (.runtime_release.target | type == "string" and length > 0)
    and (.runtime_release.archive | type == "string" and length > 0)
    and (.runtime_release.member | type == "string" and length > 0)
    and (.runtime_release.sha256 |
      type == "string" and test("^[0-9a-f]{64}$"))
    and (.runtime_release.size |
      type == "number" and . > 0 and . == floor)
    and (.runtime_release.release_manifest | type == "string" and length > 0)
    and (.runtime_release.checksums | type == "string" and length > 0)
  ' "$manifest_path" >/dev/null ||
    die "consumer manifest has an invalid ref or runtime_release contract: $manifest_path"

  source_revision="$(jq -r '.ref' "$manifest_path")"
  release_repository="$(jq -r '.runtime_release.repository' "$manifest_path")"
  release_tag="$(jq -r '.runtime_release.tag' "$manifest_path")"
  release_version="$(jq -r '.runtime_release.version' "$manifest_path")"
  release_target="$(jq -r '.runtime_release.target' "$manifest_path")"
  release_archive="$(jq -r '.runtime_release.archive' "$manifest_path")"
  release_member="$(jq -r '.runtime_release.member' "$manifest_path")"
  release_sha256="$(jq -r '.runtime_release.sha256' "$manifest_path")"
  release_size="$(jq -r '.runtime_release.size' "$manifest_path")"
  release_manifest="$(jq -r '.runtime_release.release_manifest' "$manifest_path")"
  release_checksums="$(jq -r '.runtime_release.checksums' "$manifest_path")"

  [[ "$release_repository" == "$expected_repository" ]] ||
    die "runtime release repository must be ${expected_repository}"
  [[ "$release_tag" == "v${release_version}" ]] ||
    die "runtime release tag must exactly match version (expected v${release_version}, found ${release_tag})"
  [[ "$release_target" == "$expected_target" ]] ||
    die "runtime release target must be ${expected_target} (found ${release_target})"
  [[ "$release_manifest" == "$expected_release_manifest" ]] ||
    die "runtime release manifest must be ${expected_release_manifest}"
  [[ "$release_checksums" == "$expected_checksums" ]] ||
    die "runtime checksum asset must be ${expected_checksums}"

  local expected_archive expected_member
  expected_archive="agent-plugins-${release_tag}-${release_target}.tar.gz"
  expected_member="agent-plugins-${release_tag}-${release_target}/agent-plugins"
  [[ "$release_archive" == "$expected_archive" ]] ||
    die "runtime archive name must be ${expected_archive}"
  [[ "$release_member" == "$expected_member" ]] ||
    die "runtime executable member must be ${expected_member}"
  [[ "$release_archive" != "$release_manifest" &&
    "$release_archive" != "$release_checksums" &&
    "$release_manifest" != "$release_checksums" ]] ||
    die "runtime release asset names must be distinct"

  if [[ -n "${AGENT_PLUGINS_RUNTIME_ROOT-}" ]]; then
    runtime_root="$AGENT_PLUGINS_RUNTIME_ROOT"
  else
    [[ -n "${RUNNER_TEMP-}" ]] ||
      die "RUNNER_TEMP is required unless AGENT_PLUGINS_RUNTIME_ROOT is set"
    runtime_root="${RUNNER_TEMP%/}/agent-plugins-runtime"
  fi
  [[ "$runtime_root" == /* && "$runtime_root" != "/" ]] ||
    die "agent-plugins runtime root must be an absolute, non-root path"
  if [[ -e "$runtime_root" || -L "$runtime_root" ]]; then
    [[ -d "$runtime_root" && ! -L "$runtime_root" ]] ||
      die "agent-plugins runtime root must be a regular directory: $runtime_root"
    runtime_root="$(cd -- "$runtime_root" && pwd -P)"
  fi
  reject_actions_cache_root
  require_actions_ephemeral_root

  install_dir="${runtime_root}/${source_revision}/${release_target}/${release_sha256}"
  artifacts_dir="${install_dir}/artifacts"
  runtime_executable="${install_dir}/bin/agent-plugins"
}

validate_release_manifest() {
  local path="$1"
  [[ -f "$path" && ! -L "$path" ]] || die "release manifest is missing or unsafe: $path"

  jq -e \
    --arg version "$release_version" \
    --arg tag "$release_tag" \
    --arg target "$release_target" \
    --arg revision "$source_revision" \
    --arg archive "$release_archive" \
    --arg member "$release_member" \
    --arg sha256 "$release_sha256" \
    --argjson size "$release_size" \
    --arg pex_version "$expected_pex_version" \
    --arg python_version "$expected_python_version" \
    --arg pbs_release "$expected_pbs_release" \
    --arg build_backend "$expected_build_backend" \
    --arg build_backend_version "$expected_build_backend_version" \
    --arg inventory_format "$expected_inventory_format" '
      type == "object"
      and .schema_version == 1
      and .version == $version
      and .tag == $tag
      and .target == $target
      and .source_revision == $revision
      and (.source_date_epoch |
        type == "number" and . >= 0 and . == floor)
      and (.runtime | type == "object")
      and .runtime.packager == "pex"
      and .runtime.pex_version == $pex_version
      and .runtime.scie == "eager"
      and .runtime.scie_platform == "linux-x86_64"
      and .runtime.python_version == $python_version
      and .runtime.pbs_release == $pbs_release
      and .runtime.build_backend == $build_backend
      and .runtime.build_backend_version == $build_backend_version
      and .runtime.pbs_stripped == true
      and .runtime.dependency_lock == "uv.lock"
      and .runtime.dependency_lock_format == "pex-lock-from-hashed-requirements"
      and .runtime.dependencies_binary_only == true
      and (.runtime.uv_lock_sha256 |
        type == "string" and test("^[0-9a-f]{64}$"))
      and .runtime.embedded_marketplace == true
      and .runtime.embedded_marketplace_name == "agent-plugins-marketplace"
      and .runtime.embedded_required_plugin == "project-management-workflows"
      and .runtime.embedded_inventory_format == $inventory_format
      and (.runtime.embedded_marketplace_sha256 |
        type == "string" and test("^[0-9a-f]{64}$"))
      and (.artifacts | type == "array" and length == 3)
      and (.artifacts | all(.[];
        type == "object"
        and (.kind | type == "string")
        and (.name |
          type == "string" and test("^[A-Za-z0-9][A-Za-z0-9._+-]*$"))
        and (.sha256 |
          type == "string" and test("^[0-9a-f]{64}$"))
        and (.size | type == "number" and . > 0 and . == floor)))
      and ((.artifacts | map(.name)) as $names |
        ($names == ($names | sort))
        and (($names | unique | length) == ($names | length)))
      and ((.artifacts | map(.kind) | sort) == ["sdist", "standalone", "wheel"])
      and ([.artifacts[] | select(.kind == "standalone")] | length == 1)
      and ([.artifacts[] | select(.kind == "standalone")][0] |
        .name == $archive
        and .target == $target
        and .member == $member
        and .sha256 == $sha256
        and .size == $size)
    ' "$path" >/dev/null ||
    die "release manifest does not match the pinned standalone runtime contract: $path"

  embedded_marketplace_sha256="$(jq -r '.runtime.embedded_marketplace_sha256' "$path")"
}

validate_checksums() {
  local directory="$1"
  local manifest_file="${directory}/${release_manifest}"
  local checksums_file="${directory}/${release_checksums}"
  local manifest_sha actual expected

  [[ -f "$checksums_file" && ! -L "$checksums_file" ]] ||
    die "checksum asset is missing or unsafe: $checksums_file"
  manifest_sha="$(sha256_file "$manifest_file")"
  actual="$(<"$checksums_file")"
  [[ "$(wc -l <"$checksums_file" | tr -d ' ')" == "4" ]] ||
    die "${release_checksums} must contain exactly four newline-terminated entries"
  expected="$({
    jq -r '.artifacts[] | "\(.sha256)  \(.name)"' "$manifest_file"
    printf '%s  %s\n' "$manifest_sha" "$release_manifest"
  })"
  [[ "$actual" == "$expected" ]] ||
    die "${release_checksums} must list each name-sorted manifest artifact once, followed by ${release_manifest}"
}

validate_archive() {
  local archive_path="$1"
  local actual_sha actual_size names verbose first_mode second_mode expected_directory

  [[ -f "$archive_path" && ! -L "$archive_path" ]] ||
    die "runtime archive is missing or unsafe: $archive_path"
  actual_sha="$(sha256_file "$archive_path")"
  [[ "$actual_sha" == "$release_sha256" ]] ||
    die "runtime archive SHA-256 mismatch (expected ${release_sha256}, found ${actual_sha})"
  actual_size="$(file_size "$archive_path")"
  [[ "$actual_size" == "$release_size" ]] ||
    die "runtime archive size mismatch (expected ${release_size}, found ${actual_size})"

  expected_directory="${release_member%/*}/"
  names="$(tar -tzf "$archive_path")" || die "runtime archive cannot be listed"
  [[ "$names" == "${expected_directory}"$'\n'"${release_member}" ]] ||
    die "runtime archive must contain exactly ${expected_directory} and ${release_member}"

  verbose="$(tar -tvzf "$archive_path")" || die "runtime archive types cannot be inspected"
  [[ "$(printf '%s\n' "$verbose" | wc -l | tr -d ' ')" == "2" ]] ||
    die "runtime archive contains an unexpected number of entries"
  first_mode="$(printf '%s\n' "$verbose" | sed -n '1s/^\(.\{10\}\).*/\1/p')"
  second_mode="$(printf '%s\n' "$verbose" | sed -n '2s/^\(.\{10\}\).*/\1/p')"
  [[ "$first_mode" == "drwxr-xr-x" ]] ||
    die "runtime archive root must be a mode-0755 directory"
  [[ "$second_mode" == "-rwxr-xr-x" ]] ||
    die "runtime archive executable must be a mode-0755 regular file"
}

validate_release_files() {
  local directory="$1"
  local entry_count

  [[ -d "$directory" && ! -L "$directory" ]] || die "release asset directory is unsafe: $directory"
  for name in "$release_archive" "$release_manifest" "$release_checksums"; do
    [[ -f "${directory}/${name}" && ! -L "${directory}/${name}" ]] ||
      die "expected one regular release asset named ${name}"
  done
  entry_count="$(find "$directory" -mindepth 1 -maxdepth 1 -print | wc -l | tr -d ' ')"
  [[ "$entry_count" == "3" ]] ||
    die "release download must contain exactly the archive, manifest, and checksums (found ${entry_count} entries)"

  validate_release_manifest "${directory}/${release_manifest}"
  validate_checksums "$directory"
  validate_archive "${directory}/${release_archive}"
}

new_session_root() {
  local sessions_dir="${runtime_root}/sessions"
  [[ ! -L "$sessions_dir" ]] || die "runtime session root must not be a symlink: $sessions_dir"
  mkdir -p -- "$sessions_dir"
  [[ -d "$sessions_dir" && ! -L "$sessions_dir" ]] ||
    die "runtime session root is not a regular directory: $sessions_dir"
  sessions_dir="$(cd -- "$sessions_dir" && pwd -P)"
  case "${sessions_dir}/" in
    "${runtime_root%/}/"*) ;;
    *) die "runtime session root escapes the ephemeral runtime root" ;;
  esac
  mktemp -d "${sessions_dir}/runtime.XXXXXX"
}

run_isolated_runtime() {
  local session_root="$1"
  shift
  env -i \
    HOME="${session_root}/home" \
    PATH="" \
    TMPDIR="${session_root}/tmp" \
    PEX_IGNORE_RCFILES=1 \
    SCIE_BASE="${session_root}/scie" \
    PEX_ROOT="${session_root}/pex" \
    "$@"
}

run_isolated_interpreter() {
  local session_root="$1"
  shift
  env -i \
    HOME="${session_root}/home" \
    PATH="" \
    TMPDIR="${session_root}/tmp" \
    PEX_IGNORE_RCFILES=1 \
    PEX_INTERPRETER=1 \
    SCIE_BASE="${session_root}/scie" \
    PEX_ROOT="${session_root}/pex" \
    "$@"
}

verify_runtime_identity() {
  local executable="$1"
  local session_root version_output revision_output smoke_output smoke_code

  [[ -f "$executable" && ! -L "$executable" && -x "$executable" ]] ||
    die "installed runtime is missing, linked, or not executable: $executable"
  session_root="$(new_session_root)"
  mkdir -p -- "${session_root}/home" "${session_root}/tmp"

  if ! version_output="$(
    run_isolated_runtime "$session_root" "$executable" --version
  )"; then
    rm -rf -- "$session_root"
    die "installed runtime failed its --version check"
  fi
  [[ "$version_output" == "agent-plugins ${release_version}" ]] || {
    rm -rf -- "$session_root"
    die "installed runtime reported unexpected version: ${version_output}"
  }

  if ! revision_output="$(
    run_isolated_runtime "$session_root" "$executable" --source-revision
  )"; then
    rm -rf -- "$session_root"
    die "installed runtime failed its --source-revision check"
  fi
  [[ "$revision_output" == "$source_revision" ]] || {
    rm -rf -- "$session_root"
    die "installed runtime source revision mismatch (expected ${source_revision}, found ${revision_output})"
  }

  smoke_code="from agent_plugins import build_info; from agent_plugins.marketplace import bootstrap; assert build_info.embedded_marketplace_sha256() == '${embedded_marketplace_sha256}'; print('${smoke_sentinel}')"
  if ! smoke_output="$(
    run_isolated_interpreter "$session_root" "$executable" -c "$smoke_code"
  )"; then
    rm -rf -- "$session_root"
    die "installed runtime failed its isolated interpreter import and provenance smoke"
  fi
  rm -rf -- "$session_root"
  [[ "$smoke_output" == "$smoke_sentinel" ]] ||
    die "installed runtime returned unexpected interpreter smoke output"
}

verify_installed_bytes() {
  local archived_sha installed_sha
  archived_sha="$(tar -xOzf "${artifacts_dir}/${release_archive}" "$release_member" | sha256sum | awk '{print $1}')" ||
    die "could not hash the archived runtime member"
  installed_sha="$(sha256_file "$runtime_executable")"
  [[ "$installed_sha" == "$archived_sha" ]] ||
    die "installed executable bytes do not match the pinned archive member"
  [[ "$(file_mode "$runtime_executable")" == "555" ]] ||
    die "installed runtime must be immutable and executable (mode 0555)"
  for asset in \
    "${artifacts_dir}/${release_archive}" \
    "${artifacts_dir}/${release_manifest}" \
    "${artifacts_dir}/${release_checksums}"; do
    [[ "$(file_mode "$asset")" == "444" ]] ||
      die "installed verification asset must be immutable (mode 0444): $asset"
  done
}

verify_install() {
  local parent_physical
  [[ -d "$install_dir" && ! -L "$install_dir" ]] ||
    die "verified runtime is not prepared at ${install_dir}; run ${program_name} --prepare in this job"
  [[ -d "${runtime_root}/${source_revision}" &&
    ! -L "${runtime_root}/${source_revision}" &&
    -d "${runtime_root}/${source_revision}/${release_target}" &&
    ! -L "${runtime_root}/${source_revision}/${release_target}" ]] ||
    die "runtime installation parent chain contains a missing or linked directory"
  parent_physical="$(cd -- "${runtime_root}/${source_revision}/${release_target}" && pwd -P)"
  [[ "$parent_physical" == "${runtime_root}/${source_revision}/${release_target}" ]] ||
    die "runtime installation parent chain escapes the ephemeral runtime root"
  [[ -d "${install_dir}/artifacts" && ! -L "${install_dir}/artifacts" &&
    -d "${install_dir}/bin" && ! -L "${install_dir}/bin" ]] ||
    die "runtime installation must contain regular artifacts and bin directories"
  [[ "$(find "$install_dir" -mindepth 1 -maxdepth 1 -print | wc -l | tr -d ' ')" == "2" &&
    "$(find "${install_dir}/bin" -mindepth 1 -maxdepth 1 -print | wc -l | tr -d ' ')" == "1" ]] ||
    die "runtime installation contains unexpected files"
  for directory in "$install_dir" "${install_dir}/artifacts" "${install_dir}/bin"; do
    [[ "$(file_mode "$directory")" == "555" ]] ||
      die "installed runtime directory must be immutable (mode 0555): $directory"
  done
  validate_release_files "$artifacts_dir"
  verify_installed_bytes
  verify_runtime_identity "$runtime_executable"
}

download_release_assets() {
  local destination="$1"
  [[ -n "$acquisition_token" ]] ||
    die "AGENT_PLUGINS_READ_TOKEN is required to acquire the private runtime release"
  require_command gh
  mkdir -p -- "${destination}/gh-config"

  if env \
    GH_TOKEN="$acquisition_token" \
    GH_CONFIG_DIR="${destination}/gh-config" \
    GH_PROMPT_DISABLED=1 \
    gh release download "$release_tag" \
      --repo "$release_repository" \
      --dir "$destination" \
      --pattern "$release_archive" \
      --pattern "$release_manifest" \
      --pattern "$release_checksums"; then
    :
  else
    local status=$?
    unset acquisition_token
    die "private release download failed with status ${status}"
  fi
  unset acquisition_token

  # The gh configuration directory was isolated only to avoid shared auth state;
  # it is not a release asset and must not enter validation or installation.
  rm -rf -- "${destination}/gh-config"
}

prepare_runtime() {
  local download_dir install_parent staging_dir extraction_dir lock_path lock_owner

  mkdir -p -- "$runtime_root"
  [[ -d "$runtime_root" && ! -L "$runtime_root" ]] ||
    die "agent-plugins runtime root must be a regular directory: $runtime_root"
  runtime_root="$(cd -- "$runtime_root" && pwd -P)"
  reject_actions_cache_root
  require_actions_ephemeral_root
  install_dir="${runtime_root}/${source_revision}/${release_target}/${release_sha256}"
  artifacts_dir="${install_dir}/artifacts"
  runtime_executable="${install_dir}/bin/agent-plugins"

  if [[ -e "$install_dir" || -L "$install_dir" ]]; then
    unset acquisition_token
    verify_install
    note "verified existing ephemeral runtime at ${install_dir}"
    return
  fi

  download_dir="$(mktemp -d "${runtime_root}/download.XXXXXX")"
  prepare_download_dir="$download_dir"
  download_release_assets "$download_dir"
  validate_release_files "$download_dir"

  install_parent="${runtime_root}/${source_revision}/${release_target}"
  mkdir -p -- "$install_parent"
  [[ -d "${runtime_root}/${source_revision}" &&
    ! -L "${runtime_root}/${source_revision}" &&
    -d "$install_parent" && ! -L "$install_parent" ]] ||
    die "runtime installation parent chain must contain only regular directories"
  install_parent="$(cd -- "$install_parent" && pwd -P)"
  [[ "$install_parent" == "${runtime_root}/${source_revision}/${release_target}" ]] ||
    die "runtime installation parent chain escapes the ephemeral runtime root"
  staging_dir="$(mktemp -d "${install_parent}/.install.XXXXXX")"
  prepare_staging_dir="$staging_dir"
  mkdir -p -- "${staging_dir}/artifacts" "${staging_dir}/bin" "${staging_dir}/extract"
  cp -- \
    "${download_dir}/${release_archive}" \
    "${download_dir}/${release_manifest}" \
    "${download_dir}/${release_checksums}" \
    "${staging_dir}/artifacts/"
  artifacts_dir="${staging_dir}/artifacts"
  validate_release_files "$artifacts_dir"
  extraction_dir="${staging_dir}/extract"
  tar -xzf "${staging_dir}/artifacts/${release_archive}" \
    -C "$extraction_dir" -- "$release_member"
  cp -- "${extraction_dir}/${release_member}" "${staging_dir}/bin/agent-plugins"
  rm -rf -- "$extraction_dir"
  discard_download_tree

  chmod 0444 \
    "${staging_dir}/artifacts/${release_archive}" \
    "${staging_dir}/artifacts/${release_manifest}" \
    "${staging_dir}/artifacts/${release_checksums}"
  chmod 0555 "$staging_dir/bin/agent-plugins" "$staging_dir/artifacts" "$staging_dir/bin"

  runtime_executable="${staging_dir}/bin/agent-plugins"
  validate_archive "${artifacts_dir}/${release_archive}"
  verify_installed_bytes
  verify_runtime_identity "$runtime_executable"
  chmod 0555 "$staging_dir"

  lock_path="${install_dir}.lock"
  lock_owner="${install_parent}/.lock-owner.$$.${RANDOM}.${RANDOM}"
  prepare_lock_path="$lock_path"
  prepare_lock_owner="$lock_owner"
  (set -o noclobber; : >"$lock_owner") 2>/dev/null ||
    die "could not create unique runtime lock owner: ${lock_owner}"
  if ! ln "$lock_owner" "$lock_path" 2>/dev/null; then
    release_prepare_lock
    if [[ -d "$install_dir" ]]; then
      discard_staging_tree
      artifacts_dir="${install_dir}/artifacts"
      runtime_executable="${install_dir}/bin/agent-plugins"
      verify_install
      note "verified concurrently prepared ephemeral runtime at ${install_dir}"
      return
    fi
    die "another process is preparing this runtime; retry after it completes: ${lock_path}"
  fi
  if [[ -e "$install_dir" || -L "$install_dir" ]]; then
    release_prepare_lock
    discard_staging_tree
    artifacts_dir="${install_dir}/artifacts"
    runtime_executable="${install_dir}/bin/agent-plugins"
    verify_install
    note "verified concurrently prepared ephemeral runtime at ${install_dir}"
    return
  fi
  mv -- "$staging_dir" "$install_dir"
  prepare_staging_dir=""
  release_prepare_lock

  artifacts_dir="${install_dir}/artifacts"
  runtime_executable="${install_dir}/bin/agent-plugins"
  verify_install
  note "prepared and verified ephemeral runtime at ${install_dir}"
}

exec_runtime() {
  local session_root
  session_root="$(new_session_root)"
  unset \
    AGENT_PLUGINS_READ_TOKEN \
    PYTHONHOME PYTHONPATH PYTHONINSPECT PYTHONSTARTUP PYTHONUSERBASE \
    PEX_INTERPRETER PEX_MODULE PEX_SCRIPT PEX_TOOLS \
    PEX_PATH PEX_PYTHON PEX_PYTHON_PATH PEX_EXTRA_SYS_PATH
  exec env \
    PEX_IGNORE_RCFILES=1 \
    SCIE_BASE="${session_root}/scie" \
    PEX_ROOT="${session_root}/pex" \
    "$runtime_executable" "$@"
}

exec_python_compatibility() {
  local session_root
  local -a clean_environment
  session_root="$(new_session_root)"
  mkdir -p -- "${session_root}/home" "${session_root}/tmp"
  clean_environment=(
    env -i
    "HOME=${session_root}/home"
    "PATH=${PATH:-/usr/bin:/bin}"
    "TMPDIR=${session_root}/tmp"
    "CI=${CI-}"
    "RUNNER_TEMP=${RUNNER_TEMP-}"
    "RUNNER_OS=${RUNNER_OS-}"
    "RUNNER_ARCH=${RUNNER_ARCH-}"
    "RUNNER_NAME=${RUNNER_NAME-}"
    "RUNNER_ENVIRONMENT=${RUNNER_ENVIRONMENT-}"
    "GITHUB_ACTIONS=${GITHUB_ACTIONS-}"
    "GITHUB_API_URL=${GITHUB_API_URL-}"
    "GITHUB_SERVER_URL=${GITHUB_SERVER_URL-}"
    "GITHUB_GRAPHQL_URL=${GITHUB_GRAPHQL_URL-}"
    "GITHUB_REPOSITORY=${GITHUB_REPOSITORY-}"
    "GITHUB_REPOSITORY_ID=${GITHUB_REPOSITORY_ID-}"
    "GITHUB_REPOSITORY_OWNER=${GITHUB_REPOSITORY_OWNER-}"
    "GITHUB_REPOSITORY_OWNER_ID=${GITHUB_REPOSITORY_OWNER_ID-}"
    "GITHUB_WORKSPACE=${GITHUB_WORKSPACE-}"
    "GITHUB_EVENT_NAME=${GITHUB_EVENT_NAME-}"
    "GITHUB_EVENT_PATH=${GITHUB_EVENT_PATH-}"
    "GITHUB_SHA=${GITHUB_SHA-}"
    "GITHUB_REF=${GITHUB_REF-}"
    "GITHUB_REF_NAME=${GITHUB_REF_NAME-}"
    "GITHUB_REF_TYPE=${GITHUB_REF_TYPE-}"
    "GITHUB_HEAD_REF=${GITHUB_HEAD_REF-}"
    "GITHUB_BASE_REF=${GITHUB_BASE_REF-}"
    "GITHUB_RUN_ID=${GITHUB_RUN_ID-}"
    "GITHUB_RUN_NUMBER=${GITHUB_RUN_NUMBER-}"
    "GITHUB_RUN_ATTEMPT=${GITHUB_RUN_ATTEMPT-}"
    "GITHUB_WORKFLOW=${GITHUB_WORKFLOW-}"
    "GITHUB_WORKFLOW_REF=${GITHUB_WORKFLOW_REF-}"
    "GITHUB_WORKFLOW_SHA=${GITHUB_WORKFLOW_SHA-}"
    "GITHUB_JOB=${GITHUB_JOB-}"
    "GITHUB_ACTION=${GITHUB_ACTION-}"
    "GITHUB_ACTOR=${GITHUB_ACTOR-}"
    "GITHUB_ACTOR_ID=${GITHUB_ACTOR_ID-}"
    "GITHUB_TRIGGERING_ACTOR=${GITHUB_TRIGGERING_ACTOR-}"
  )
  exec "${clean_environment[@]}" \
    PEX_IGNORE_RCFILES=1 \
    PEX_INTERPRETER=1 \
    SCIE_BASE="${session_root}/scie" \
    PEX_ROOT="${session_root}/pex" \
    "$runtime_executable" "$@"
}

require_supported_host
load_consumer_contract
require_command awk
require_command find
require_command ln
require_command mktemp
require_command sed
require_command sha256sum
require_command stat
require_command tar

case "${1-}" in
  --prepare)
    [[ "$#" == "1" ]] || die "--prepare does not accept additional arguments"
    prepare_runtime
    ;;
  --verify)
    [[ "$#" == "1" ]] || die "--verify does not accept additional arguments"
    unset acquisition_token
    verify_install
    note "verified ephemeral runtime at ${install_dir}"
    ;;
  python)
    unset acquisition_token
    shift
    verify_install
    exec_python_compatibility "$@"
    ;;
  *)
    unset acquisition_token
    verify_install
    exec_runtime "$@"
    ;;
esac
