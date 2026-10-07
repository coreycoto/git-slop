#!/usr/bin/env bash
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT
export RUNNER_TEMP="$scratch" GITHUB_SERVER_URL=https://github.com
cd "$scratch"

# The bootstrap adapter must preserve pinned bytes and cannot turn transport
# configuration into a native approval or an unbounded/foreign download.
mkdir "$scratch/fake-bin"
cat > "$scratch/fake-bin/gh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == api && "$2" == --hostname && "$3" == github.com && "$4" == repos/example/widgets/releases/assets/35 ]]
printf 'request\n' >> "$HISTORY_REQUESTS"
if [[ $# -eq 4 ]]; then
  printf '%s\n' "$HISTORY_METADATA"
else
  [[ $# -eq 6 && "$5" == -H && "$6" == 'Accept: application/octet-stream' ]]
  [[ "${HISTORY_DOWNLOAD_FAIL-false}" != true ]] || exit 6
  cat "$HISTORY_BODY"
fi
SH
chmod +x "$scratch/fake-bin/gh"
export HISTORY_REQUESTS="$scratch/history-requests" HISTORY_BODY="$scratch/history-body"
touch "$HISTORY_REQUESTS"
# Document bytes must not execute shell text.
# shellcheck disable=SC2016
printf '%s\n' '{"fixture":9007199254740993,"tokens":"$(touch unexpected)"}' > "$HISTORY_BODY"
history_sha="$(shasum -a 256 "$HISTORY_BODY" | awk '{print $1}')"
history_size="$(wc -c < "$HISTORY_BODY" | tr -d '[:space:]')"
descriptor="$(jq -nc --arg sha "$history_sha" '{kind:"history-promotion",asset_id:"35",sha256:$sha}')"
metadata="$(jq -nc --arg sha "$history_sha" --argjson size "$history_size" '{id:35,state:"uploaded",size:$size,digest:("sha256:"+$sha)}')"
history_call() {
  PATH="$scratch/fake-bin:$PATH" HISTORY_METADATA="$HISTORY_METADATA" \
    GH_STEWARD_HISTORY_DOCUMENT="$GH_STEWARD_HISTORY_DOCUMENT" \
    bash "$script_dir/acquire-gh-steward-history.sh" example/widgets
}
export GH_STEWARD_HISTORY_DOCUMENT='' HISTORY_METADATA="$metadata"
[[ -z "$(history_call)" && ! -s "$HISTORY_REQUESTS" ]]
for kind in history-promotion history-cutover; do
  GH_STEWARD_HISTORY_DOCUMENT="$(jq -c --arg kind "$kind" '.kind=$kind' <<< "$descriptor")"
  input="$(history_call)"
  [[ "$input" == "$kind=$RUNNER_TEMP/gh-steward-history."*/document.json ]]
  document="${input#*=}"
  cmp "$document" "$HISTORY_BODY"
  case "$(uname -s)" in
    Darwin) mode="$(stat -f '%Lp' "$document")" ;;
    *) mode="$(stat -c '%a' "$document")" ;;
  esac
  [[ "$mode" == 600 ]]
done
expect_history_failure() {
  if history_call > "$scratch/rejected-history" 2> "$scratch/rejected-history-error"; then
    printf 'unsafe history transport was accepted\n' >&2
    exit 1
  fi
  [[ ! -s "$scratch/rejected-history" ]]
}
for invalid in '{}' "$(jq -c '.kind="legacy-checkpoint"' <<< "$descriptor")" \
  "$(jq -c '.asset_id="../35"' <<< "$descriptor")" \
  "$(jq -c '.sha256="bad"' <<< "$descriptor")"; do
  export GH_STEWARD_HISTORY_DOCUMENT="$invalid"
  before="$(wc -l < "$HISTORY_REQUESTS")"
  expect_history_failure
  [[ "$(wc -l < "$HISTORY_REQUESTS")" -eq "$before" ]]
done
export GH_STEWARD_HISTORY_DOCUMENT="$descriptor"
for invalid in "$(jq -c '.id=36' <<< "$metadata")" \
  "$(jq -c '.size=8388609' <<< "$metadata")" \
  "$(jq -c '.state="new"' <<< "$metadata")" \
  "$(jq -c '.digest="sha256:bad"' <<< "$metadata")"; do
  export HISTORY_METADATA="$invalid"
  before="$(wc -l < "$HISTORY_REQUESTS")"
  expect_history_failure
  [[ "$(wc -l < "$HISTORY_REQUESTS")" -eq $((before + 1)) ]]
done
export HISTORY_METADATA="$metadata" HISTORY_DOWNLOAD_FAIL=true
expect_history_failure
unset HISTORY_DOWNLOAD_FAIL
# Same-size corrupt bytes must fail the hash check.
# shellcheck disable=SC2016
printf '%s\n' '{"fixture":9007199254740994,"tokens":"$(touch unexpected)"}' > "$HISTORY_BODY"
expect_history_failure
head -c 8388609 /dev/zero > "$HISTORY_BODY"
expect_history_failure
unset GH_STEWARD_HISTORY_DOCUMENT
printf 'history transport preserved raw bytes and rejected invalid pins and oversized responses\n'

[[ ! -e unexpected ]]
