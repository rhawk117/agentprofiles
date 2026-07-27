#!/usr/bin/env python3
import json
import os
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

CTX_W = 14
LIM_W = 8
FILL = "━"
GIT_CACHE_TTL = int(os.environ.get("CLAUDE_STATUS_GIT_CACHE_SECONDS", "5"))
CACHE_DIR = (
    Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache")) / "claude-statusline"
)

_NC = os.environ.get("NO_COLOR") is not None


def _c(code: str) -> str:
    return "" if _NC else code


RESET = _c("\033[0m")
BOLD = _c("\033[1m")
DIM = _c("\033[2m")
ITALIC = _c("\033[3m")
GRAY = _c("\033[90m")
GREEN = _c("\033[32m")
YELLOW = _c("\033[33m")
RED = _c("\033[91m")
CYAN = _c("\033[36m")
BLUE = _c("\033[34m")
MAGENTA = _c("\033[35m")
SEP = f"{GRAY}  ·  {RESET}"

# Semantic color per permission mode.
MODE_COLORS = {
    "default": GRAY,
    "plan": BLUE,
    "auto": GREEN,
    "acceptEdits": YELLOW,
    "dontAsk": MAGENTA,
    "bypassPermissions": RED + BOLD,
}


def dig(obj, *path, default=None):
    for key in path:
        if not isinstance(obj, dict):
            return default
        obj = obj.get(key)
        if obj is None:
            return default
    return obj


def as_int(value, default=0):
    try:
        return int(float(value))
    except (TypeError, ValueError):
        return default


def as_float(value, default=0.0):
    try:
        return float(value)
    except (TypeError, ValueError):
        return default


def pct_color(pct: int) -> str:
    if pct < 50:
        return GREEN
    if pct < 80:
        return YELLOW
    return RED


def grad_color(pct: int) -> str:
    """Green up to 50%, then interpolate green->red from 50% to 100%."""
    if _NC:
        return ""
    pct = max(0, min(100, pct))
    if pct <= 50:
        r, g = 0, 255
    else:
        f = (pct - 50) / 50
        r, g = int(255 * f), int(255 * (1 - f))
    return f"\033[38;2;{r};{g};0m"


