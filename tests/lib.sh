#!/usr/bin/env bash
# Assertion helpers shared by the per-tree smoke suites. Sourced, never run.
# Each suite keeps its own `failures` count and ends with `finish <name>`.

failures=0
ok() { printf '  \033[32mok\033[0m    %s\n' "$1"; }
bad() {
  printf '  \033[31mFAIL\033[0m  %s\n' "$1"
  [ $# -gt 1 ] && printf '        %s\n' "$2"
  failures=$((failures + 1))
}
group() { printf '\n\033[1m%s\033[0m\n' "$1"; }

has() { case "$2" in *"$1"*) return 0 ;; *) return 1 ;; esac; }

assert_has() { # name haystack needle
  if has "$3" "$2"; then ok "$1"; else bad "$1" "expected to find: $3"; fi
}
assert_lacks() {
  if has "$3" "$2"; then bad "$1" "did not expect: $3"; else ok "$1"; fi
}
assert_eq() {
  if [ "$2" = "$3" ]; then ok "$1"; else bad "$1" "got '$2', want '$3'"; fi
}

finish() { # tree-name
  if [ "$failures" -eq 0 ]; then
    printf '\n\033[32mall %s smoke tests passed\033[0m\n' "$1"
    exit 0
  fi
  printf '\n\033[31m%d %s smoke test(s) failed\033[0m\n' "$failures" "$1"
  exit 1
}
