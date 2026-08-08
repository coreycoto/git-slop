#!/usr/bin/env bash
set -euo pipefail

changed=()
while IFS= read -r file; do
  changed+=("$file")
done < <(git diff --name-only --merge-base HEAD origin/main 2>/dev/null || git diff --name-only HEAD)
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
