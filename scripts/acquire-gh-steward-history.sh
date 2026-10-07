#!/usr/bin/env bash
# Internal transport only; native recovery owns document admission and authority.
set -euo pipefail

die() { printf 'acquire-gh-steward-history: %s\n' "$*" >&2; exit 1; }
[[ $# -eq 1 && "$1" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || die 'one repository identity is required'
descriptor="${GH_STEWARD_HISTORY_DOCUMENT-}"
[[ -n "$descriptor" ]] || exit 0
[[ ${#descriptor} -le 1024 ]] || die 'history document descriptor is too large'
[[ "${GITHUB_SERVER_URL-}" == https://github.com ]] || die 'transport requires the selected GitHub.com server'
[[ -n "${RUNNER_TEMP-}" && "$RUNNER_TEMP" == /* && -d "$RUNNER_TEMP" && ! -L "$RUNNER_TEMP" && "$RUNNER_TEMP" != *$'\n'* && "$RUNNER_TEMP" != *$'\r'* ]] || die 'a real absolute runner temporary directory is required'
jq -e '
  type == "object" and keys == ["asset_id", "kind", "sha256"] and
  (.asset_id | type == "string" and test("^[1-9][0-9]{0,14}$")) and
  (.kind == "history-cutover" or .kind == "history-promotion") and
  (.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
' <<< "$descriptor" > /dev/null || die 'history document descriptor is invalid'
asset_id="$(jq -r '.asset_id' <<< "$descriptor")"
kind="$(jq -r '.kind' <<< "$descriptor")"
expected_sha="$(jq -r '.sha256' <<< "$descriptor")"
endpoint="repos/$1/releases/assets/$asset_id"
umask 077
scratch="$(mktemp -d "$RUNNER_TEMP/gh-steward-history.XXXXXX")"
keep=false
trap 'if [[ "$keep" != true ]]; then rm -rf -- "$scratch"; fi' EXIT

# Both requests are bounded before their responses are parsed or admitted.
gh api --hostname github.com "$endpoint" | head -c 65537 > "$scratch/asset.json" || die 'history asset metadata request failed'
[[ "$(wc -c < "$scratch/asset.json")" -le 65536 ]] || die 'history asset metadata exceeds 64 KiB'
jq -e --arg id "$asset_id" --arg sha "$expected_sha" '
  (.id | tostring) == $id and .state == "uploaded" and
  (.size | type == "number" and . > 0 and . <= 8388608 and . == floor) and
  (.digest == null or .digest == ("sha256:" + $sha))
' "$scratch/asset.json" > /dev/null || die 'history asset identity, size or digest is invalid'
expected_size="$(jq -r '.size' "$scratch/asset.json")"
document="$scratch/document.json"
gh api --hostname github.com "$endpoint" -H 'Accept: application/octet-stream' | head -c 8388609 > "$document" || die 'history document request failed'
actual_size="$(wc -c < "$document")"
[[ "$actual_size" -eq "$expected_size" && "$actual_size" -le 8388608 ]] || die 'history document size differs or exceeds 8 MiB'
actual_sha="$(shasum -a 256 "$document" | awk '{print $1}')"
[[ "$actual_sha" == "$expected_sha" ]] || die 'history document bytes differ from the transport pin'
keep=true
printf '%s=%s\n' "$kind" "$document"
