#!/usr/bin/env python3
import json
import os
import re
import sys
import time
from pathlib import Path

LOG = Path(os.environ.get("CLAUDE_READ_LOG", Path.home() / ".claude-read-log.jsonl"))
TRACKED = {"Read", "Grep", "Glob", "Bash", "Edit", "Write", "NotebookEdit"}
MAX_REFS = 60

TRACEBACK = re.compile(r'File "([^"]+\.pyi?)", line (\d+)')
COLON_REF = re.compile(r"(?:^|[\s'\"(])([\w./\\-]+\.pyi?):(\d+)")
PYTEST_ID = re.compile(r"(?:^|[\s'\"(])([\w./\\-]+\.pyi?)::")
BARE_PATH = re.compile(r"(?:^|[\s'\"(])([\w./\\-]+\.pyi?)(?=[\s'\":,)]|$)")


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


def turn_ordinal(session: str) -> int:
    counter = LOG.parent / f".turn-{session or 'default'}"
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

    tool = payload.get("tool_name", "")
    if tool not in TRACKED:
        return 0

    tool_input = payload.get("tool_input") or {}
    response = payload.get("tool_response", payload.get("tool_output"))
    session = str(payload.get("session_id") or "default")

    text = as_text(response)
    record = {
        "ts": time.time(),
        "session": session,
        "agent_id": payload.get("agent_id"),
        "agent_type": payload.get("agent_type"),
        "prompt_id": payload.get("prompt_id"),
        "cwd": payload.get("cwd"),
        "tool": tool,
        "path": tool_input.get("file_path"),
        "pattern": tool_input.get("pattern"),
        "command": (tool_input.get("command") or "")[:200] or None,
        "partial": bool(tool_input.get("offset") or tool_input.get("limit")),
        "out_bytes": len(text),
        "refs": [] if tool == "Read" else extract_refs(text),
    }

    try:
        LOG.parent.mkdir(parents=True, exist_ok=True)
        record["turn"] = turn_ordinal(session)
        with LOG.open("a") as fh:
            fh.write(json.dumps(record) + "\n")
    except OSError:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
