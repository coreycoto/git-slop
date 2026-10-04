#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
# shellcheck source=scripts/dependency-remediation-paths.sh
source "$script_dir/dependency-remediation-paths.sh"

for safe in "src/example.rs" "Cargo.lock" "tests/unit.test.ts" $'src/line\nbreak.rs'; do
  dependency_remediation_path_allowed "$safe" || {
    printf 'safe path was rejected: %q\n' "$safe" >&2
    exit 1
  }
done

for unsafe in "../outside.rs" "src/../outside.rs" "scripts/publish.sh" "/absolute.rs" "src\\outside.rs" "src//empty-component.rs" "src/trailing/"; do
  if dependency_remediation_path_allowed "$unsafe"; then
    printf 'unsafe path was accepted: %q\n' "$unsafe" >&2
    exit 1
  fi
done

temporary="$(mktemp -d)"
trap 'rm -rf -- "$temporary"' EXIT
git -C "$temporary" init -q
git -C "$temporary" config user.name Test
git -C "$temporary" config user.email test@example.invalid
printf 'base\n' > "$temporary/base.txt"
git -C "$temporary" add base.txt
git -C "$temporary" commit -qm base
source_sha="$(git -C "$temporary" rev-parse HEAD)"
newline_path=$'src/line\nbreak.rs'
mkdir -p "$temporary/src"
printf 'content\n' > "$temporary/$newline_path"
git -C "$temporary" add -- "$newline_path"
actual="$(git -C "$temporary" diff --cached --name-only -z "$source_sha" | jq -Rs 'split("\u0000") | map(select(length > 0)) | sort')"
expected="$(jq -cn --arg path "$newline_path" '[$path]')"
jq -en --argjson expected "$expected" --argjson actual "$actual" '$expected == $actual' >/dev/null

result="$temporary/result.json"
jq -n --arg path "$newline_path" '{changed_files:[$path]}' > "$result"
dependency_remediation_validate_result_paths "$result"
jq -n '{changed_files:["src/../outside.rs"]}' > "$result"
if dependency_remediation_validate_result_paths "$result"; then
  echo "path-traversal inventory was accepted" >&2
  exit 1
fi

echo "dependency remediation path and NUL inventory tests passed"