def hash_bar(pct: int, width: int) -> str:
    """Bracketed [###----] gauge, fill colored on the green->red gradient."""
    pct = max(0, min(100, pct))
    filled = min(width, pct * width // 100)
    color = grad_color(pct)
    return f"{GRAY}[{color}{'#' * filled}{GRAY}{'-' * (width - filled)}]{RESET}"


def gauge(label: str, pct: int, width: int, tail: str = "") -> str:
    color = grad_color(pct)
    seg = f"{GRAY}{label}{RESET} {hash_bar(pct, width)} {color}{pct}%{RESET}"
    if tail:
        seg += f" {tail}"
    return seg


def human(tokens: int) -> str:
    if tokens >= 1_000_000:
        tenths = tokens // 100_000
        return (
            f"{tenths // 10}M" if tenths % 10 == 0 else f"{tenths // 10}.{tenths % 10}M"
        )
    if tokens >= 1_000:
        return f"{tokens // 1000}k"
    return str(tokens)


def human_dur(ms: int) -> str:
    s = int(ms // 1000)
    if s < 60:
        return f"{s}s"
    m, s = divmod(s, 60)
    if m < 60:
        return f"{m}m"
    h, m = divmod(m, 60)
    return f"{h}h{m:02d}m"


def fmt_reset(value) -> str:
    dt = None
    if isinstance(value, (int, float)) or (isinstance(value, str) and value.isdigit()):
        try:
            dt = datetime.fromtimestamp(int(float(value)), tz=timezone.utc).astimezone()
        except (OverflowError, OSError, ValueError):
            return ""
    elif isinstance(value, str):
        try:
            dt = datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone()
        except ValueError:
            return ""
    if dt is None:
        return ""
    return dt.strftime("%I:%M%p").lstrip("0").lower()


def _cache_slug(session_id: str) -> str:
    return (
        "".join(ch if ch.isalnum() or ch in "-_" else "_" for ch in session_id)[:64]
        or "default"
    )


def git_state(cwd: str, session_id: str):
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    cache = CACHE_DIR / f"{_cache_slug(session_id)}.json"
    try:
        if time.time() - cache.stat().st_mtime < GIT_CACHE_TTL:
            return json.loads(cache.read_text())
    except (OSError, ValueError):
        pass

    state = {
        "branch": "",
        "staged": 0,
        "modified": 0,
        "untracked": 0,
        "ahead": 0,
        "behind": 0,
    }
    try:
        out = subprocess.run(
            ["git", "status", "--porcelain=v2", "--branch", "--untracked-files=normal"],
            cwd=cwd,
            capture_output=True,
            text=True,
            timeout=3,
            check=False,
        )
        if out.returncode == 0:
            for line in out.stdout.splitlines():
                if line.startswith("# branch.head "):
                    head = line.split(" ", 2)[2]
                    state["branch"] = "" if head == "(detached)" else head
                elif line.startswith("# branch.ab "):
                    parts = line.split()
                    state["ahead"] = abs(as_int(parts[2]))
                    state["behind"] = abs(as_int(parts[3]))
                elif line.startswith(("1 ", "2 ")):
                    xy = line.split(" ", 2)[1]
                    if xy[0] != ".":
                        state["staged"] += 1
                    if xy[1] != ".":
                        state["modified"] += 1
                elif line.startswith("? "):
                    state["untracked"] += 1
    except (OSError, subprocess.SubprocessError):
        pass

    try:
        tmp = cache.with_suffix(".tmp")
        tmp.write_text(json.dumps(state))
        tmp.replace(cache)
    except OSError:
        pass
    return state


EDIT_TOOLS = {"Edit", "Write", "MultiEdit", "NotebookEdit"}
AGENT_TOOLS = {"Task", "Agent"}


def _fresh_stats(path: str) -> dict:
    return {
        "path": path,
        "offset": 0,
        "turns": 0,
        "tools": 0,
        "errors": 0,
        "agents": 0,
        "edits": 0,
        "bash": 0,
        "mode": "",
    }


def session_stats(transcript_path: str, session_id: str) -> dict:
    """Derive live session activity from the transcript.

    Reads incrementally from a cached byte offset so a growing transcript is
    not re-parsed in full on every refresh.
    """
    if not transcript_path:
        return _fresh_stats("")
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    cache = CACHE_DIR / f"{_cache_slug(session_id)}.stats.json"
    st = _fresh_stats(transcript_path)
    try:
        prev = json.loads(cache.read_text())
        if prev.get("path") == transcript_path:
            st = {**st, **prev}
    except (OSError, ValueError):
        pass

    try:
        size = os.path.getsize(transcript_path)
    except OSError:
        return st
    if size < st["offset"]:  # transcript rotated/truncated
        st = _fresh_stats(transcript_path)

    try:
        with open(transcript_path, "rb") as f:
            f.seek(st["offset"])
            chunk = f.read()
    except OSError:
        chunk = b""

    if chunk:
        nl = chunk.rfind(b"\n")
        if nl >= 0:  # only consume complete lines; leave any partial tail
            st["offset"] += nl + 1
            for raw in chunk[: nl + 1].split(b"\n"):
                if not raw.strip():
                    continue
                try:
                    obj = json.loads(raw)
                except ValueError:
                    continue
                _tally(obj, st)

    try:
        tmp = cache.with_suffix(".stats.tmp")
        tmp.write_text(json.dumps(st))
        tmp.replace(cache)
    except OSError:
        pass
    return st


def _tally(obj: dict, st: dict) -> None:
    kind = obj.get("type")
    if kind == "permission-mode":
        st["mode"] = obj.get("permissionMode") or st["mode"]
    elif kind == "assistant":
        for c in dig(obj, "message", "content", default=[]) or []:
            if not isinstance(c, dict) or c.get("type") != "tool_use":
                continue
            st["tools"] += 1
            name = c.get("name")
            if name in AGENT_TOOLS:
                st["agents"] += 1
            elif name in EDIT_TOOLS:
                st["edits"] += 1
            elif name == "Bash":
                st["bash"] += 1
    elif kind == "user" and not obj.get("isMeta"):
        content = dig(obj, "message", "content")
        if isinstance(content, str):
            st["turns"] += 1
        elif isinstance(content, list):
            has_text = has_result = False
            for c in content:
                if not isinstance(c, dict):
                    continue
                t = c.get("type")
                if t == "text":
                    has_text = True
                elif t == "tool_result":
                    has_result = True
                    if c.get("is_error"):
                        st["errors"] += 1
            if has_text and not has_result:
                st["turns"] += 1


def main() -> int:
    try:
        data = json.load(sys.stdin)
    except (ValueError, OSError):
        print(f"{GRAY}statusline: bad input{RESET}")
        return 0

    model = dig(data, "model", "display_name", default="claude")
    cwd = dig(data, "workspace", "current_dir", default=os.getcwd())
    session_id = str(dig(data, "session_id", default="default"))
    transcript_path = str(dig(data, "transcript_path", default=""))
    style = dig(data, "output_style", "name", default="")
    version = dig(data, "version", default="")
    dir_name = os.path.basename(cwd.replace("\\", "/").rstrip("/")) or cwd

    used_pct = as_int(dig(data, "context_window", "used_percentage"))
    window = as_int(dig(data, "context_window", "context_window_size"))
    tokens = as_int(dig(data, "context_window", "total_input_tokens"))

    cost_usd = as_float(dig(data, "cost", "total_cost_usd"))
    dur_ms = as_int(dig(data, "cost", "total_duration_ms"))
    added = as_int(dig(data, "cost", "total_lines_added"))
    removed = as_int(dig(data, "cost", "total_lines_removed"))

    budget = as_int(os.environ.get("CLAUDE_CODE_AUTO_COMPACT_WINDOW"))
    trigger_pct = as_int(os.environ.get("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE"), 100)
    compact_at = 0
    if budget > 0:
        if window > 0:
            budget = min(budget, window)
        compact_at = budget * max(1, min(100, trigger_pct)) // 100

    git = git_state(cwd, session_id)
    stats = session_stats(transcript_path, session_id)

    # ---- line 1: identity, mode, location, git --------------------------
    line1 = f"{BOLD}{model}{RESET}"
    mode = stats.get("mode") or ""
    if mode and mode != "default":
        line1 += f" {MODE_COLORS.get(mode, MAGENTA)}{ITALIC}{mode}{RESET}"
    if style and style != "default":
        line1 += f" {DIM}{ITALIC}{style}{RESET}"
    line1 += f"{SEP}{dir_name}"
    if git["branch"]:
        line1 += f"{SEP}{CYAN}{BOLD}{git['branch']}{RESET}"
        marks = []
        if git["staged"]:
            marks.append(f"{GREEN}+{git['staged']}{RESET}")
        if git["modified"]:
            marks.append(f"{YELLOW}~{git['modified']}{RESET}")
        if git["untracked"]:
            marks.append(f"{GRAY}?{git['untracked']}{RESET}")
        if marks:
            line1 += " " + " ".join(marks)
        sync = []
        if git["ahead"]:
            sync.append(f"↑{git['ahead']}")
        if git["behind"]:
            sync.append(f"↓{git['behind']}")
        if sync:
            line1 += f" {MAGENTA}{''.join(sync)}{RESET}"

    # ---- line 2: gauges (context, compact, rate limits) -----------------
    tail = f"{GRAY}{human(tokens)}/{human(window)}{RESET}" if window > 0 else ""
    gauges = [gauge("ctx", used_pct, CTX_W, tail)]
    if compact_at > 0:
        cpct = min(100, tokens * 100 // compact_at)
        left = f"{DIM}{ITALIC}{human(max(0, compact_at - tokens))} left{RESET}"
        gauges.append(gauge("cmp", cpct, LIM_W, left))
    for key, label in (("five_hour", "5h"), ("seven_day", "7d")):
        pct = dig(data, "rate_limits", key, "used_percentage")
        if pct is None:
            continue
        stamp = fmt_reset(dig(data, "rate_limits", key, "resets_at"))
        tail = f"{DIM}{ITALIC}{stamp}{RESET}" if stamp else ""
        gauges.append(gauge(label, as_int(pct), LIM_W, tail))
    line2 = SEP.join(gauges)

    # ---- line 3: session activity ---------------------------------------
    act = [
        f"{GRAY}turns{RESET} {BOLD}{stats['turns']}{RESET}",
        f"{GRAY}tools{RESET} {BOLD}{stats['tools']}{RESET}",
    ]
    if stats["bash"]:
        act.append(f"{GRAY}sh{RESET} {stats['bash']}")
    if stats["edits"]:
        act.append(f"{GRAY}edits{RESET} {YELLOW}{stats['edits']}{RESET}")
    if stats["agents"]:
        act.append(f"{GRAY}agents{RESET} {MAGENTA}{BOLD}{stats['agents']}{RESET}")
    if stats["errors"]:
        act.append(f"{GRAY}err{RESET} {RED}{BOLD}{stats['errors']}{RESET}")
    if added or removed:
        act.append(f"{GREEN}+{added}{RESET}/{RED}-{removed}{RESET}")
    if cost_usd > 0:
        ccol = GREEN if cost_usd < 1 else YELLOW if cost_usd < 5 else RED
        act.append(f"{ccol}${cost_usd:.2f}{RESET}")
    if dur_ms > 0:
        act.append(f"{DIM}{ITALIC}{human_dur(dur_ms)}{RESET}")
    if version:
        act.append(f"{DIM}{ITALIC}v{version}{RESET}")
    line3 = SEP.join(act)

    print(f"{line1}\n{line2}\n{line3}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
