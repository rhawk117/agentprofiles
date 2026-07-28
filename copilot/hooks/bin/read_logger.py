#!/usr/bin/env python3
"""Copilot `postToolUse` hook: append one JSONL record per tracked tool call.

Port of claude/bin/hooks/read_logger.py with three Copilot-specific changes:
Copilot tool names are lowercase, its payload keys are camelCase, and the
turn-ordinal counter lives under COPILOT_HOME rather than beside the log --
which in the Claude version defaults to writing dotfiles straight into $HOME.

stdout stays empty. postToolUse stdout is fed back into the model's context, so
anything printed here would silently become part of the conversation.
"""

import json
import os
import re
import sys
import time
from pathlib import Path

TRACKED = {
    "view",
    "bash",
    "edit",
    "create",
    "str_replace_editor",
    "apply_patch",
    "grep",
    "glob",
}
MAX_REFS = 60

TRACEBACK = re.compile(r'File "([^"]+\.pyi?)", line (\d+)')
COLON_REF = re.compile(r"(?:^|[\s'\"(])([\w./\\-]+\.pyi?):(\d+)")
PYTEST_ID = re.compile(r"(?:^|[\s'\"(])([\w./\\-]+\.pyi?)::")
BARE_PATH = re.compile(r"(?:^|[\s'\"(])([\w./\\-]+\.pyi?)(?=[\s'\":,)]|$)")


def copilot_home() -> Path:
    return Path(os.environ.get("COPILOT_HOME") or Path.home() / ".copilot")


def log_path() -> Path:
    override = os.environ.get("COPILOT_READ_LOG")
    return Path(override) if override else copilot_home() / "read-log" / "reads.jsonl"


def first(payload: dict, *keys, default=None):
    """Copilot's documented keys are camelCase; the snake_case spellings are
    accepted too because the payload shape has not been observed live."""
    for key in keys:
        value = payload.get(key)
        if value is not None:
            return value
    return default


def as_text(value) -> str:
    if value is None:
        return ""
    if isinstance(value, str):
        return value
    try:
        return json.dumps(value)
    except (TypeError, ValueError):
        return str(value)


def extract_refs(text: str) -> list[dict]:
    refs: list[dict] = []
    seen: set[tuple[str, int]] = set()

    def add(path: str, line: int, kind: str) -> None:
        key = (path, line)
        if key in seen:
            return
        seen.add(key)
        refs.append({"path": path, "line": line, "kind": kind})

    for pattern, kind in ((TRACEBACK, "traceback"), (COLON_REF, "line_ref")):
        for match in pattern.finditer(text):
            add(match.group(1), int(match.group(2)), kind)
            if len(refs) >= MAX_REFS:
                return refs
    for pattern, kind in ((PYTEST_ID, "test_id"), (BARE_PATH, "bare")):
        for match in pattern.finditer(text):
            add(match.group(1), 0, kind)
            if len(refs) >= MAX_REFS:
                return refs
    return refs


def turn_ordinal(log: Path, session: str) -> int:
    counter = log.parent / f".turn-{session or 'default'}"
    try:
        n = int(counter.read_text()) + 1
    except (OSError, ValueError):
        n = 1
    try:
        counter.write_text(str(n))
    except OSError:
        pass
    return n


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except (ValueError, OSError):
        return 0
    if not isinstance(payload, dict):
        return 0

    tool = str(first(payload, "toolName", "tool_name", default=""))
    if tool not in TRACKED:
        return 0

    args = first(payload, "toolArgs", "tool_input", default={}) or {}
    if not isinstance(args, dict):
        args = {}
    response = first(payload, "toolResult", "tool_response", "toolOutput")
    session = str(first(payload, "sessionId", "session_id", default="default"))

    text = as_text(response)
    record = {
        "ts": time.time(),
        "session": session,
        "agent_id": first(payload, "agentId", "agent_id"),
        "agent_type": first(payload, "agentType", "agent_type"),
        "cwd": first(payload, "cwd"),
        "tool": tool,
        "path": args.get("path") or args.get("file_path"),
        "pattern": args.get("pattern"),
        "command": (args.get("command") or "")[:200] or None,
        "partial": bool(args.get("offset") or args.get("limit")),
        "out_bytes": len(text),
        "refs": [] if tool == "view" else extract_refs(text),
    }

    log = log_path()
    try:
        log.parent.mkdir(parents=True, exist_ok=True)
        record["turn"] = turn_ordinal(log, session)
        with log.open("a") as fh:
            fh.write(json.dumps(record) + "\n")
    except OSError:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
