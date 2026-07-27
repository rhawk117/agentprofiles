#!/usr/bin/env python3
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

MAX_CHARS = 1800
MAX_FINDINGS = 20
TIMEOUT = 25


def emit(context: str | None = None) -> int:
    if context:
        json.dump(
            {
                "hookSpecificOutput": {
                    "hookEventName": "PostToolUse",
                    "additionalContext": context[:MAX_CHARS],
                }
            },
            sys.stdout,
        )
    return 0


def project_root(path: Path) -> Path | None:
    for parent in [path] + list(path.parents):
        if (parent / "pyproject.toml").is_file():
            return parent
    return None


def ruff_cmd(root: Path | None) -> list[str] | None:
    if shutil.which("uv"):
        return (
            ["uv", "run", "--project", str(root), "ruff"] if root else ["uvx", "ruff"]
        )
    if shutil.which("ruff"):
        return ["ruff"]
    return None


def run(cmd: list[str], cwd: Path) -> tuple[int, str, str]:
    try:
        proc = subprocess.run(
            cmd,
            cwd=cwd,
            capture_output=True,
            text=True,
            timeout=TIMEOUT,
            env={**os.environ, "NO_COLOR": "1"},
        )
        return proc.returncode, proc.stdout.strip(), proc.stderr.strip()
    except subprocess.TimeoutExpired:
        return 124, "", "timed out"
    except OSError as exc:
        return 127, "", str(exc)


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except (ValueError, OSError):
        return emit()

    raw = (payload.get("tool_input") or {}).get("file_path") or ""
    if not raw:
        return emit()

    path = Path(raw)
    if not path.is_absolute():
        path = Path(payload.get("cwd") or os.getcwd()) / path

    if path.suffix != ".py" or not path.is_file():
        return emit()

    root = project_root(path)
    base = ruff_cmd(root)
    if base is None:
        return emit()

    cwd = root or path.parent
    before = path.read_bytes()

    notes: list[str] = []
    fmt_rc, _, fmt_err = run([*base, "format", "--quiet", str(path)], cwd)
    if fmt_rc not in (0, 1):
        notes.append(f"ruff format failed: {fmt_err[:200]}")
    elif path.read_bytes() != before:
        notes.append(
            f"ruff format rewrote {path.name} after your edit. "
            "Your in-context copy is now stale; re-read before further edits."
        )

    check_rc, check_out, check_err = run(
        [*base, "check", "--quiet", "--output-format", "concise", str(path)], cwd
    )
    if check_rc not in (0, 1):
        notes.append(f"ruff check failed: {check_err[:200]}")
    findings = [ln for ln in check_out.splitlines() if ln.strip()]
    if findings:
        shown = findings[:MAX_FINDINGS]
        extra = len(findings) - len(shown)
        header = f"ruff check found {len(findings)} issue(s) in {path.name}:"
        body = "\n".join(shown)
        if extra > 0:
            body += f"\n... and {extra} more"
        notes.append(f"{header}\n{body}")

    return emit("\n\n".join(notes) if notes else None)


if __name__ == "__main__":
    sys.exit(main())
