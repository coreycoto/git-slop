#!/usr/bin/env bash
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
for test_script in \
  with-gh-steward.test.sh \
  prepare-codex-plugins.test.sh \
  recover-gh-steward-run.test.sh \
  acquire-gh-steward-history.test.sh \
  dependency-remediation-paths.test.sh; do
  bash "$script_dir/$test_script"
done
