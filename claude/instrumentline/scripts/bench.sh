#!/usr/bin/env bash
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

QUIET=0
ITERATIONS=200
BUDGET_MILLISECONDS=15

for argument in "$@"; do
    case "${argument}" in
        --quiet) QUIET=1 ;;
        --iterations=*) ITERATIONS="${argument#*=}" ;;
        --budget=*) BUDGET_MILLISECONDS="${argument#*=}" ;;
        -h | --help)
            printf 'usage: bench.sh [--quiet] [--iterations=N] [--budget=MILLISECONDS]\n'
            exit 0
            ;;
        *) fail_with_message "unknown argument: ${argument}" ;;
    esac
done

enter_project_root
BINARY_PATH="${PROJECT_ROOT}/target/release/instrumentline"
[[ -x ${BINARY_PATH} ]] || cargo build --release --locked --quiet

PAYLOAD_PATH="${PROJECT_ROOT}/tests/fixtures/cruising.json"
[[ -f ${PAYLOAD_PATH} ]] || fail_with_message "fixture missing: ${PAYLOAD_PATH}"

STATE_DIRECTORY="$(mktemp -d)"
trap 'rm -rf "${STATE_DIRECTORY}"' EXIT

announce_stage "measuring ${ITERATIONS} invocations"
START_NANOSECONDS=$(date +%s%N)
for _ in $(seq 1 "${ITERATIONS}"); do
    INSTRUMENTLINE_STATE_DIR="${STATE_DIRECTORY}" COLUMNS=120 \
        "${BINARY_PATH}" render <"${PAYLOAD_PATH}" >/dev/null
done
END_NANOSECONDS=$(date +%s%N)

TOTAL_MILLISECONDS=$(((END_NANOSECONDS - START_NANOSECONDS) / 1000000))
PER_INVOCATION_MICROSECONDS=$(((END_NANOSECONDS - START_NANOSECONDS) / ITERATIONS / 1000))
PER_INVOCATION_MILLISECONDS=$((PER_INVOCATION_MICROSECONDS / 1000))

if [[ ${QUIET} -eq 0 ]]; then
    printf '  total       %s ms over %s runs\n' "${TOTAL_MILLISECONDS}" "${ITERATIONS}"
fi
printf '  per run     %s.%03d ms (budget %s ms)\n' \
    "${PER_INVOCATION_MILLISECONDS}" "$((PER_INVOCATION_MICROSECONDS % 1000))" \
    "${BUDGET_MILLISECONDS}"

if ((PER_INVOCATION_MICROSECONDS > BUDGET_MILLISECONDS * 1000)); then
    fail_with_message "startup budget exceeded"
fi
announce_success "within startup budget"
