#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

type Json = dict[str, object]

KEYS = {
    "tool": "tool_calls",
    "turn": "turns",
    "error": "errors",
    "agent": "agents",
}


def copilot_home() -> Path:
    override = os.environ.get("COPILOT_HOME")
    if override:
        return Path(override)
    return Path.home() / ".copilot"


def state_dir() -> Path:
    return copilot_home() / "statusline-state"


def read_payload() -> Json:
    try:
        raw = sys.stdin.read()
    except Exception:
        return {}
    if not raw.strip():
        return {}
    try:
        parsed = json.loads(raw)
    except Exception:
        return {}
    if not isinstance(parsed, dict):
        return {}
    return parsed


def session_id(payload: Json) -> str | None:
    for field in ("sessionId", "session_id"):
        value = payload.get(field)
        if isinstance(value, str) and value.strip():
            return "".join(c for c in value.strip() if c.isalnum() or c in "-_")
    return None


def load(path: Path) -> Json:
    try:
        with path.open(encoding="utf-8") as handle:
            parsed = json.load(handle)
    except Exception:
        return {}
    if not isinstance(parsed, dict):
        return {}
    return parsed


def save(path: Path, state: Json) -> None:
    temp = path.with_suffix(".json.tmp")
    try:
        state_dir().mkdir(parents=True, exist_ok=True)
        temp.write_text(json.dumps(state), encoding="utf-8")
        temp.replace(path)
    except Exception:
        return


def bump(path: Path, key: str) -> None:
    state = load(path)
    current = state.get(key)
    state[key] = (int(current) if isinstance(current, int) else 0) + 1
    save(path, state)


def absorb_peak(path: Path, sid: str) -> None:
    peak_path = state_dir() / f"{sid}.peak"
    state = load(path)
    try:
        observed = float(peak_path.read_text(encoding="utf-8").strip())
    except Exception:
        observed = 0.0
    if 0.0 < observed <= 100.0:
        state["observed_compact_pct"] = round(observed, 2)
    current = state.get("compactions")
    state["compactions"] = (int(current) if isinstance(current, int) else 0) + 1
    save(path, state)
    try:
        peak_path.unlink()
    except Exception:
        return


def reset(path: Path, sid: str) -> None:
    carried = load(path).get("observed_compact_pct")
    state: Json = {
        "tool_calls": 0,
        "turns": 0,
        "errors": 0,
        "agents": 0,
        "compactions": 0,
    }
    if isinstance(carried, (int, float)):
        state["observed_compact_pct"] = carried
    save(path, state)
    try:
        (state_dir() / f"{sid}.peak").unlink()
    except Exception:
        return


def prune(keep: str) -> None:
    try:
        entries = sorted(
            state_dir().glob("*.json"), key=lambda p: p.stat().st_mtime, reverse=True
        )
    except Exception:
        return
    for stale in entries[24:]:
        if stale.stem == keep:
            continue
        try:
            stale.unlink()
            stale.with_suffix(".peak").unlink(missing_ok=True)
        except Exception:
            continue


def main() -> int:
    if len(sys.argv) < 2:
        return 0
    action = sys.argv[1]
    payload = read_payload()
    sid = session_id(payload)
    if not sid:
        return 0
    path = state_dir() / f"{sid}.json"

    if action == "reset":
        reset(path, sid)
        prune(sid)
        return 0
    if action == "compact":
        absorb_peak(path, sid)
        return 0
    key = KEYS.get(action)
    if key is None:
        return 0
    bump(path, key)
    return 0


if __name__ == "__main__":
    sys.exit(main())
