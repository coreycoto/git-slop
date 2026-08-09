#!/usr/bin/env bash
set -euo pipefail

changed=()
base_ref="${VERIFY_CHANGED_BASE:-}"
if [[ -z "$base_ref" ]]; then
  base_ref="$(git rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null || true)"
fi
if [[ -z "$base_ref" ]] && git rev-parse --verify --quiet origin/main >/dev/null; then
  base_ref="origin/main"
fi
if [[ -n "$base_ref" ]] && merge_base="$(git merge-base HEAD "$base_ref" 2>/dev/null)"; then
  changed_source=(git diff --name-only "$merge_base")
elif git rev-parse --verify --quiet HEAD^ >/dev/null; then
  changed_source=(git diff --name-only HEAD^)
else
  changed_source=(git diff --name-only HEAD)
fi
while IFS= read -r file; do
  changed+=("$file")
done < <(
  {
    "${changed_source[@]}"
    git ls-files --others --exclude-standard
  } | LC_ALL=C sort -u
)
if ((${#changed[@]} == 0)); then
  echo "No changed files detected."
  exit 0
fi

matches() {
  local pattern="$1"
  printf '%s\n' "${changed[@]}" | grep -Eq "$pattern"
}

if matches '^(Cargo\.(toml|lock)|build\.rs|src/|tests/)'; then
  cargo fmt -p git-slop -- --check
  cargo clippy -p git-slop --all-targets --all-features --locked -- -D warnings
  cargo test -p git-slop --all-targets --all-features --locked
fi

if matches '^(xtask/|\.codex/|\.agents/|plugins/|config/|\.github/)'; then
  cargo fmt --manifest-path xtask/Cargo.toml --all -- --check
  cargo clippy --manifest-path xtask/Cargo.toml --all-targets --all-features --locked -- -D warnings
  cargo test --manifest-path xtask/Cargo.toml --all-targets --all-features --locked
  cargo xtask validate
fi

if matches '^(action/|action\.yml$)'; then
  node --test action/*.test.mjs
fi
