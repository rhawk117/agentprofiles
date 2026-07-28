#!/usr/bin/env bash
# Dispatcher over the per-tree smoke suites. SMOKE_TREE selects which run:
# claude, copilot, or all (the default).
#
# install.sh gates on this, so a red assertion in one tree must not block
# installing the other. That is the whole reason the suites are separate files.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tree="${SMOKE_TREE:-all}"

case "$tree" in
  claude) suites=(smoke-claude.sh) ;;
  copilot) suites=(smoke-copilot.sh) ;;
  all) suites=(smoke-claude.sh smoke-copilot.sh) ;;
  *)
    printf 'smoke.sh: unknown SMOKE_TREE %s (want claude, copilot or all)\n' "$tree" >&2
    exit 2
    ;;
esac

status=0
for suite in "${suites[@]}"; do
  bash "$REPO/tests/$suite" || status=1
done
exit "$status"
