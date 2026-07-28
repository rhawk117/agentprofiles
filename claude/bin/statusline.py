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
PER_MILLION = 1_000_000
MS_PER_HOUR = 3_600_000
BURN_MIN_MS = 60_000  # below a minute the $/hr figure is noise

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

ORANGE = _c("\033[38;5;208m")

# Semantic color per permission mode.
MODE_COLORS = {
    "default": GRAY,
    "plan": BLUE,
    "auto": GREEN,
    "acceptEdits": YELLOW,
    "dontAsk": MAGENTA,
    "bypassPermissions": RED + BOLD,
}

VENDOR_COLORS = {"claude": ORANGE, "gpt": CYAN, "google": MAGENTA}

# Glyph and text attributes per verdict recorded in model-rates.json.
VERDICT_STYLES = {
    "amazing": ("★", BOLD),
    "out-classed": ("✧", ""),
    "niche": ("◇", DIM + ITALIC),
    "legacy": ("◴", DIM + ITALIC),
}

EFFORT_COLORS = {
    "low": GREEN,
    "medium": CYAN,
    "high": YELLOW,
    "xhigh": ORANGE,
    "max": RED,
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


def load_rates() -> dict:
    """First readable model-rates.json wins; absence disables cost segments."""
    override = os.environ.get("CLAUDE_MODEL_RATES")
    candidates = [Path(override)] if override else []
    candidates.append(Path.home() / ".claude" / "model-rates.json")
    # __file__ resolves through the install symlink back into the repo checkout.
    candidates.append(Path(__file__).resolve().parent.parent / "model-rates.json")
    for path in candidates:
        try:
            parsed = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        if isinstance(parsed, dict):
            return parsed
    return {}


def model_rates(model_id: str, table: dict) -> dict:
    """Exact match on the model id, else the longest key that prefixes it."""
    if not model_id:
        return {}
    entry = table.get(model_id)
    if isinstance(entry, dict):
        return entry
    matches = [
        (key, value)
        for key, value in table.items()
        if not key.startswith("_")
        and isinstance(value, dict)
        and model_id.startswith(key)
    ]
    if not matches:
        return {}
    return max(matches, key=lambda item: len(item[0]))[1]


def model_badge(name: str, rates: dict, effort: str, thinking: bool) -> str:
    glyphs = ""
    attrs = ""
    verdicts = rates.get("verdicts")
    for verdict in verdicts if isinstance(verdicts, list) else []:
        style = VERDICT_STYLES.get(str(verdict))
        if style is None:
            continue
        glyphs += style[0]
        attrs += style[1]

    color = VENDOR_COLORS.get(str(rates.get("vendor")), "")
    badge = f"{color}{attrs or BOLD}{name}{RESET}"
    if glyphs:
        badge = f"{color}{glyphs}{RESET} {badge}"
    if effort:
        badge += f" {EFFORT_COLORS.get(effort, GRAY)}[{effort}]{RESET}"
    if thinking:
        badge += f" {MAGENTA}✻{RESET}"
    return badge


def context_cost(usage: dict, rates: dict, fast: bool) -> float:
    """Dollar cost of sending the current context once, at the model's rates."""
    if not rates:
        return 0.0
    fast_in = rates.get("fast_input") if fast else None
    fast_out = rates.get("fast_output") if fast else None
    priced = (
        ("input_tokens", fast_in if fast_in is not None else rates.get("input")),
        ("output_tokens", fast_out if fast_out is not None else rates.get("output")),
        ("cache_read_input_tokens", rates.get("cache_read")),
        ("cache_creation_input_tokens", rates.get("cache_write")),
    )
    total = sum(as_int(usage.get(key)) * as_float(rate) for key, rate in priced)
    return total / PER_MILLION


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


def context_gauge(used_pct: int, tokens: int, window: int, compact_at: int) -> str:
    """Context bar with the auto-compact threshold marked in-line as a `·` cell,
    so one gauge carries both figures instead of two side by side."""
    used_pct = max(0, min(100, used_pct))
    counts = f"{GRAY}◌ {human(tokens)}/{human(window)}{RESET}" if window > 0 else ""

    filled = min(CTX_W, used_pct * CTX_W // 100)
    cells = ["#" if i < filled else "-" for i in range(CTX_W)]
    marker = -1
    if compact_at > 0 and window > 0:
        compact_pct = max(0, min(100, compact_at * 100 // window))
        marker = min(CTX_W - 1, max(0, -(-compact_pct * CTX_W // 100) - 1))
        cells[marker] = "·"

    painted = []
    for i, cell in enumerate(cells):
        if i == marker:
            painted.append(f"{YELLOW}{ITALIC}{cell}{RESET}")
        elif cell == "#":
            painted.append(f"{grad_color(used_pct)}{cell}{RESET}")
        else:
            painted.append(f"{GRAY}{cell}{RESET}")

    seg = (
        f"{YELLOW}◉{RESET} {GRAY}[{RESET}{''.join(painted)}{GRAY}]{RESET} "
        f"{grad_color(used_pct)}{used_pct}%{RESET}"
    )
    if counts:
        seg += f"  {counts}"
    if compact_at > 0:
        left = human(max(0, compact_at - tokens))
        seg += f"  {YELLOW}{ITALIC}{left} left{RESET}"
    return seg


def rate_limit_state(data: dict) -> dict:
    """Rate limits are account-wide and missing from many payloads, so the last
    values seen are recorded and replayed rather than blinking out mid-session.
    A remembered window is dropped once its reset time has passed."""
    cache = CACHE_DIR / "rate-limits.json"
    try:
        remembered = json.loads(cache.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        remembered = {}
    if not isinstance(remembered, dict):
        remembered = {}

    now = datetime.now(timezone.utc)
    limits = {}
    for key, entry in remembered.items():
        if not isinstance(entry, dict):
            continue
        resets = reset_dt(entry.get("resets_at"))
        if resets is not None and resets <= now:
            continue
        limits[key] = {**entry, "stale": True}

    fresh = {}
    for key in ("five_hour", "seven_day"):
        pct = dig(data, "rate_limits", key, "used_percentage")
        if pct is None:
            continue
        fresh[key] = {
            "pct": as_int(pct),
            "resets_at": dig(data, "rate_limits", key, "resets_at"),
            "stale": False,
        }
    if not fresh:
        return limits

    limits.update(fresh)
    try:
        CACHE_DIR.mkdir(parents=True, exist_ok=True)
        tmp = cache.with_name(f"{cache.name}.{os.getpid()}.tmp")
        tmp.write_text(json.dumps(limits), encoding="utf-8")
        tmp.replace(cache)
    except OSError:
        pass
    return limits


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


def reset_dt(value):
    """Reset stamps arrive as epoch seconds or ISO-8601 depending on the field."""
    if value is None or isinstance(value, bool):
        return None
    if isinstance(value, (int, float)) or (isinstance(value, str) and value.isdigit()):
        try:
            return datetime.fromtimestamp(
                int(float(value)), tz=timezone.utc
            ).astimezone()
        except (OverflowError, OSError, ValueError):
            return None
    if isinstance(value, str):
        try:
            return datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone()
        except ValueError:
            return None
    return None


def fmt_reset(value) -> str:
    dt = reset_dt(value)
    return dt.strftime("%I:%M%p").lstrip("0").lower() if dt else ""


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
    rates = model_rates(str(dig(data, "model", "id", default="")), load_rates())
    effort = str(dig(data, "effort", "level", default=""))
    thinking = bool(dig(data, "thinking", "enabled", default=False))
    fast = bool(dig(data, "fast_mode", default=False))
    usage = dig(data, "context_window", "current_usage", default={}) or {}
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
    line1 = model_badge(str(model), rates, effort, thinking)
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
    gauges = [context_gauge(used_pct, tokens, window, compact_at)]
    limits = rate_limit_state(data)
    for key, label in (("five_hour", "5h"), ("seven_day", "7d")):
        entry = limits.get(key)
        if not entry:
            continue
        stamp = fmt_reset(entry.get("resets_at"))
        tail = f"{DIM}{ITALIC}{stamp}{RESET}" if stamp else ""
        if entry.get("stale"):
            label = f"{DIM}{label}{RESET}"
        gauges.append(gauge(label, as_int(entry.get("pct")), LIM_W, tail))
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
    msg_cost = context_cost(usage, rates, fast)
    if msg_cost > 0:
        act.append(f"{GRAY}~${msg_cost:.2f}/msg{RESET}")
    if cost_usd > 0 and dur_ms >= BURN_MIN_MS:
        act.append(f"{GRAY}${cost_usd * MS_PER_HOUR / dur_ms:.2f}/hr{RESET}")
    cached = as_int(usage.get("cache_read_input_tokens"))
    if tokens > 0 and cached > 0:
        hit = min(100, cached * 100 // tokens)
        act.append(f"{GRAY}cache{RESET} {grad_color(100 - hit)}{hit}%{RESET}")
    if dur_ms > 0:
        act.append(f"{DIM}{ITALIC}{human_dur(dur_ms)}{RESET}")
    if version:
        act.append(f"{DIM}{ITALIC}v{version}{RESET}")
    line3 = SEP.join(act)

    lines = [line1, line2, line3]
    if dig(data, "exceeds_200k_tokens", default=False) and (
        os.environ.get("CLAUDE_STATUS_NO_ALERT") != "1"
    ):
        lines.insert(1, f"{RED}{BOLD}(!) LONG_CONTEXT · premium input rate (!){RESET}")

    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
