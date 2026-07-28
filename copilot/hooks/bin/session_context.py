#!/usr/bin/env python3
"""Copilot session-context hook: put the repository's state in front of the model.

Port of claude/bin/hooks/session_context.sh, split into two phases because
Copilot's `sessionStart` stdout is only optionally consumed. The banner is
cached at session start and injected into the first prompt as a fallback, so it
lands whether or not the harness acted on the sessionStart output.

    sessionStart          --> session_context.py cache
    userPromptTransformed --> session_context.py inject

`userPromptSubmitted` cannot do the injection: its stdout is documented as not
processed. `userPromptTransformed` is the only prompt-stage event whose output
is applied, via `modifiedTransformedPrompt`.
"""

import json
import os
import subprocess
import sys
import time
from pathlib import Path

SENTINEL = "<repo_state"
PENDING_MAX_AGE = 2 * 24 * 3600
GIT_TIMEOUT = 3


def copilot_home() -> Path:
    return Path(os.environ.get("COPILOT_HOME") or Path.home() / ".copilot")


def pending_dir() -> Path:
    return copilot_home() / "session-context"


def slug(value: str) -> str:
    return "".join(c if c.isalnum() or c in "-_" else "_" for c in value)[:64] or "default"


def git(*args: str) -> str:
    try:
        out = subprocess.run(
            ["git", *args],
            capture_output=True,
            text=True,
            timeout=GIT_TIMEOUT,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return ""
    return out.stdout.strip() if out.returncode == 0 else ""


def count(*args: str) -> int:
    out = git(*args)
    return len(out.splitlines()) if out else 0


def repo_banner() -> str:
    """Empty outside a work tree, which is the signal to stay silent."""
    if git("rev-parse", "--is-inside-work-tree") != "true":
        return ""

    branch = git("branch", "--show-current")
    if not branch:
        branch = f"(detached {git('rev-parse', '--short', 'HEAD')})"

    sync = ""
    upstream = git("rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}")
    if upstream:
        counts = git("rev-list", "--left-right", "--count", "HEAD...@{u}").split()
        ahead, behind = (counts + ["0", "0"])[:2]
        parts = []
        if ahead.isdigit() and int(ahead):
            parts.append(f"ahead {ahead}")
        if behind.isdigit() and int(behind):
            parts.append(f"behind {behind}")
        if parts:
            sync = f" | {' '.join(parts)} vs {upstream}"

    staged = count("diff", "--cached", "--name-only")
    modified = count("diff", "--name-only")
    untracked = count("ls-files", "--others", "--exclude-standard")

    lines = [
        f"Repo state: {branch}{sync} | "
        f"staged {staged}, modified {modified}, untracked {untracked}"
    ]
    if 0 < staged + modified <= 12:
        changed = git("diff", "--name-status", "HEAD").splitlines()[:12]
        if changed:
            lines.append("Changed files:")
            lines += [f"  {line}" for line in changed]

    commits = git("log", "--oneline", "--no-decorate", "-5").splitlines()
    if commits:
        lines.append("Recent commits:")
        lines += [f"  {line}" for line in commits]

    stashes = count("stash", "list")
    if stashes:
        lines.append(f"Stashes: {stashes} (not applied)")

    body = "\n".join(lines)
    return f"{SENTINEL}>\n{body}\n</repo_state>"


def session_id(payload: dict) -> str:
    for key in ("sessionId", "session_id"):
        value = payload.get(key)
        if isinstance(value, str) and value.strip():
            return slug(value)
    return "default"


def read_payload() -> dict:
    try:
        payload = json.load(sys.stdin)
    except (ValueError, OSError):
        return {}
    return payload if isinstance(payload, dict) else {}


def prune(directory: Path) -> None:
    cutoff = time.time() - PENDING_MAX_AGE
    try:
        stale = [p for p in directory.glob("*.pending") if p.stat().st_mtime < cutoff]
    except OSError:
        return
    for path in stale:
        try:
            path.unlink()
        except OSError:
            pass


def cache(payload: dict) -> int:
    banner = repo_banner()
    if not banner:
        return 0
    directory = pending_dir()
    try:
        directory.mkdir(parents=True, exist_ok=True)
        prune(directory)
        path = directory / f"{session_id(payload)}.pending"
        tmp = path.with_suffix(f".{os.getpid()}.tmp")
        tmp.write_text(banner, encoding="utf-8")
        tmp.replace(path)
    except OSError:
        pass
    json.dump({"additionalContext": banner}, sys.stdout)
    return 0


def claim(path: Path) -> str | None:
    """Read the pending banner and consume it in one step.

    os.replace is atomic, so of any number of concurrent injects exactly one
    renames the file successfully and the rest find it gone. Without this two
    prompts racing at session start would both inject the banner.
    """
    taken = path.with_suffix(f".{os.getpid()}.taken")
    try:
        os.replace(path, taken)
    except OSError:
        return None
    try:
        return taken.read_text(encoding="utf-8")
    except OSError:
        return None
    finally:
        try:
            taken.unlink()
        except OSError:
            pass


def inject(payload: dict) -> int:
    path = pending_dir() / f"{session_id(payload)}.pending"
    if not path.is_file():
        return 0

    prompt = ""
    for key in ("transformedPrompt", "prompt", "userPrompt", "text"):
        value = payload.get(key)
        if isinstance(value, str):
            prompt = value
            break
    if SENTINEL in prompt:
        return 0

    banner = claim(path)
    if banner is None:
        return 0
    json.dump({"modifiedTransformedPrompt": f"{banner}\n\n{prompt}"}, sys.stdout)
    return 0


def main() -> int:
    phase = sys.argv[1] if len(sys.argv) > 1 else "cache"
    if phase not in ("cache", "inject"):
        return 0
    payload = read_payload()
    return cache(payload) if phase == "cache" else inject(payload)


if __name__ == "__main__":
    sys.exit(main())
