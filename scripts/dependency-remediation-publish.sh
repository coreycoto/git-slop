#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  printf '%s\n' 'usage: dependency-remediation-publish.sh prepare|push|create-pr|verify-pr|continue PACKAGE_DIR' >&2
  exit 2
fi

trusted_root="${GH_STEWARD_TRUSTED_ROOT:?trusted control checkout is required}"
xtask_bin="${GIT_SLOP_XTASK_BIN:?prebuilt trusted xtask binary is required}"
[[ -x "$xtask_bin" ]] || { echo 'trusted xtask binary is not executable' >&2; exit 1; }
exec "$xtask_bin" --repo-root "$trusted_root" dependency-remediation publish "$1" "$2"
