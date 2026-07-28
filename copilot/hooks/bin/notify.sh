#!/usr/bin/env bash
# Copilot `notification` hook: raise a desktop notification, falling back to the
# terminal bell. Port of claude/bin/hooks/notify.sh.
#
# The notification payload's key names are not documented, so a missing title or
# message degrades to a default rather than failing.
set -uo pipefail

input=$(cat)

if command -v jq >/dev/null 2>&1; then
  msg=$(printf '%s' "$input" | jq -r '.message // .text // empty' 2>/dev/null)
  title=$(printf '%s' "$input" | jq -r '.title // empty' 2>/dev/null)
elif command -v python3 >/dev/null 2>&1; then
  read -r -d '' msg title < <(printf '%s' "$input" | python3 -c 'import json,sys
try:
    d = json.loads(sys.stdin.read(), strict=False)
except Exception:
    d = {}
if not isinstance(d, dict):
    d = {}
print(d.get("message") or d.get("text") or "")
print(d.get("title") or "")
print("\0", end="")' 2>/dev/null) || true
fi

msg=$(printf '%s' "${msg:-}" | tr -d '\r\n' | head -c 200)
title=$(printf '%s' "${title:-}" | tr -d '\r\n' | head -c 60)
[ -z "$msg" ] && msg="Copilot CLI needs your attention"
[ -z "$title" ] && title="Copilot CLI"

export CC_NOTIFY_MSG="$msg" CC_NOTIFY_TITLE="$title"

notify_windows() {
  local ps=$1
  "$ps" -NoProfile -NonInteractive -Command '
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $n = New-Object System.Windows.Forms.NotifyIcon
    $n.Icon = [System.Drawing.SystemIcons]::Information
    $n.Visible = $true
    $n.ShowBalloonTip(5000, $env:CC_NOTIFY_TITLE, $env:CC_NOTIFY_MSG, [System.Windows.Forms.ToolTipIcon]::Info)
    Start-Sleep -Milliseconds 5500
    $n.Dispose()
  ' >/dev/null 2>&1 &
  disown 2>/dev/null || true
}

case "$(uname -s)" in
Linux*)
  if command -v wsl-notify-send.exe >/dev/null 2>&1; then
    wsl-notify-send.exe --category "$title" "$msg" >/dev/null 2>&1 &
  elif grep -qi microsoft /proc/version 2>/dev/null && command -v powershell.exe >/dev/null 2>&1; then
    notify_windows powershell.exe
  elif command -v notify-send >/dev/null 2>&1; then
    notify-send -u normal -a "Copilot CLI" "$title" "$msg" >/dev/null 2>&1 &
  else
    printf '\a' >/dev/tty 2>/dev/null || printf '\a'
  fi
  ;;
MINGW* | MSYS* | CYGWIN*)
  if command -v powershell >/dev/null 2>&1; then
    notify_windows powershell
  else
    printf '\a'
  fi
  ;;
Darwin*)
  osascript \
    -e 'on run argv' \
    -e 'display notification (item 1 of argv) with title (item 2 of argv)' \
    -e 'end run' \
    "$msg" "$title" >/dev/null 2>&1 &
  ;;
*)
  printf '\a'
  ;;
esac

exit 0
