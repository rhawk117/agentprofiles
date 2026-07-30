#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIRECTORY="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "${SCRIPT_DIRECTORY}/.." && pwd)"
export SCRIPT_DIRECTORY PROJECT_ROOT
readonly SCRIPT_DIRECTORY PROJECT_ROOT

if [[ -t 1 ]]; then
    readonly ANSI_BOLD=$'\033[1m'
    readonly ANSI_GREEN=$'\033[32m'
    readonly ANSI_RED=$'\033[31m'
    readonly ANSI_CYAN=$'\033[36m'
    readonly ANSI_DIM=$'\033[2m'
    readonly ANSI_RESET=$'\033[0m'
else
    readonly ANSI_BOLD='' ANSI_GREEN='' ANSI_RED='' ANSI_CYAN='' ANSI_DIM='' ANSI_RESET=''
fi

announce_stage() {
    printf '%s\n' "${ANSI_CYAN}${ANSI_BOLD}==>${ANSI_RESET}${ANSI_BOLD} $*${ANSI_RESET}" >&2
}

announce_detail() {
    printf '%s\n' "${ANSI_DIM}    $*${ANSI_RESET}" >&2
}

announce_success() {
    printf '%s\n' "${ANSI_GREEN}${ANSI_BOLD} ok ${ANSI_RESET} $*" >&2
}

fail_with_message() {
    printf '%s\n' "${ANSI_RED}${ANSI_BOLD}fail${ANSI_RESET} $*" >&2
    exit 1
}

require_command() {
    local required_command="$1"
    command -v "${required_command}" >/dev/null 2>&1 ||
        fail_with_message "required command not found on PATH: ${required_command}"
}

enter_project_root() {
    cd "${PROJECT_ROOT}" || fail_with_message "cannot enter project root: ${PROJECT_ROOT}"
}

require_rust_toolchain() {
    require_command cargo
    require_command rustc
    cargo fmt --version >/dev/null 2>&1 ||
        fail_with_message "rustfmt component missing: run 'rustup component add rustfmt'"
    cargo clippy --version >/dev/null 2>&1 ||
        fail_with_message "clippy component missing: run 'rustup component add clippy'"
}
