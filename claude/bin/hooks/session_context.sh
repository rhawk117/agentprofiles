#!/usr/bin/env bash
set -uo pipefail

git rev-parse --is-inside-work-tree >/dev/null 2>&1 || exit 0

branch=$(git branch --show-current 2>/dev/null)
[ -z "$branch" ] && branch="(detached $(git rev-parse --short HEAD 2>/dev/null))"

upstream=$(git rev-parse --abbrev-ref --symbolic-full-name '@{u}' 2>/dev/null || true)
sync=""
if [ -n "$upstream" ]; then
  counts=$(git rev-list --left-right --count "HEAD...@{u}" 2>/dev/null || echo "0	0")
  ahead=${counts%%	*}
  behind=${counts##*	}
  [ "${ahead:-0}" -gt 0 ] && sync=" ahead ${ahead}"
  [ "${behind:-0}" -gt 0 ] && sync="${sync} behind ${behind}"
  [ -n "$sync" ] && sync=" |${sync} vs ${upstream}"
fi

staged=$(git diff --cached --name-only 2>/dev/null | wc -l | tr -d ' ')
modified=$(git diff --name-only 2>/dev/null | wc -l | tr -d ' ')
untracked=$(git ls-files --others --exclude-standard 2>/dev/null | wc -l | tr -d ' ')

echo "Repo state: ${branch}${sync} | staged ${staged}, modified ${modified}, untracked ${untracked}"

if [ "$((staged + modified))" -gt 0 ] && [ "$((staged + modified))" -le 12 ]; then
  echo "Changed files:"
  git diff --name-status HEAD 2>/dev/null | head -12 | sed 's/^/  /'
fi

echo "Recent commits:"
git log --oneline --no-decorate -5 2>/dev/null | sed 's/^/  /'

stash=$(git stash list 2>/dev/null | wc -l | tr -d ' ')
[ "${stash:-0}" -gt 0 ] && echo "Stashes: ${stash} (not applied)"

exit 0