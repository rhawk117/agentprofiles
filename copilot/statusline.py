#!/usr/bin/env python3
import json
import math
import os
import re
import subprocess
import sys
import time
from pathlib import Path

type Json = dict[str, object]

CTX_W = 14
LIM_W = 6

GIT_CACHE_TTL = int(os.environ.get("COPILOT_STATUS_GIT_CACHE_SECONDS", "5"))
DEFAULT_COMPACT_PCT = 80.0
NANO_AIU_PER_AIC = 1_000_000_000
PER_MILLION = 1_000_000
MS_PER_HOUR = 3_600_000
BURN_MIN_MS = 60_000  # below a minute the $/hr figure is noise
CACHE_TTL_ALERT_USD = 0.10  # below this the 5m-vs-1h ambiguity is not worth a line

_NC = os.environ.get("NO_COLOR") is not None


def _c(code: str) -> str:
    return "" if _NC else code


RESET = _c("\033[0m")
BOLD = _c("\033[1m")
GRAY = _c("\033[90m")
GREEN = _c("\033[32m")
YELLOW = _c("\033[33m")
RED = _c("\033[91m")
CYAN = _c("\033[36m")
MAGENTA = _c("\033[35m")
ORANGE = _c("\033[38;5;208m")
DIM = _c("\033[2m")
ITALIC = _c("\033[3m")
UNDERLINE = _c("\033[4m")
SEP = f"{GRAY}   {RESET}"

VENDOR_COLORS = {
    "claude": ORANGE,
    "gpt": CYAN,
    "google": MAGENTA,
}
REASONING_COLORS = {
    "low": GREEN,
    "medium": CYAN,
    "high": YELLOW,
    "xhigh": ORANGE,
    "max": RED,
}
VERDICT_STYLES = {
    "amazing": ("★", BOLD),
    "out-classed": ("✧", ""),
    "niche": ("◇", DIM + ITALIC),
    "legacy": ("◴", DIM + ITALIC),
}


def copilot_home() -> Path:
    override = os.environ.get("COPILOT_HOME")
    if override:
        return Path(override)
    return Path.home() / ".copilot"


def state_dir() -> Path:
    return copilot_home() / "statusline-state"


def cache_dir() -> Path:
    base = os.environ.get("XDG_CACHE_HOME")
    root = Path(base) if base else Path.home() / ".cache"
    return root / "copilot-statusline"


def dig(obj: object, *path: str, default: object = None) -> object:
    for key in path:
        if not isinstance(obj, dict):
            return default
        obj = obj.get(key)
        if obj is None:
            return default
    return obj


def as_int(value: object, default: int = 0) -> int:
    try:
        return int(float(value))
    except (TypeError, ValueError):
        return default


def as_float(value: object, default: float = 0.0) -> float:
    try:
        return float(value)
    except (TypeError, ValueError):
        return default


def slug(value: str) -> str:
    return (
        "".join(ch if ch.isalnum() or ch in "-_" else "_" for ch in value)[:64]
        or "default"
    )


def pct_color(pct: int) -> str:
    if pct < 50:
        return GREEN
    if pct < 80:
        return YELLOW
    return RED


