#!/usr/bin/env bash
set -Eeuo pipefail

STATE_DIRECTORY="${INSTRUMENTLINE_STATE_DIR:-${HOME}/.claude/instrumentline-state}"

PAYLOAD="$(cat)"

extract_field() {
    local field="$1"
    printf '%s' "${PAYLOAD}" |
        sed -n "s/.*\"${field}\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" |
        head -n 1
}

SESSION_ID="$(extract_field session_id)"
PERMISSION_MODE="$(extract_field permission_mode)"

[[ -n ${SESSION_ID} ]] || exit 0
[[ -n ${PERMISSION_MODE} ]] || exit 0

SAFE_SESSION_ID="${SESSION_ID//[^a-zA-Z0-9-]/}"
[[ -n ${SAFE_SESSION_ID} ]] || exit 0

mkdir -p "${STATE_DIRECTORY}"
printf '%s' "${PERMISSION_MODE}" >"${STATE_DIRECTORY}/${SAFE_SESSION_ID}.mode.tmp"
mv "${STATE_DIRECTORY}/${SAFE_SESSION_ID}.mode.tmp" "${STATE_DIRECTORY}/${SAFE_SESSION_ID}.mode"
