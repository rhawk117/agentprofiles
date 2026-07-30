#!/usr/bin/env bash
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

require_rust_toolchain
enter_project_root

announce_stage "cargo test (all targets)"
cargo test --all-targets --all-features --locked "$@" ||
    fail_with_message "tests failed"
announce_success "tests passed"
