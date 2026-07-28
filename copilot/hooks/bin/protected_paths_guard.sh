#!/usr/bin/env bash
# Copilot `preToolUse` hook: keep credential files and environment dumps behind a
# prompt. Port of claude/bin/hooks/protected_paths_guard.sh.
#
# Three differences from the Claude original, all forced by Copilot's contract:
#
#   1. Output is flat. Copilot reads permissionDecision at the top level; the
#      hookSpecificOutput wrapper Claude uses is silently ignored here.
#   2. Payload keys are camelCase: toolName / toolArgs.
#   3. Two tiers. Claude expresses its credential list as 25 permissions.deny
#      globs in settings.json, but Copilot's permissions object only supports
#      disableBypassPermissionsMode -- so the deny list has to live here. Literal
#      credential paths deny; the fuzzier Bash heuristics only ask.
#
# Every path exits 0, and there is no `set -e`. This is load-bearing: preToolUse
# treats a non-zero exit as a denial regardless of what stdout said, so a guard
# that crashes -- or that lets a grep non-match propagate -- blocks every tool
# call for the rest of the session.
set -uo pipefail

input=$(cat)

emit() {
  printf '{"permissionDecision":"%s","permissionDecisionReason":"%s"}' "$1" "$2"
  exit 0
}

# Both the command string and any path argument, tab-separated. Copilot's own
# key spellings are tried first, Claude's accepted as a fallback because the
# payload shape has not been observed live.
if command -v jq >/dev/null 2>&1; then
  fields=$(printf '%s' "$input" | jq -r '
    (.toolArgs // .tool_input // {}) as $a
    | [($a.command // ""), ($a.path // $a.file_path // $a.filePath // "")]
    | @tsv' 2>/dev/null) || fields=$'\x00PARSEFAIL'
elif command -v python3 >/dev/null 2>&1; then
  fields=$(printf '%s' "$input" | python3 -c 'import json,sys
try:
    d = json.loads(sys.stdin.read(), strict=False)
except Exception:
    raise SystemExit(3)
if not isinstance(d, dict):
    raise SystemExit(3)
a = d.get("toolArgs") or d.get("tool_input") or {}
if not isinstance(a, dict):
    a = {}
def s(*keys):
    for k in keys:
        v = a.get(k)
        if isinstance(v, str):
            return v.replace("\t", " ").replace("\n", " ")
    return ""
print(s("command") + "\t" + s("path", "file_path", "filePath"), end="")' 2>/dev/null) || fields=$'\x00PARSEFAIL'
else
  fields=$'\x00PARSEFAIL'
fi

[ "$fields" = $'\x00PARSEFAIL' ] &&
  emit ask "protected-paths-guard could not parse hook input (jq and python3 both unavailable, or malformed JSON). Failing closed."

IFS=$'\t' read -r cmd path <<<"$fields"
cmd=${cmd:-}
path=${path:-}
[ -z "$cmd$path" ] && exit 0

secret_paths='(^|[^A-Za-z0-9_.-])\.env([^A-Za-z0-9_-]|$)'
secret_paths+='|\.env\.|/secrets?/|credentials\.json'
secret_paths+='|id_rsa|id_ed25519|id_ecdsa|id_dsa'
secret_paths+='|\.(pem|key|p12|pfx|jks|keystore|ovpn|kdbx|asc|gpg)([^A-Za-z0-9]|$)'
secret_paths+='|\.netrc|\.npmrc|\.pypirc|\.pgpass|\.git-credentials|\.htpasswd'
secret_paths+='|\.aws/|\.kube/|kubeconfig|\.ssh/|\.gnupg/|\.docker/config\.json'
secret_paths+='|terraform\.tfstate|\.terraform/|\.vault-token|service[-_]account.*\.json'

env_dump='(^|[;&|`(]|\$\()[[:space:]]*(env|printenv|set)[[:space:]]*($|[;&|)])'
env_dump+='|declare[[:space:]]+-[xp]'
env_dump+='|/proc/[0-9]+/environ|/proc/self/environ'

if printf '%s %s' "$cmd" "$path" | grep -qE "$secret_paths"; then
  emit deny "Blocked: this touches a protected credential path. Rephrase without the secret, or read it yourself outside the session."
fi

if printf '%s' "$cmd" | grep -qE "$env_dump"; then
  emit ask "Command dumps the process environment, which may expose injected secrets. Approve only if intended."
fi

if printf '%s' "$cmd" | grep -qE '\|[[:space:]]*(sudo[[:space:]]+)?(ba|z|k|da|)sh([[:space:]]|$)'; then
  emit ask "Command pipes downloaded content directly into a shell. Inspect the payload before approving."
fi

exit 0
