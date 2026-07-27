#!/usr/bin/env python3
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

type Json = dict[str, object]

MAX_CHARS = 3000
MAX_PROMPTS = 5
PROMPT_CHARS = 220
TEXT_KEYS = ("content", "text", "message", "prompt")


def copilot_home() -> Path:
    override = os.environ.get("COPILOT_HOME")
    return Path(override) if override else Path.home() / ".copilot"


def state_dir() -> Path:
    return copilot_home() / "compaction-state"


def read_payload() -> Json:
    try:
        parsed = json.loads(sys.stdin.read() or "{}")
    except ValueError, OSError:
        return {}
    return parsed if isinstance(parsed, dict) else {}


def slug(value: object) -> str:
    if not isinstance(value, str):
        return ""
    return "".join(c for c in value if c.isalnum() or c in "-_")[:64]


def git(cwd: str, *args: str) -> str:
    if not cwd or not Path(cwd).is_dir():
        return ""
    try:
        out = subprocess.run(
            ["git", *args], cwd=cwd, capture_output=True, text=True, timeout=2
        )
    except OSError, subprocess.SubprocessError:
        return ""
    return out.stdout.strip() if out.returncode == 0 else ""


def events_file(raw: object) -> Path | None:
    if not isinstance(raw, str) or not raw:
        return None
    path = Path(raw)
    if path.is_file():
        return path
    candidate = path / "events.jsonl"
    return candidate if candidate.is_file() else None


def extract_text(node: object) -> str:
    if isinstance(node, str):
        return node
    if isinstance(node, list):
        return " ".join(extract_text(item) for item in node if item)
    if isinstance(node, dict):
        for key in TEXT_KEYS:
            if key in node:
                return extract_text(node[key])
    return ""


def recent_prompts(raw: object) -> list[str]:
    path = events_file(raw)
    if path is None:
        return []
    found: list[str] = []
    try:
        with path.open(encoding="utf-8", errors="replace") as handle:
            for line in handle:
                line = line.strip()
                if not line or '"user"' not in line:
                    continue
                try:
                    event = json.loads(line)
                except ValueError:
                    continue
                if not isinstance(event, dict):
                    continue
                role = event.get("role") or event.get("type") or ""
                if not isinstance(role, str) or "user" not in role.lower():
                    continue
                text = " ".join(extract_text(event).split())
                if text:
                    found.append(text[:PROMPT_CHARS])
    except OSError:
        return []
    return found[-MAX_PROMPTS:]


def compose(payload: Json, previous: int) -> str:
    cwd = payload.get("cwd") if isinstance(payload.get("cwd"), str) else ""
    trigger = (
        payload.get("trigger") if isinstance(payload.get("trigger"), str) else "unknown"
    )
    stamp = datetime.now(timezone.utc).astimezone().strftime("%Y-%m-%d %H:%M")

    lines = [f"compaction {previous + 1} at {stamp}, trigger {trigger}"]
    branch = git(cwd, "symbolic-ref", "--quiet", "--short", "HEAD")
    if branch:
        lines.append(f"branch {branch}")
    stat = git(cwd, "diff", "--stat", "HEAD")
    if stat:
        lines.append("uncommitted changes:")
        lines.extend(f"  {row}" for row in stat.splitlines()[-12:])
    untracked = git(cwd, "ls-files", "--others", "--exclude-standard").splitlines()
    if untracked:
        lines.append("untracked files:")
        lines.extend(f"  {row}" for row in untracked[:8])

    prompts = recent_prompts(
        payload.get("transcriptPath") or payload.get("transcript_path")
    )
    if prompts:
        lines.append("most recent requests before compaction:")
        lines.extend(f"  {p}" for p in prompts)

    body = "\n".join(lines)
    return body[:MAX_CHARS]


def snapshot(payload: Json) -> int:
    sid = slug(payload.get("sessionId") or payload.get("session_id"))
    if not sid:
        return 0
    directory = state_dir()
    note = directory / f"{sid}.md"
    count = 0
    try:
        directory.mkdir(parents=True, exist_ok=True)
        existing = note.read_text(encoding="utf-8")
        count = existing.count("compaction ")
    except OSError:
        existing = ""
    try:
        note.write_text(compose(payload, count), encoding="utf-8")
        (directory / f"{sid}.pending").write_text("1", encoding="utf-8")
    except OSError:
        return 0
    return 0


def emit(sid: str, banner: str) -> int:
    try:
        body = (state_dir() / f"{sid}.md").read_text(encoding="utf-8").strip()
    except OSError:
        return 0
    if not body:
        return 0
    print(json.dumps({"additionalContext": f"<{banner}>\n{body}\n</{banner}>"}))
    return 0


def reinject(payload: Json) -> int:
    sid = slug(payload.get("sessionId") or payload.get("session_id"))
    if not sid:
        return 0
    flag = state_dir() / f"{sid}.pending"
    if not flag.exists():
        return 0
    try:
        flag.unlink()
    except OSError:
        return 0
    return emit(sid, "context_restored_after_compaction")


def rehydrate(payload: Json) -> int:
    source = payload.get("source")
    if source != "resume":
        return 0
    sid = slug(payload.get("sessionId") or payload.get("session_id"))
    if not sid:
        return 0
    return emit(sid, "context_carried_from_previous_session")


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        return 0
    payload = read_payload()
    action = argv[1]
    if action == "snapshot":
        return snapshot(payload)
    if action == "reinject":
        return reinject(payload)
    if action == "rehydrate":
        return rehydrate(payload)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
