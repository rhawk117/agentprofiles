#!/usr/bin/env bash
set -uo pipefail

input=$(cat)

emit_ask() {
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask","permissionDecisionReason":"%s"}}' "$1"
  exit 0
}

if command -v jq >/dev/null 2>&1; then
  cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // empty' 2>/dev/null) || cmd=$'\x00PARSEFAIL'
elif command -v python3 >/dev/null 2>&1; then
  cmd=$(printf '%s' "$input" | python3 -c 'import json,sys
try:
    print(json.loads(sys.stdin.read(), strict=False).get("tool_input", {}).get("command", ""), end="")
except Exception:
    raise SystemExit(3)' 2>/dev/null) || cmd=$'\x00PARSEFAIL'
else
  cmd=$'\x00PARSEFAIL'
fi

[ "$cmd" = $'\x00PARSEFAIL' ] && emit_ask "protected-paths-guard could not parse hook input (jq and python3 both unavailable or malformed JSON). Failing closed."
[ -z "$cmd" ] && exit 0

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

if printf '%s' "$cmd" | grep -qE "$secret_paths"; then
  emit_ask "Command references a protected credential path. Approve only if this access is intended."
fi

if printf '%s' "$cmd" | grep -qE "$env_dump"; then
  emit_ask "Command dumps the process environment, which may expose injected secrets. Approve only if intended."
fi

if printf '%s' "$cmd" | grep -qE '\|[[:space:]]*(sudo[[:space:]]+)?(ba|z|k|da|)sh([[:space:]]|$)'; then
  emit_ask "Command pipes downloaded content directly into a shell. Inspect the payload before approving."
fi

exit 0