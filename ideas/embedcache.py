#!/usr/bin/env python3
"""
idea: use a more efficient embed cache for frequently accessed files similar to copilot
"""
import argparse
import array
import json
import math
import sqlite3
import sys
from pathlib import Path

type Vector = array.array
type Entry = tuple[str, str, str, Vector]

SCHEMA = """
CREATE TABLE IF NOT EXISTS embeddings (
    id          TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    text_hash   TEXT NOT NULL,
    dim         INTEGER NOT NULL,
    content     TEXT NOT NULL,
    metadata    TEXT NOT NULL DEFAULT '{}',
    embedding   BLOB NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (id, provider_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_embeddings_hash ON embeddings (text_hash, provider_id);
"""


def connect(path: Path, create: bool) -> sqlite3.Connection:
    conn = sqlite3.connect(path)
    conn.execute("pragma journal_mode=wal")
    conn.execute("pragma synchronous=normal")
    if create:
        conn.executescript(SCHEMA)
    return conn


def pack(values: list[float]) -> bytes:
    vec = array.array("f", values)
    if sys.byteorder != "little":
        vec.byteswap()
    return vec.tobytes()


def unpack(blob: bytes) -> Vector:
    vec = array.array("f")
    vec.frombytes(blob)
    if sys.byteorder != "little":
        vec.byteswap()
    return vec


def normalize(values: list[float] | Vector) -> Vector:
    length = math.sqrt(sum(v * v for v in values))
    if length == 0.0:
        return array.array("f", values)
    return array.array("f", (v / length for v in values))


def cosine(a: Vector, b: Vector) -> float:
    return sum(x * y for x, y in zip(a, b))


def load_source(path: Path) -> list[Entry]:
    if not path.is_file():
        raise SystemExit(f"no cache at {path}")
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        rows = conn.execute(
            "select id, content, metadata, embedding, created_at from embeddings"
        ).fetchall()
    except sqlite3.DatabaseError as exc:
        raise SystemExit(f"cannot read {path}: {exc}")
    finally:
        conn.close()
    entries: list[Entry] = []
    for entry_id, content, metadata, embedding, created in rows:
        try:
            values = (
                json.loads(embedding)
                if isinstance(embedding, str)
                else list(unpack(embedding))
            )
        except ValueError, TypeError:
            continue
        entries.append(
            (entry_id, content, metadata or "{}", normalize(values), created)
        )
    return entries


def newest_per_id(entries: list) -> list:
    best: dict[str, tuple] = {}
    for item in entries:
        current = best.get(item[0])
        if current is None or item[4] >= current[4]:
            best[item[0]] = item
    return sorted(best.values())


def cmd_stats(args: argparse.Namespace) -> int:
    path = Path(args.db)
    if not path.is_file():
        print(f"no cache at {path}", file=sys.stderr)
        return 1
    try:
        conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
        conn.execute("select 1 from embeddings limit 1")
    except sqlite3.DatabaseError as exc:
        print(f"cannot read {path}: {exc}", file=sys.stderr)
        return 1
    total = conn.execute("select count(*) from embeddings").fetchone()[0]
    distinct = conn.execute("select count(distinct id) from embeddings").fetchone()[0]
    providers = conn.execute(
        "select provider_id, count(*) from embeddings group by 1"
    ).fetchall()
    span = conn.execute(
        "select min(created_at), max(created_at) from embeddings"
    ).fetchone()
    sample = conn.execute("select embedding from embeddings limit 1").fetchone()
    conn.close()

    dim = 0
    text_bytes = 0
    if sample:
        blob = sample[0]
        if isinstance(blob, str):
            dim = len(json.loads(blob))
            text_bytes = 1
        else:
            dim = len(unpack(blob))

    size = path.stat().st_size
    print(f"file            {path}")
    print(f"size            {size:,} bytes")
    print(f"rows            {total}")
    print(f"distinct ids    {distinct}")
    stale = total - distinct
    share = f" ({100 * stale / total:.0f}% of rows)" if total else ""
    print(f"stale rows      {stale}{share}")
    print(f"dimensions      {dim}")
    print(f"written         {span[0]} .. {span[1]}")
    for provider, count in providers:
        print(f"provider        {provider} x{count}")
    if text_bytes and dim and total:
        blob_bytes = total * dim * 4
        print(
            f"storage         JSON TEXT; float32 BLOB would use ~{blob_bytes:,} bytes for the vectors"
        )
    return 0