def context_segment(used_pct: int, tokens: int, limit: int, compact_pct: float) -> str:
    used_pct = max(0, min(100, used_pct))
    compact_pct = max(0.0, min(100.0, compact_pct))
    token_text = f"{GRAY}◌ {human(tokens)}/{human(limit)}{RESET}"
    if limit <= 0:
        return token_text

    filled_cells = min(10, used_pct // 10)
    threshold_cell = min(9, max(0, math.ceil(compact_pct / 10) - 1))
    cells = ["#" if index < filled_cells else "-" for index in range(10)]
    cells[threshold_cell] = "·"
    colored_cells = []
    for index, cell in enumerate(cells):
        if index == threshold_cell:
            colored_cells.append(f"{YELLOW}{ITALIC}{cell}{RESET}")
        elif cell == "#":
            colored_cells.append(f"{pct_color(used_pct)}{cell}{RESET}")
        else:
            colored_cells.append(f"{GRAY}{cell}{RESET}")
    threshold_text = f"{YELLOW}{ITALIC}{compact_pct:g}%{RESET}"
    return (
        f"{YELLOW}◉{RESET} [{''.join(colored_cells)}] "
        f"{pct_color(used_pct)}{used_pct}%{RESET}  "
        f"{token_text}  {threshold_text}"
    )


def human(tokens: int) -> str:
    if tokens >= 1_000_000:
        tenths = tokens // 100_000
        return (
            f"{tenths // 10}M" if tenths % 10 == 0 else f"{tenths // 10}.{tenths % 10}M"
        )
    if tokens >= 1_000:
        return f"{tokens // 1000}k"
    return str(tokens)


def _supports_256_color() -> bool:
    return (
        os.environ.get("COPILOT_STATUS_256_COLOR", "1") != "0"
        and os.environ.get("TERM", "") != "dumb"
    )


def vendor_color(vendor: object) -> str:
    if vendor == "claude" and not _supports_256_color():
        return YELLOW
    return VENDOR_COLORS.get(str(vendor), "")


def reasoning_style(level: str) -> str:
    normalized = level.strip().lower()
    if not normalized:
        return ""
    color = REASONING_COLORS.get(normalized, "")
    return f"{color}[{level}]{RESET}" if color else f"[{level}]"


def session_identifier(data: Json) -> str:
    for field in ("session_id", "sessionId"):
        value = data.get(field)
        if isinstance(value, str) and value.strip():
            return slug(value)
    return "default"


def model_segment(model_id: str, metadata: Json | None, reasoning: str = "") -> str:
    model_color = vendor_color(metadata.get("vendor")) if metadata else ""
    verdicts = metadata.get("verdicts", []) if metadata else []
    if not isinstance(verdicts, list):
        verdicts = []
    verdict_symbols = []
    verdict_attributes = ""
    for verdict in verdicts:
        style = VERDICT_STYLES.get(str(verdict))
        if style is None:
            continue
        icon, attributes = style
        verdict_symbols.append(icon)
        verdict_attributes += attributes

    model_style = model_color + verdict_attributes
    model_text = f"({model_style}{model_id}{RESET})" if model_style else f"({model_id})"

    parts = [model_text]
    if verdict_symbols:
        parts.insert(0, "".join(verdict_symbols))
    styled_reasoning = reasoning_style(reasoning)
    if styled_reasoning:
        parts.append(styled_reasoning)
    return " ".join(parts)


def context_cost(window: object, rates: Json) -> float:
    """Dollars for the whole session, each token class at its own rate.

    Priced per class rather than against one blended rate: cache reads bill an
    order of magnitude below fresh input, so a single rate overstates a
    well-cached session badly. `total_reasoning_tokens` is assumed to be already
    inside `total_output_tokens` and is not added -- unverified, but if it is
    wrong the figure comes out low, which is the safe direction.
    """
    if not rates:
        return 0.0
    priced = (
        ("total_input_tokens", rates.get("input")),
        ("total_output_tokens", rates.get("output")),
        ("total_cache_read_tokens", rates.get("cache_read")),
        ("total_cache_write_tokens", rates.get("cache_write")),
    )
    total = sum(as_int(dig(window, key)) * as_float(rate) for key, rate in priced)
    return total / PER_MILLION


def message_cost(window: object, rates: Json) -> float:
    """Lower bound on one more turn: the cost of resending the context.

    From `last_call_input_tokens`, not `total_input_tokens` -- the latter is a
    running sum that grows without bound, so dividing it by anything inflates
    the figure as the session goes on. The reply's output tokens are not
    knowable in advance and are left out.
    """
    if not rates:
        return 0.0
    tokens = as_int(dig(window, "last_call_input_tokens"))
    return tokens * as_float(rates.get("input")) / PER_MILLION


def cache_hit_ratio(window: object) -> float | None:
    """Share of prompt tokens served from cache, the main efficiency lever."""
    fresh = as_int(dig(window, "total_input_tokens"))
    cached = as_int(dig(window, "total_cache_read_tokens"))
    total = fresh + cached
    return cached / total if total > 0 else None


def pulse(now: float) -> str:
    """Underline that alternates once a second, so the alert flashes instead of
    sitting there. Phase comes off the wall clock rather than a stored counter,
    which keeps it a steady square wave however irregularly the line repaints --
    and with refreshInterval at an odd number of seconds it also flips on every
    idle repaint rather than aliasing to one phase. SGR 4 rather than SGR 5:
    Windows Terminal ignores blink.
    """
    return UNDERLINE if int(now) % 2 else ""


def cache_write_alert(window: object, rates: Json) -> str:
    """Cache writes bill at one rate for a 5m TTL and a higher one for 1h, but
    the payload reports a single undifferentiated count. context_cost() prices
    the cheaper of the two, so say so once the gap is worth knowing. Silent for
    every non-Anthropic model, which does not bill cache writes at all.
    """
    if os.environ.get("COPILOT_STATUS_NO_ALERT") == "1":
        return ""
    cheap = as_float(rates.get("cache_write"))
    dear = as_float(rates.get("cache_write_1h"))
    written = as_int(dig(window, "total_cache_write_tokens"))
    if not written or dear <= cheap:
        return ""
    if written * (dear - cheap) / PER_MILLION < CACHE_TTL_ALERT_USD:
        return ""
    return (
        f"{pulse(time.time())}{RED}{BOLD}(!) CACHE_WRITE {human(written)} · "
        f"~${written * dear / PER_MILLION:.2f} if 1h TTL (!){RESET}"
    )


def load_json_file(path: Path) -> Json:
    try:
        with path.open(encoding="utf-8") as handle:
            parsed = json.load(handle)
    except (OSError, ValueError):
        return {}
    if not isinstance(parsed, dict):
        return {}
    return parsed


def model_metadata(model_id: str, table: Json | None = None) -> Json | None:
    if not model_id:
        return None
    catalog = (
        table
        if table is not None
        else load_json_file(copilot_home() / "model-rates.json")
    )
    entry = catalog.get(model_id)
    if isinstance(entry, dict):
        return entry
    matches = [
        (key, value)
        for key, value in catalog.items()
        if isinstance(value, dict) and model_id.startswith(key)
    ]
    if not matches:
        return None
    return max(matches, key=lambda item: len(item[0]))[1]


def counters(session_id: str) -> Json:
    return load_json_file(state_dir() / f"{slug(session_id)}.json")


def record_peak(session_id: str, pct: float) -> None:
    if pct <= 0:
        return
    path = state_dir() / f"{slug(session_id)}.peak"
    try:
        state_dir().mkdir(parents=True, exist_ok=True)
        previous = float(path.read_text(encoding="utf-8").strip())
    except (OSError, ValueError):
        previous = 0.0
    if pct <= previous:
        return
    try:
        tmp = path.with_suffix(".peak.tmp")
        tmp.write_text(f"{pct:.4f}", encoding="utf-8")
        tmp.replace(path)
    except OSError:
        return


def compact_threshold(state: Json) -> float:
    learned = as_float(state.get("observed_compact_pct"))
    if 0.0 < learned <= 100.0:
        return learned
    override = as_float(os.environ.get("COPILOT_COMPACT_PCT"))
    if 0.0 < override <= 100.0:
        return override
    return DEFAULT_COMPACT_PCT


def _recorded_aic_value(value: object, *, nano: bool) -> float | None:
    if value is None:
        return None
    parsed = as_float(value, float("nan"))
    if not math.isfinite(parsed):
        return None
    return parsed / NANO_AIU_PER_AIC if nano else parsed


def _aic_from_formatted(value: object) -> float | None:
    """`ai_used.formatted` is a display string of unspecified shape, so take the
    first number out of it and let the caller fall through to the numeric field
    when there is not one."""
    if not isinstance(value, str):
        return None
    match = re.search(r"-?\d+(?:\.\d+)?", value.replace(",", ""))
    if match is None:
        return None
    parsed = as_float(match.group(), float("nan"))
    return parsed if math.isfinite(parsed) else None


def authoritative_aic(data: Json, window: object) -> float | None:
    """Session credits as Copilot itself counts them, which beats our own
    arithmetic when the two disagree.

    Only two fields carry this: `ai_used.formatted` and `total_nano_aiu`. Their
    nesting is not documented, so both the payload root and the context window
    are probed. Everything else the previous version looked for was invented,
    including a session-store.db table that appears in no published schema.
    """
    for root in (data, window):
        credits = _aic_from_formatted(dig(root, "ai_used", "formatted"))
        if credits is not None:
            return credits
        for path in (("ai_used", "total_nano_aiu"), ("total_nano_aiu",)):
            nano = _recorded_aic_value(dig(root, *path), nano=True)
            if nano is not None:
                return nano
    return None


def git_state(cwd: str, session_id: str) -> Json:
    state: Json = {
        "branch": "",
        "staged": 0,
        "modified": 0,
        "untracked": 0,
        "ahead": 0,
        "behind": 0,
    }
    if not cwd or not Path(cwd).is_dir():
        return state
    cache = cache_dir() / f"{slug(session_id)}.json"
    try:
        cache_dir().mkdir(parents=True, exist_ok=True)
        if time.time() - cache.stat().st_mtime < GIT_CACHE_TTL:
            cached = json.loads(cache.read_text(encoding="utf-8"))
            if isinstance(cached, dict):
                return cached
    except (OSError, ValueError):
        pass
    try:
        out = subprocess.run(
            ["git", "status", "--porcelain=v2", "--branch", "--untracked-files=normal"],
            cwd=cwd,
            capture_output=True,
            text=True,
            timeout=3,
        )
    except (OSError, subprocess.SubprocessError):
        return state
    if out.returncode == 0:
        for line in out.stdout.splitlines():
            if line.startswith("# branch.head "):
                head = line.split(" ", 2)[2]
                state["branch"] = "" if head == "(detached)" else head
            elif line.startswith("# branch.ab "):
                parts = line.split()
                state["ahead"] = abs(as_int(parts[2]))
                state["behind"] = abs(as_int(parts[3]))
            elif line.startswith("1 ") or line.startswith("2 "):
                xy = line.split(" ", 2)[1]
                if xy[0] != ".":
                    state["staged"] = as_int(state["staged"]) + 1
                if xy[1] != ".":
                    state["modified"] = as_int(state["modified"]) + 1
            elif line.startswith("? "):
                state["untracked"] = as_int(state["untracked"]) + 1
    try:
        tmp = cache.with_suffix(".tmp")
        tmp.write_text(json.dumps(state), encoding="utf-8")
        tmp.replace(cache)
    except OSError:
        pass
    return state


def branch_segment(git: Json) -> str:
    branch = git.get("branch")
    if not isinstance(branch, str) or not branch:
        return ""
    segment = f"{SEP}{CYAN}{branch}{RESET}"
    marks = []
    if as_int(git.get("staged")):
        marks.append(f"{GREEN}+{as_int(git.get('staged'))}{RESET}")
    if as_int(git.get("modified")):
        marks.append(f"{YELLOW}~{as_int(git.get('modified'))}{RESET}")
    if as_int(git.get("untracked")):
        marks.append(f"{GRAY}?{as_int(git.get('untracked'))}{RESET}")
    if marks:
        segment += " " + " ".join(marks)
    sync = []
    if as_int(git.get("ahead")):
        sync.append(f"\u2191{as_int(git.get('ahead'))}")
    if as_int(git.get("behind")):
        sync.append(f"\u2193{as_int(git.get('behind'))}")
    if sync:
        segment += f" {MAGENTA}{''.join(sync)}{RESET}"
    return segment


def _threshold(name: str, default: float) -> float:
    value = as_float(os.environ.get(name), default)
    return value if value > 0 else default


def activity_thresholds() -> dict[str, float]:
    defaults = {
        "tools_warn": 10.0,
        "tools_alarm": 20.0,
        "turns_warn": 5.0,
        "turns_alarm": 10.0,
        "ratio_warn": 4.0,
        "ratio_alarm": 8.0,
    }
    thresholds = {
        key: _threshold(f"COPILOT_STATUS_{key.upper()}", default)
        for key, default in defaults.items()
    }
    for kind in ("tools", "turns", "ratio"):
        warn = thresholds[f"{kind}_warn"]
        alarm = thresholds[f"{kind}_alarm"]
        if alarm <= warn:
            thresholds[f"{kind}_warn"] = defaults[f"{kind}_warn"]
            thresholds[f"{kind}_alarm"] = defaults[f"{kind}_alarm"]
    return thresholds


def _severity(value: float, warn: float, alarm: float) -> int:
    if value >= alarm:
        return 2
    if value >= warn:
        return 1
    return 0


def _activity_color(base: str, severity: int) -> str:
    if severity >= 2:
        return RED
    if severity == 1:
        return YELLOW
    return base


def activity_segment(state: Json) -> str:
    tools = as_int(state.get("tool_calls"))
    turns = as_int(state.get("turns"))
    errors = as_int(state.get("errors"))
    agents = as_int(state.get("agents"))
    thresholds = activity_thresholds()
    ratio = tools / turns if turns > 0 else 0.0
    ratio_severity = _severity(
        ratio, thresholds["ratio_warn"], thresholds["ratio_alarm"]
    )
    tools_severity = max(
        _severity(tools, thresholds["tools_warn"], thresholds["tools_alarm"]),
        ratio_severity,
    )
    turns_severity = max(
        _severity(turns, thresholds["turns_warn"], thresholds["turns_alarm"]),
        ratio_severity,
    )
    tools_color = _activity_color(CYAN, tools_severity)
    turns_color = _activity_color(GRAY, turns_severity)
    error_color = RED if errors else GRAY
    return " \u25e6 ".join(
        (
            f"{tools_color}⌕ tools({tools}){RESET}",
            f"{turns_color}⟳ turns({turns}){RESET}",
            f"{error_color}✕ errors({errors}){RESET}",
            f"{MAGENTA}⌬ agents({agents}){RESET}",
        )
    )


def duration_segment(ms: int) -> str:
    if ms <= 0:
        return ""
    total = ms // 1000
    hours, remainder = divmod(total, 3600)
    minutes, seconds = divmod(remainder, 60)
    if hours:
        return f"{GRAY}{hours}:{minutes:02d}:{seconds:02d}{RESET}"
    return f"{GRAY}{minutes}:{seconds:02d}{RESET}"


def main() -> int:
    try:
        data = json.load(sys.stdin)
    except (ValueError, OSError):
        print(f"{GRAY}statusline: bad input{RESET}")
        return 0
    if not isinstance(data, dict):
        print(f"{GRAY}statusline: bad input{RESET}")
        return 0

    model_id = str(dig(data, "model", "id", default=""))
    model = model_id or str(dig(data, "model", "display_name", default="copilot"))
    model_table = load_json_file(copilot_home() / "model-rates.json")
    metadata = model_metadata(model_id, model_table)
    reasoning = str(
        dig(
            data,
            "reasoning_effort",
            default=dig(
                data,
                "effort_level",
                default=dig(data, "effortLevel", default=""),
            ),
        )
    )
    cwd = str(
        dig(
            data,
            "workspace",
            "current_dir",
            default=dig(data, "cwd", default=os.getcwd()),
        )
    )
    session_id = session_identifier(data)
    version = dig(data, "version", default="")
    dir_name = os.path.basename(cwd.replace("\\", "/").rstrip("/")) or cwd

    window = data.get("context_window")
    used_pct = as_int(
        dig(
            window,
            "current_context_used_percentage",
            default=dig(window, "used_percentage"),
        )
    )
    tokens = as_int(dig(window, "current_context_tokens"))
    limit = as_int(
        dig(
            window,
            "displayed_context_limit",
            default=dig(window, "context_window_size"),
        )
    )

    state = counters(session_id)
    record_peak(session_id, as_float(dig(window, "current_context_used_percentage")))
    threshold = compact_threshold(state)

    line1 = f"{model_segment(model, metadata, reasoning)}{SEP}{dir_name}"
    line1 += branch_segment(git_state(cwd, session_id))
    if version:
        line1 += f"{SEP}{GRAY}v{version}{RESET}"

    line2 = context_segment(used_pct, tokens, limit, threshold)
    line2 += f"{SEP}{activity_segment(state)}"

    # Line 3 is what the session has spent. There is no budget gauge: the plan
    # here is enterprise with no usage cap, so a percentage-of-quota bar would
    # be measuring against nothing. Absolute figures instead.
    rates = metadata or {}
    session_usd = context_cost(window, rates)
    credits = authoritative_aic(data, window)
    duration_ms = as_int(dig(data, "cost", "total_duration_ms"))

    spend = []
    alert = cache_write_alert(window, rates)
    if alert:
        spend.append(alert)
    if session_usd > 0:
        spend.append(f"{BOLD}${session_usd:.2f}{RESET}")
    elif not rates:
        billed = as_int(dig(window, "total_input_tokens")) + as_int(
            dig(window, "total_output_tokens")
        )
        if billed > 0:
            spend.append(f"{GRAY}{human(billed)} tok, no rate{RESET}")
    if credits is not None:
        spend.append(f"{GRAY}{credits:.0f} aic{RESET}")
    per_message = message_cost(window, rates)
    if per_message > 0:
        spend.append(f"{GRAY}~${per_message:.2f}/msg{RESET}")
    if session_usd > 0 and duration_ms >= BURN_MIN_MS:
        spend.append(f"{GRAY}${session_usd * MS_PER_HOUR / duration_ms:.2f}/hr{RESET}")
    hit_ratio = cache_hit_ratio(window)
    if hit_ratio is not None:
        spend.append(f"{GRAY}cache {hit_ratio:.0%}{RESET}")
    elapsed = duration_segment(duration_ms)
    if elapsed:
        spend.append(elapsed)

    print(line1)
    print(line2)
    if spend:
        print(SEP.join(spend))
    return 0


if __name__ == "__main__":
    sys.exit(main())
