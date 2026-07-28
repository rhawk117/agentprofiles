#!/usr/bin/env python3
import json
import os
import subprocess
import sys
from pathlib import Path

type Json = dict[str, object]

MAX_CHARS = 2400
SKIP = {
    ".git",
    ".venv",
    "venv",
    "__pycache__",
    "node_modules",
    ".mypy_cache",
    ".ruff_cache",
    "build",
    "dist",
}
SHARED_GLOBS = [
    "uv.lock",
    "poetry.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "Cargo.lock",
    "alembic.ini",
    "conftest.py",
    ".pre-commit-config.yaml",
    "docker-compose.yml",
]
SHARED_DIRS = ["migrations", "alembic", ".github/workflows", "fixtures"]


def copilot_home() -> Path:
    override = os.environ.get("COPILOT_HOME")
    return Path(override) if override else Path.home() / ".copilot"


def read_payload() -> Json:
    try:
        parsed = json.loads(sys.stdin.read() or "{}")
    except (ValueError, OSError):
        return {}
    return parsed if isinstance(parsed, dict) else {}


def git(cwd: Path, *args: str) -> str:
    try:
        out = subprocess.run(
            ["git", *args], cwd=cwd, capture_output=True, text=True, timeout=2
        )
    except (OSError, subprocess.SubprocessError):
        return ""
    return out.stdout.strip() if out.returncode == 0 else ""


def packages(root: Path) -> list[str]:
    found = []
    for path in sorted(root.rglob("__init__.py")):
        if any(part in SKIP for part in path.parts):
            continue
        parent = path.parent
        if not (parent.parent / "__init__.py").exists():
            found.append(str(parent.relative_to(root)))
    return found[:12]


def toolchain(root: Path) -> list[str]:
    lines = []
    if (root / "pyproject.toml").exists():
        try:
            text = (root / "pyproject.toml").read_text(
                encoding="utf-8", errors="replace"
            )
        except OSError:
            text = ""
        if "ruff" in text:
            lines.append("lint: uv run ruff check . && uv run ruff format --check .")
        if "[tool.ty" in text or "ty" in text.split("dependencies", 1)[0]:
            lines.append("types: uv run ty check")
        if "pytest" in text:
            lines.append("tests: uv run pytest")
    if (root / "Makefile").exists():
        lines.append(
            "a Makefile is present; prefer its targets when they cover the task"
        )
    return lines


def shared_resources(root: Path) -> list[str]:
    found = []
    for name in SHARED_GLOBS:
        if (root / name).exists():
            found.append(name)
    for name in SHARED_DIRS:
        if (root / name).is_dir():
            found.append(f"{name}/")
    for path in sorted(root.rglob("conftest.py")):
        if any(part in SKIP for part in path.parts):
            continue
        rel = str(path.relative_to(root))
        if rel not in found:
            found.append(rel)
    return found[:14]


def static_context(agent: str) -> str:
    path = copilot_home() / "agent-context" / f"{agent}.md"
    try:
        return path.read_text(encoding="utf-8").strip()
    except OSError:
        return ""


def build(agent: str, root: Path) -> str:
    blocks: list[str] = []
    branch = git(root, "symbolic-ref", "--quiet", "--short", "HEAD")
    dirty = [
        line[2:].strip() for line in git(root, "status", "--porcelain").splitlines()
    ][:10]

    header = [f"repository root: {root}"]
    if branch:
        header.append(f"branch: {branch}")
    blocks.append("\n".join(header))

    if agent in {"scout", "code-analyst", "engineer"}:
        pkgs = packages(root)
        if pkgs:
            blocks.append("python packages:\n" + "\n".join(f"  {p}" for p in pkgs))

    if agent in {"code-analyst", "engineer"}:
        chain = toolchain(root)
        if chain:
            blocks.append(
                "verification commands detected from the manifest:\n"
                + "\n".join(f"  {c}" for c in chain)
            )

    if agent == "engineer" and dirty:
        blocks.append(
            "files already modified in this working tree (another implementer may own these):\n"
            + "\n".join(f"  {d}" for d in dirty)
        )

    if agent == "plan-critic":
        shared = shared_resources(root)
        if shared:
            blocks.append(
                "shared-resource candidates in this repository; check the plan accounts for any "
                "a parallel group touches:\n" + "\n".join(f"  {s}" for s in shared)
            )

    extra = static_context(agent)
    if extra:
        blocks.append(extra)

    body = "\n\n".join(blocks)
    if len(body) > MAX_CHARS:
        body = body[:MAX_CHARS].rsplit("\n", 1)[0] + "\n  (truncated)"
    return body


def main() -> int:
    payload = read_payload()
    agent = payload.get("agentName")
    if not isinstance(agent, str) or not agent:
        return 0
    raw_cwd = payload.get("cwd")
    root = (
        Path(raw_cwd)
        if isinstance(raw_cwd, str) and Path(raw_cwd).is_dir()
        else Path.cwd()
    )
    top = git(root, "rev-parse", "--show-toplevel")
    if top:
        root = Path(top)
    try:
        body = build(agent, root)
    except Exception:
        return 0
    if not body.strip():
        return 0
    context = f'<repository_context agent="{agent}">\n{body}\n</repository_context>'
    print(json.dumps({"additionalContext": context}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
