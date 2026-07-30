#!/usr/bin/env bash
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

enter_project_root
BINARY_PATH="${PROJECT_ROOT}/target/release/instrumentline"
[[ -x ${BINARY_PATH} ]] || cargo build --release --locked --quiet

TERMINAL_COLUMNS="${COLUMNS:-100}"
STATE_DIRECTORY="$(mktemp -d)"
trap 'rm -rf "${STATE_DIRECTORY}"' EXIT

for fixture_path in "${PROJECT_ROOT}"/tests/fixtures/*.json; do
    fixture_name="$(basename -- "${fixture_path}" .json)"
    printf '\n%s%s%s\n' "${ANSI_BOLD}" "${fixture_name}" "${ANSI_RESET}"
    INSTRUMENTLINE_STATE_DIR="${STATE_DIRECTORY}" COLUMNS="${TERMINAL_COLUMNS}" \
        "${BINARY_PATH}" render <"${fixture_path}"
done
printf '\n'
