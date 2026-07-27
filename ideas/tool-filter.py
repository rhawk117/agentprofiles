#!/usr/bin/env python3
import hashlib
import json
import os
import re
import sqlite3
import sys
from pathlib import Path

type Json = dict[str, object]

MAX_LINES = int(os.environ.get("COPILOT_TRUNCATE_LINES", "120"))
MAX_CHARS = int(os.environ.get("COPILOT_TRUNCATE_CHARS", "12000"))
HEAD_LINES = int(os.environ.get("COPILOT_TRUNCATE_HEAD", "40"))
TAIL_LINES = int(os.environ.get("COPILOT_TRUNCATE_TAIL", "40"))

UV_TOOLS = {"pytest", "ruff", "ty", "mypy", "black", "coverage", "alembic"}
SEGMENT = re.compile(r"(^|\|\||&&|\||;|\n)\s*([a-zA-Z_][\w.-]*)")
ARG_PATH_KEYS = ("path", "file_path", "filename", "file", "target")
ARG_CMD_KEYS = ("command", "cmd", "script", "input")


def copilot_home() -> Path:
    override = os.environ.get("COPILOT_HOME")
    return Path(override) if override else Path.home() / ".copilot"


def cache_root() -> Path:
    return copilot_home() / "tool-cache"


def read_payload() -> Json:
    try:
        parsed = json.loads(sys.stdin.read() or "{}")
    except ValueError, OSError:
        return {}
    return parsed if isinstance(parsed, dict) else {}


def session_of(payload: Json) -> str:
    raw = payload.get("sessionId") or payload.get("session_id") or ""
    if not isinstance(raw, str):
        return ""
    return "".join(c for c in raw if c.isalnum() or c in "-_")[:64]


def args_of(payload: Json) -> Json:
    raw = payload.get("toolArgs") or payload.get("tool_input")
    if isinstance(raw, str):
        try:
            raw = json.loads(raw)
        except ValueError:
            return {}
    return raw if isinstance(raw, dict) else {}


def pick(source: Json, keys: tuple[str, ...]) -> tuple[str, str]:
    for key in keys:
        value = source.get(key)
        if isinstance(value, str) and value.strip():
            return key, value
    return "", ""


def result_text(payload: Json) -> str:
    result = payload.get("toolResult") or payload.get("tool_result")
    if not isinstance(result, dict):
        return ""
    for key in ("textResultForLlm", "text_result_for_llm"):
        value = result.get(key)
        if isinstance(value, str):
            return value
    return ""


def emit_result(text: str) -> int:
    print(
        json.dumps(
            {"modifiedResult": {"resultType": "success", "textResultForLlm": text}}
        )
    )
    return 0


def db() -> sqlite3.Connection:
    cache_root().mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(cache_root() / "reads.db", timeout=2.0)
    conn.execute("pragma journal_mode=wal")
    conn.execute(
        "create table if not exists reads ("
        " session_id text not null, path text not null, content_hash text not null,"
        " seen_at text not null default (datetime('now')),"
        " primary key (session_id, path))"
    )
    return conn


def truncate(payload: Json) -> int:
    text = result_text(payload)
    if not text:
        return 0
    lines = text.splitlines()
    if len(lines) <= MAX_LINES and len(text) <= MAX_CHARS:
        return 0

    session = session_of(payload) or "unknown"
    digest = hashlib.sha256(text.encode("utf-8", "replace")).hexdigest()[:12]
    log = cache_root() / "output" / f"{session}-{digest}.log"
    try:
        log.parent.mkdir(parents=True, exist_ok=True)
        log.write_text(text, encoding="utf-8")
        pointer = str(log)
    except OSError:
        pointer = ""

    head = lines[:HEAD_LINES]
    tail = lines[-TAIL_LINES:] if len(lines) > HEAD_LINES + TAIL_LINES else []
    hidden = len(lines) - len(head) - len(tail)
    middle = [f"... {hidden} lines omitted ..."] if hidden > 0 else []
    note = [f"[full output: {pointer}]"] if pointer else ["[full output not saved]"]
    return emit_result("\n".join(head + middle + tail + note))


def rewrite(payload: Json) -> int:
    cwd = payload.get("cwd")
    if not isinstance(cwd, str) or not (Path(cwd) / "pyproject.toml").is_file():
        return 0
    args = args_of(payload)
    key, command = pick(args, ARG_CMD_KEYS)
    if not key or "uv run" in command:
        return 0

    changed = False

    def swap(match: re.Match[str]) -> str:
        nonlocal changed
        lead, token = match.group(1), match.group(2)
        if token not in UV_TOOLS:
            return match.group(0)
        changed = True
        return f"{lead}{' ' if lead.strip() else ''}uv run {token}".replace("  ", " ")

    updated = SEGMENT.sub(swap, command)
    if not changed or updated == command:
        return 0
    print(json.dumps({"modifiedArgs": {**args, key: updated}}))
    return 0


def dedupe(payload: Json) -> int:
    session = session_of(payload)
    _, path = pick(args_of(payload), ARG_PATH_KEYS)
    if not session or not path:
        return 0
    text = result_text(payload)
    if not text or len(text) < 400:
        return 0
    digest = hashlib.sha256(text.encode("utf-8", "replace")).hexdigest()

    try:
        conn = db()
        row = conn.execute(
            "select content_hash, seen_at from reads where session_id=? and path=?",
            (session, path),
        ).fetchone()
        if row and row[0] == digest:
            conn.close()
            return emit_result(
                f"{path} is unchanged since you read it earlier in this session "
                f"(first read {row[1]} UTC). The contents are already above in your context. "
                f"Re-read it only if you need it again after a compaction."
            )
        conn.execute(
            "insert into reads (session_id, path, content_hash) values (?,?,?)"
            " on conflict(session_id, path) do update set content_hash=excluded.content_hash,"
            " seen_at=datetime('now')",
            (session, path, digest),
        )
        conn.commit()
        conn.close()
    except sqlite3.Error:
        return 0
    return 0


def invalidate(payload: Json) -> int:
    session = session_of(payload)
    _, path = pick(args_of(payload), ARG_PATH_KEYS)
    if not session or not path:
        return 0
    try:
        conn = db()
        conn.execute("delete from reads where session_id=? and path=?", (session, path))
        conn.commit()
        conn.close()
    except sqlite3.Error:
        return 0
    return 0


def reset(payload: Json) -> int:
    session = session_of(payload)
    if not session:
        return 0
    try:
        conn = db()
        conn.execute("delete from reads where session_id=?", (session,))
        conn.commit()
        conn.close()
    except sqlite3.Error:
        return 0
    return 0


ACTIONS = {
    "truncate": truncate,
    "rewrite": rewrite,
    "dedupe": dedupe,
    "invalidate": invalidate,
    "reset": reset,
}


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        return 0
    action = ACTIONS.get(argv[1])
    if action is None:
        return 0
    try:
        return action(read_payload())
    except Exception:
        return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
