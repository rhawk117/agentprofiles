#!/usr/bin/env bash
# Install this repo's claude/ tree onto ~/.claude by symlink, so edits here take
# effect without reinstalling. Additive: content that exists only in ~/.claude is
# left alone. Everything replaced is copied into a timestamped backup first.
#
#   ./install.sh              smoke test, back up, link
#   ./install.sh --check      audit only, no writes
#   ./install.sh --skip-tests skip the smoke suite (not recommended)
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$REPO/claude"
DEST="${CLAUDE_HOME:-$HOME/.claude}"

# Individually linked; a whole-directory link would hide global-only content.
MANAGED_FILES=(settings.json CLAUDE.md model-rates.json bin/statusline.py)
MANAGED_DIRS=(bin/hooks skills agents)

# Superseded by bin/hooks/*: hyphenated names, no longer referenced by settings.json.
LEGACY_HOOKS=(
  hooks/notify.sh
  hooks/protected-paths-guard.sh
  hooks/ruff-on-edit.sh
  hooks/session-context.sh
)

BOLD=$'\033[1m' GREEN=$'\033[32m' YELLOW=$'\033[33m' RED=$'\033[31m' GRAY=$'\033[90m' RESET=$'\033[0m'
[ -t 1 ] || { BOLD= GREEN= YELLOW= RED= GRAY= RESET=; }

check_only=0
skip_tests=0
for arg in "$@"; do
  case "$arg" in
    --check) check_only=1 ;;
    --skip-tests) skip_tests=1 ;;
    -h | --help)
      sed -n '2,9p' "${BASH_SOURCE[0]}" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      printf '%sunknown argument: %s%s\n' "$RED" "$arg" "$RESET" >&2
      exit 2
      ;;
  esac
done

# Every path this installer manages, relative to both trees.
manifest() {
  printf '%s\n' "${MANAGED_FILES[@]}"
  local dir entry
  for dir in "${MANAGED_DIRS[@]}"; do
    [ -d "$SRC/$dir" ] || continue
    for entry in "$SRC/$dir"/*; do
      [ -e "$entry" ] || continue
      printf '%s/%s\n' "$dir" "$(basename "$entry")"
    done
  done
}

# ok | broken | foreign | replaced | missing
status_of() {
  local rel="$1" link="$DEST/$1" want="$SRC/$1"
  if [ -L "$link" ]; then
    [ "$(readlink -- "$link")" = "$want" ] || { echo foreign; return; }
    [ -e "$link" ] && echo ok || echo broken
  elif [ -e "$link" ]; then
    echo replaced
  else
    echo missing
  fi
}

# --------------------------------------------------------------------------
if [ "$check_only" -eq 1 ]; then
  printf '%s%s -> %s%s\n\n' "$BOLD" "$SRC" "$DEST" "$RESET"
  problems=0
  while read -r rel; do
    case "$(status_of "$rel")" in
      ok) printf '  %sok%s        %s\n' "$GREEN" "$RESET" "$rel" ;;
      broken)
        printf '  %sbroken%s    %s %s(symlink target is gone)%s\n' "$RED" "$RESET" "$rel" "$GRAY" "$RESET"
        problems=$((problems + 1))
        ;;
      foreign)
        printf '  %sforeign%s   %s %s(-> %s)%s\n' "$YELLOW" "$RESET" "$rel" "$GRAY" "$(readlink -- "$DEST/$rel")" "$RESET"
        problems=$((problems + 1))
        ;;
      replaced)
        printf '  %sreplaced%s  %s %s(regular file, not a link)%s\n' "$YELLOW" "$RESET" "$rel" "$GRAY" "$RESET"
        problems=$((problems + 1))
        ;;
      missing)
        printf '  %smissing%s   %s\n' "$GRAY" "$RESET" "$rel"
        problems=$((problems + 1))
        ;;
    esac
  done < <(manifest)

  for rel in "${LEGACY_HOOKS[@]}"; do
    [ -e "$DEST/$rel" ] && printf '  %slegacy%s    %s %s(superseded, install will retire it)%s\n' \
      "$YELLOW" "$RESET" "$rel" "$GRAY" "$RESET"
  done

  if [ "$problems" -eq 0 ]; then
    printf '\n%severything is linked%s\n' "$GREEN" "$RESET"
    exit 0
  fi
  printf '\n%s%d path(s) need installing%s\n' "$YELLOW" "$problems" "$RESET"
  exit 1
fi

# --------------------------------------------------------------------------
command -v python3 >/dev/null 2>&1 || {
  printf '%spython3 is required by the hooks and the statusline%s\n' "$RED" "$RESET" >&2
  exit 1
}

if [ "$skip_tests" -eq 0 ]; then
  printf '%srunning smoke tests%s\n' "$BOLD" "$RESET"
  bash "$REPO/tests/smoke.sh" || {
    printf '\n%ssmoke tests failed; nothing was installed%s\n' "$RED" "$RESET" >&2
    exit 1
  }
  printf '\n'
fi

BACKUP="$DEST/backups/agentprofiles-$(date +%Y%m%d-%H%M%S)"
backed_up=0

# Copy a path into the backup tree, preserving its position under ~/.claude.
preserve() {
  local rel="$1"
  mkdir -p "$BACKUP/$(dirname "$rel")"
  cp -a "$DEST/$rel" "$BACKUP/$rel"
  backed_up=$((backed_up + 1))
}

printf '%sinstalling %s -> %s%s\n' "$BOLD" "$SRC" "$DEST" "$RESET"
linked=0
unchanged=0

while read -r rel; do
  status="$(status_of "$rel")"
  if [ "$status" = ok ]; then
    unchanged=$((unchanged + 1))
    continue
  fi
  # A broken or foreign symlink has nothing worth keeping; a real file does.
  [ "$status" = replaced ] && preserve "$rel"
  mkdir -p "$DEST/$(dirname "$rel")"
  ln -sfn "$SRC/$rel" "$DEST/$rel"
  printf '  %slink%s  %s\n' "$GREEN" "$RESET" "$rel"
  linked=$((linked + 1))
done < <(manifest)

retired=0
for rel in "${LEGACY_HOOKS[@]}"; do
  [ -e "$DEST/$rel" ] || continue
  preserve "$rel"
  rm -f "$DEST/$rel"
  printf '  %sretire%s %s\n' "$YELLOW" "$RESET" "$rel"
  retired=$((retired + 1))
done
# Only if the legacy hooks were all it held.
[ -d "$DEST/hooks" ] && rmdir "$DEST/hooks" 2>/dev/null || true

printf '\n%s%d linked, %d already correct, %d legacy hook(s) retired%s\n' \
  "$BOLD" "$linked" "$unchanged" "$retired" "$RESET"
if [ "$backed_up" -gt 0 ]; then
  printf '%s%d file(s) backed up to %s%s\n' "$GRAY" "$backed_up" "$BACKUP" "$RESET"
fi
printf '%sglobal-only skills and agents were left untouched; run --check to audit%s\n' "$GRAY" "$RESET"
