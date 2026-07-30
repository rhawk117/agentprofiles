#!/usr/bin/env bash
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

require_rust_toolchain
enter_project_root

announce_stage "clippy (all targets, all features, warnings are errors)"
cargo clippy --all-targets --all-features --locked -- -D warnings ||
    fail_with_message "clippy found issues"
announce_success "clippy is clean"

announce_stage "rustdoc link check"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --quiet ||
    fail_with_message "rustdoc emitted warnings"
announce_success "rustdoc is clean"

if command -v shellcheck >/dev/null 2>&1; then
    announce_stage "shellcheck"
    # The permission-mode hook now lives in claude/bin/hooks, linted by the
    # repository's own smoke suite rather than here.
    shellcheck --severity=style --external-sources --source-path=SCRIPTDIR "${PROJECT_ROOT}"/scripts/*.sh ||
        fail_with_message "shellcheck found issues"
    announce_success "shell scripts are clean"
else
    announce_detail "shellcheck not installed, skipping shell linting"
fi
