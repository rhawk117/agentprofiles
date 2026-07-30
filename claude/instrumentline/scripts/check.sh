#!/usr/bin/env bash
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

require_rust_toolchain
enter_project_root

"${SCRIPT_DIRECTORY}/fmt.sh" --check
"${SCRIPT_DIRECTORY}/lint.sh"
"${SCRIPT_DIRECTORY}/test.sh"

announce_stage "release build"
cargo build --release --locked || fail_with_message "release build failed"

BINARY_PATH="${PROJECT_ROOT}/target/release/instrumentline"
[[ -x ${BINARY_PATH} ]] || fail_with_message "release binary missing at ${BINARY_PATH}"

announce_stage "startup budget"
"${SCRIPT_DIRECTORY}/bench.sh" --quiet

announce_success "full gate passed"