def cmd_audit(args: argparse.Namespace) -> int:
    entries = newest_per_id(load_source(Path(args.db)))
    if len(entries) < 2:
        print("not enough entries to audit")
        return 1
    ids = [e[0] for e in entries]
    vecs = [e[3] for e in entries]
    count = len(ids)

    pairs = []
    nearest = [(-1.0, -1.0, -1)] * count
    for i in range(count):
        best = second = -1.0
        best_j = -1
        for j in range(count):
            if i == j:
                continue
            sim = cosine(vecs[i], vecs[j])
            if j > i and sim >= args.threshold:
                pairs.append((sim, ids[i], ids[j]))
            if sim > best:
                second, best, best_j = best, sim, j
            elif sim > second:
                second = sim
        nearest[i] = (best, second, best_j)

    pairs.sort(reverse=True)
    print(f"== confusable pairs at cosine >= {args.threshold} ==")
    if not pairs:
        print("  none")
    for sim, left, right in pairs[: args.limit]:
        print(f"  {sim:.3f}  {left}  <->  {right}")

    print("\n== weakest separation (nearest neighbour barely beats the runner-up) ==")
    ranked = sorted(range(count), key=lambda i: nearest[i][0] - nearest[i][1])
    for i in ranked[: args.limit]:
        best, second, j = nearest[i]
        print(
            f"  margin {best - second:.3f}  {ids[i]:34} nearest={ids[j]:30} sim={best:.3f}"
        )

    print(f"\n== crowding (neighbours above {args.threshold}) ==")
    crowd = sorted(
        (
            (
                sum(
                    1
                    for j in range(count)
                    if j != i and cosine(vecs[i], vecs[j]) >= args.threshold
                ),
                ids[i],
            )
            for i in range(count)
        ),
        reverse=True,
    )
    for score, name in crowd[: args.limit]:
        if score == 0:
            break
        print(f"  {score:3}  {name}")
    return 0


def cmd_search(args: argparse.Namespace) -> int:
    entries = newest_per_id(load_source(Path(args.db)))
    index = {e[0]: e for e in entries}
    target = index.get(args.id) or index.get(f"skill:{args.id}")
    if target is None:
        print(f"no entry with id {args.id!r}", file=sys.stderr)
        return 1
    scored = sorted(
        ((cosine(target[3], e[3]), e[0], e[1]) for e in entries if e[0] != target[0]),
        reverse=True,
    )
    print(f"nearest to {target[0]}")
    for sim, entry_id, content in scored[: args.k]:
        print(f"  {sim:.3f}  {entry_id}")
        print(f"         {content[:110]}")
    return 0


def cmd_import(args: argparse.Namespace) -> int:
    entries = load_source(Path(args.source))
    kept = newest_per_id(entries) if not args.keep_stale else entries
    conn = connect(Path(args.db), create=True)
    written = 0
    for entry_id, content, metadata, vector, created in kept:
        import hashlib

        digest = hashlib.sha256(content.encode()).hexdigest()
        conn.execute(
            "insert into embeddings (id, provider_id, text_hash, dim, content, metadata, embedding,"
            " created_at, updated_at) values (?,?,?,?,?,?,?,?,datetime('now'))"
            " on conflict(id, provider_id) do update set"
            " text_hash=excluded.text_hash, dim=excluded.dim, content=excluded.content,"
            " metadata=excluded.metadata, embedding=excluded.embedding, updated_at=datetime('now')",
            (
                entry_id,
                args.provider,
                digest,
                len(vector),
                content,
                metadata,
                pack(list(vector)),
                created,
            ),
        )
        written += 1
    conn.commit()
    conn.execute("vacuum")
    conn.close()
    dropped = len(entries) - len(kept)
    print(f"imported {written} entries into {args.db}")
    print(f"dropped {dropped} stale duplicate rows")
    print(
        f"size {Path(args.db).stat().st_size:,} bytes (source {Path(args.source).stat().st_size:,})"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(prog="embedcache")
    sub = parser.add_subparsers(dest="command", required=True)

    default = str(Path.home() / ".copilot" / "embedding-cache.db")

    p = sub.add_parser("stats", help="summarize a cache")
    p.add_argument("--db", default=default)
    p.set_defaults(func=cmd_stats)

    p = sub.add_parser("audit", help="find confusable descriptions")
    p.add_argument("--db", default=default)
    p.add_argument("--threshold", type=float, default=0.60)
    p.add_argument("--limit", type=int, default=12)
    p.set_defaults(func=cmd_audit)

    p = sub.add_parser("search", help="nearest neighbours of one entry")
    p.add_argument("id")
    p.add_argument("--db", default=default)
    p.add_argument("-k", type=int, default=8)
    p.set_defaults(func=cmd_search)

    p = sub.add_parser("import", help="copy a cache into the compact schema")
    p.add_argument("source")
    p.add_argument("--db", required=True)
    p.add_argument("--provider", default="text-embedding-3-small")
    p.add_argument("--keep-stale", action="store_true")
    p.set_defaults(func=cmd_import)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
