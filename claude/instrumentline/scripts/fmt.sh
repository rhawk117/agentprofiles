#!/usr/bin/env bash
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

CHECK_ONLY=0
for argument in "$@"; do
    case "${argument}" in
        --check) CHECK_ONLY=1 ;;
        -h | --help)
            printf 'usage: fmt.sh [--check]\n\n  --check  verify formatting without rewriting files\n'
            exit 0
            ;;
        *) fail_with_message "unknown argument: ${argument}" ;;
    esac
done

require_rust_toolchain
enter_project_root

if [[ ${CHECK_ONLY} -eq 1 ]]; then
    announce_stage "rustfmt (check only)"
    cargo fmt --all -- --check ||
        fail_with_message "formatting drift detected: run scripts/fmt.sh to fix"
    announce_success "formatting is clean"
else
    announce_stage "rustfmt (rewriting)"
    cargo fmt --all
    announce_success "formatting applied"
fi

if command -v shfmt >/dev/null 2>&1; then
    announce_stage "shfmt"
    if [[ ${CHECK_ONLY} -eq 1 ]]; then
        shfmt --diff --indent 4 --case-indent "${PROJECT_ROOT}/scripts" ||
            fail_with_message "shell formatting drift detected"
    else
        shfmt --write --indent 4 --case-indent "${PROJECT_ROOT}/scripts"
    fi
    announce_success "shell scripts formatted"
else
    announce_detail "shfmt not installed, skipping shell formatting"
fi
