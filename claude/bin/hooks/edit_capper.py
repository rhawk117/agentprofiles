#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

THRESHOLD = int(os.environ.get("CLAUDE_EDIT_CAP_BYTES", "10000"))
CONTEXT_LINES = int(os.environ.get("CLAUDE_EDIT_CAP_CONTEXT", "6"))
MAX_WINDOW_LINES = 80
MAX_OUT = 6000
TOOLS = {"Edit", "Write", "NotebookEdit", "MultiEdit"}


def serialized(value) -> str:
    if value is None:
        return ""
    if isinstance(value, str):
        return value
    try:
        return json.dumps(value)
    except TypeError, ValueError:
        return str(value)


def resolve(raw: str, cwd: str) -> Path | None:
    if not raw:
        return None
    p = Path(raw)
    if not p.is_absolute():
        p = Path(cwd or ".") / p
    try:
        return p.resolve()
    except OSError:
        return None


def locate(lines: list[str], needle: str) -> tuple[int, int] | None:
    if not needle:
        return None
    target = needle.splitlines() or [needle]
    first = target[0]
    span = len(target)
    for i, line in enumerate(lines):
        if line == first and lines[i : i + span] == target:
            return (i + 1, i + span)
    for i, line in enumerate(lines):
        if first and first.strip() and first.strip() in line:
            return (i + 1, i + 1)
    return None


def window(lines: list[str], start: int, end: int) -> str:
    lo = max(1, start - CONTEXT_LINES)
    hi = min(len(lines), end + CONTEXT_LINES)
    if hi - lo + 1 > MAX_WINDOW_LINES:
        hi = lo + MAX_WINDOW_LINES - 1
    out = []
    for n in range(lo, hi + 1):
        mark = ">" if start <= n <= end else " "
        out.append(f"{mark}{n:>6}  {lines[n - 1]}")
    return "\n".join(out)


def build(tool: str, path: Path, tool_input: dict, original: int) -> str | None:
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    lines = text.splitlines()
    total = len(lines)
    head = [
        f"[edit-capper] {tool} applied to {path.name} — full tool output suppressed.",
        f"File is now {total} lines / {len(text):,} bytes. Original tool response was {original:,} bytes.",
    ]

    if tool == "Write":
        head.append(
            "You authored this content in the tool call above; it is not repeated here."
        )
        return "\n".join(head)

    new_string = tool_input.get("new_string")
    if isinstance(new_string, str) and new_string:
        span = locate(lines, new_string)
        if span:
            head.append(
                f"Change landed at lines {span[0]}-{span[1]}. Surrounding context:"
            )
            head.append("")
            head.append(window(lines, span[0], span[1]))
            return "\n".join(head)
        head.append(
            "WARNING: could not locate new_string in the file after the edit. "
            "Re-read the file to verify the change applied as intended."
        )
        return "\n".join(head)

    head.append(
        "Change region could not be determined; re-read if you need to verify placement."
    )
    return "\n".join(head)


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except ValueError, OSError:
        return 0

    tool = payload.get("tool_name", "")
    if tool not in TOOLS:
        return 0

    original = len(serialized(payload.get("tool_response", payload.get("tool_output"))))
    if original <= THRESHOLD:
        return 0

    tool_input = payload.get("tool_input") or {}
    path = resolve(tool_input.get("file_path") or "", payload.get("cwd") or "")
    if path is None or not path.is_file():
        return 0

    replacement = build(tool, path, tool_input, original)
    if not replacement:
        return 0
    if len(replacement) >= original:
        return 0

    json.dump(
        {
            "hookSpecificOutput": {
                "hookEventName": "PostToolUse",
                "updatedToolOutput": replacement[:MAX_OUT],
            }
        },
        sys.stdout,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
