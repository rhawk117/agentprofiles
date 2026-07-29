#!/usr/bin/env python3
import argparse
import collections
import json
from pathlib import PurePosixPath

TOOLS = {"Edit", "Write", "NotebookEdit", "MultiEdit"}
REPLACEMENT_EST = 1500


def norm(path: str, cwd: str) -> PurePosixPath | None:
    if not path:
        return None
    p = PurePosixPath(path)
    if not p.is_absolute():
        p = PurePosixPath(cwd or "/") / p
    return p


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--log", required=True)
    ap.add_argument("--threshold", type=int, default=10000)
    ap.add_argument("--replacement", type=int, default=REPLACEMENT_EST)
    args = ap.parse_args()

    events = []
    for line in open(args.log):
        line = line.strip()
        if line:
            try:
                events.append(json.loads(line))
            except ValueError:
                pass

    total_all = sum(e.get("out_bytes") or 0 for e in events)
    capped = 0
    saved = 0
    by_ext = collections.Counter()
    by_ext_n = collections.Counter()
    by_session = collections.Counter()
    edit_total = 0

    for e in events:
        size = e.get("out_bytes") or 0
        if e.get("tool") in TOOLS:
            edit_total += size
        if e.get("tool") not in TOOLS or size <= args.threshold:
            continue
        p = norm(e.get("path") or "", e.get("cwd") or "")
        ext = (p.suffix if p else "") or "(none)"
        delta = max(0, size - args.replacement)
        capped += 1
        saved += delta
        by_ext[ext] += delta
        by_ext_n[ext] += 1
        by_session[e.get("session")] += delta

    print(f"threshold              {args.threshold:,} bytes")
    print(f"events                 {len(events):,}")
    print(f"total tool output      {total_all:,} bytes  (~{total_all // 4:,} tok)")
    print(f"edit/write output      {edit_total:,} bytes  (~{edit_total // 4:,} tok)")
    print()
    print(f"calls capped           {capped}")
    print(f"bytes saved            {saved:,}  (~{saved // 4:,} tok)")
    print(f"  as share of edits    {saved / edit_total:.1%}" if edit_total else "")
    print(f"  as share of all      {saved / total_all:.1%}" if total_all else "")
    print()
    print("saving by extension")
    for ext, b in by_ext.most_common(8):
        print(
            f"  {ext:<10} {by_ext_n[ext]:>4} calls  {b:>12,} bytes  (~{b // 4:>9,} tok)"
        )
    print()
    worst = by_session.most_common(3)
    print("worst sessions")
    for s, b in worst:
        print(f"  {str(s)[:8]}  {b:>12,} bytes saved  (~{b // 4:,} tok)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
