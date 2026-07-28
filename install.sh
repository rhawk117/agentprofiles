#!/usr/bin/env bash
# Install this repo's harness trees onto their config directories by symlink, so
# edits here take effect without reinstalling. Additive: content that exists only
# in the config directory is left alone. Everything replaced is copied into a
# timestamped backup first.
#
#   ./install.sh                    smoke test, back up, link both trees
#   ./install.sh --tree claude      only ~/.claude (also: copilot, all)
#   ./install.sh --check            audit only, no writes
#   ./install.sh --print-manifest   list the managed paths and exit
#   ./install.sh --skip-tests       skip the smoke suite (not recommended)
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

BOLD=$'\033[1m' GREEN=$'\033[32m' YELLOW=$'\033[33m' RED=$'\033[31m' GRAY=$'\033[90m' RESET=$'\033[0m'
[ -t 1 ] || { BOLD= GREEN= YELLOW= RED= GRAY= RESET=; }

check_only=0
skip_tests=0
print_manifest=0
tree=all
while [ $# -gt 0 ]; do
  case "$1" in
    --check) check_only=1 ;;
    --skip-tests) skip_tests=1 ;;
    --print-manifest) print_manifest=1 ;;
    --tree)
      shift
      tree="${1:-}"
      ;;
    --tree=*) tree="${1#--tree=}" ;;
    -h | --help)
      sed -n '2,12p' "${BASH_SOURCE[0]}" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      printf '%sunknown argument: %s%s\n' "$RED" "$1" "$RESET" >&2
      exit 2
      ;;
  esac
  shift
done

case "$tree" in
  claude) TREES=(claude) ;;
  copilot) TREES=(copilot) ;;
  all) TREES=(claude copilot) ;;
  *)
    printf '%sunknown tree: %s (want claude, copilot or all)%s\n' "$RED" "$tree" "$RESET" >&2
    exit 2
    ;;
esac

# Per-tree configuration. A `case` rather than `declare -n` namerefs, which need
# bash 4.3 and macOS still ships 3.2.
select_tree() {
  case "$1" in
    claude)
      SRC="$REPO/claude"
      DEST="${CLAUDE_HOME:-$HOME/.claude}"
      # Individually linked; a whole-directory link would hide global-only content.
      MANAGED_FILES=(settings.json CLAUDE.md model-rates.json bin/statusline.py)
      MANAGED_DIRS=(bin/hooks skills agents)
      # Superseded by bin/hooks/*: hyphenated names, no longer in settings.json.
      LEGACY_HOOKS=(
        hooks/notify.sh
        hooks/protected-paths-guard.sh
        hooks/ruff-on-edit.sh
        hooks/session-context.sh
      )
      ;;
    copilot)
      SRC="$REPO/copilot"
      DEST="${COPILOT_HOME:-$HOME/.copilot}"
      # copilot-instructions.md is deliberately absent: it is 0 bytes here, and
      # linking it over a real one would blank the user's instructions.
      MANAGED_FILES=(settings.json model-rates.json statusline.py)
      # Named exactly, never the config-dir root: ~/.copilot is a live product
      # directory holding session-store.db, session-state/ and logs/.
      MANAGED_DIRS=(hooks hooks/bin agents skills)
      LEGACY_HOOKS=()
      ;;
  esac
}

# Every path this installer manages, relative to both trees.
manifest() {
  printf '%s\n' "${MANAGED_FILES[@]}"
  local dir entry name
  for dir in "${MANAGED_DIRS[@]}"; do
    [ -d "$SRC/$dir" ] || continue
    for entry in "$SRC/$dir"/*; do
      [ -e "$entry" ] || continue
      name="$(basename "$entry")"
      # Byte-compiled output is not content, and a directory entry would get
      # symlinked wholesale into the config directory.
      [ "$name" = __pycache__ ] && continue
      # A directory listed in MANAGED_DIRS is enumerated in its own pass.
      [ -d "$entry" ] && printf '%s\n' "${MANAGED_DIRS[@]}" | grep -qxF "$dir/$name" && continue
      printf '%s/%s\n' "$dir" "$name"
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
if [ "$print_manifest" -eq 1 ]; then
  for t in "${TREES[@]}"; do
    select_tree "$t"
    while read -r rel; do printf '%s\t%s\n' "$t" "$rel"; done < <(manifest)
  done
  exit 0
fi

if [ "$check_only" -eq 1 ]; then
  problems=0
  for t in "${TREES[@]}"; do
    select_tree "$t"
    printf '%s%s -> %s%s\n' "$BOLD" "$SRC" "$DEST" "$RESET"
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

    for rel in ${LEGACY_HOOKS[@]+"${LEGACY_HOOKS[@]}"}; do
      [ -e "$DEST/$rel" ] && printf '  %slegacy%s    %s %s(superseded, install will retire it)%s\n' \
        "$YELLOW" "$RESET" "$rel" "$GRAY" "$RESET"
    done
    printf '\n'
  done

  if [ "$problems" -eq 0 ]; then
    printf '%severything is linked%s\n' "$GREEN" "$RESET"
    exit 0
  fi
  printf '%s%d path(s) need installing%s\n' "$YELLOW" "$problems" "$RESET"
  exit 1
fi

# --------------------------------------------------------------------------
command -v python3 >/dev/null 2>&1 || {
  printf '%spython3 is required by the hooks and the statusline%s\n' "$RED" "$RESET" >&2
  exit 1
}

if [ "$skip_tests" -eq 0 ]; then
  printf '%srunning smoke tests%s\n' "$BOLD" "$RESET"
  # Only the trees being installed, so a red assertion in one cannot block the other.
  SMOKE_TREE="$tree" bash "$REPO/tests/smoke.sh" || {
    printf '\n%ssmoke tests failed; nothing was installed%s\n' "$RED" "$RESET" >&2
    exit 1
  }
  printf '\n'
fi

stamp="$(date +%Y%m%d-%H%M%S)"
linked=0
unchanged=0
retired=0
backed_up=0

for t in "${TREES[@]}"; do
  select_tree "$t"
  BACKUP="$DEST/backups/agentprofiles-$stamp"

  # Copy a path into the backup tree, preserving its position under the config dir.
  preserve() {
    local rel="$1"
    mkdir -p "$BACKUP/$(dirname "$rel")"
    cp -a "$DEST/$rel" "$BACKUP/$rel"
    backed_up=$((backed_up + 1))
  }

  printf '%sinstalling %s -> %s%s\n' "$BOLD" "$SRC" "$DEST" "$RESET"

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

  for rel in ${LEGACY_HOOKS[@]+"${LEGACY_HOOKS[@]}"}; do
    [ -e "$DEST/$rel" ] || continue
    preserve "$rel"
    rm -f "$DEST/$rel"
    printf '  %sretire%s %s\n' "$YELLOW" "$RESET" "$rel"
    retired=$((retired + 1))
  done
  # Only if the legacy hooks were all it held.
  [ "$t" = claude ] && [ -d "$DEST/hooks" ] && rmdir "$DEST/hooks" 2>/dev/null || true
  printf '\n'
done

printf '%s%d linked, %d already correct, %d legacy hook(s) retired%s\n' \
  "$BOLD" "$linked" "$unchanged" "$retired" "$RESET"
if [ "$backed_up" -gt 0 ]; then
  printf '%s%d file(s) backed up%s\n' "$GRAY" "$backed_up" "$RESET"
fi
printf '%sglobal-only skills and agents were left untouched; run --check to audit%s\n' "$GRAY" "$RESET"
