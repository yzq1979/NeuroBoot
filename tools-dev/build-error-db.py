#!/usr/bin/env python3
"""build-error-db.py --- NeuroBoot v3.0 W5-6 errordb.sqlite build script.

Two execution modes:

1. **Phase 1 (this session) - fixture mode**:
       python build-error-db.py --fixtures-only --no-embed -o errordb.sqlite

   Loads `tools-dev/fixtures/error-fixtures.json` (~50 hand-written entries)
   into a brand new SQLite db with:
     - `entries` table  (text columns + optional embedding BLOB)
     - `entries_fts` FTS5 virtual table (trigram tokenizer for CJK + Latin)

   `--no-embed` skips calling llama-server's /v1/embeddings endpoint and
   leaves the embedding column NULL.  The Rust runtime detects this and
   falls back to FTS5-only retrieval.

2. **Phase 2 (next session) - full mode**:
       python build-error-db.py --crawl --embedding-url http://127.0.0.1:8080 \\
                                -o errordb.sqlite

   Same as fixture mode but additionally:
     - crawls Microsoft Learn pages (Bug Check Code Reference + Win32 system
       error codes) to expand entry count from ~50 to ~17k (TODO: not wired
       up yet; Phase 2 will implement crawl_microsoft_docs()).
     - computes Qwen3-Embedding vectors for each entry's `name + cn_desc +
       causes + keywords` concatenation via the OpenAI-compatible
       /v1/embeddings endpoint and stores them as a BLOB.

The schema is deliberately kept simple: no sqlite-vec virtual table yet
(that's Phase 2 too -- we'd add `entries_vec` USING vec0(...)` once we have
real embeddings). FTS5 trigram tokenizer is good enough for ad-hoc error
code queries in Phase 1.

Pure stdlib (no requests / beautifulsoup deps yet) -- intentional. Phase 2
crawler will add `requests beautifulsoup4` to a `--crawl`-only branch.
"""

from __future__ import annotations

import argparse
import json
import sqlite3
import sys
import urllib.request
import urllib.error
import struct
from pathlib import Path

# Default embedding dim. Qwen3-Embedding-0.6B outputs 1024-dim float vectors.
# We'd autodetect this in Phase 2 by issuing a probe embedding and reading
# the returned length; for Phase 1 (--no-embed) the dim is unused.
EMBEDDING_DIM_DEFAULT = 1024


def init_db(db_path: Path) -> sqlite3.Connection:
    """Open / create the db and apply Phase 1 schema."""
    db_path.parent.mkdir(parents=True, exist_ok=True)
    if db_path.exists():
        db_path.unlink()

    conn = sqlite3.connect(str(db_path))
    cur = conn.cursor()

    # Core table -- one row per error code / tool description.
    # embedding column is NULL until Phase 2 wires up /v1/embeddings calls.
    cur.execute(
        """
        CREATE TABLE entries (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            kind         TEXT NOT NULL,
            code         TEXT,
            name         TEXT,
            cn_desc      TEXT NOT NULL,
            causes       TEXT,
            docs_url     TEXT,
            keywords     TEXT,
            embedding    BLOB
        )
        """
    )
    cur.execute("CREATE INDEX idx_entries_code ON entries(code)")
    cur.execute("CREATE INDEX idx_entries_kind ON entries(kind)")

    # FTS5 virtual table with trigram tokenizer.
    # Trigram (3-char rolling window) works for both Chinese and Latin --
    # CJK queries like "蓝屏 启动盘" find matches in cn_desc / causes without
    # explicit word segmentation, and English queries like "irql" still
    # match "IRQL_NOT_LESS_OR_EQUAL".
    # We declare `code` UNINDEXED so the code column is stored but not
    # tokenized -- code-equality lookup goes through the regular SQL
    # index above instead.
    cur.execute(
        """
        CREATE VIRTUAL TABLE entries_fts USING fts5(
            name,
            cn_desc,
            causes,
            keywords,
            code UNINDEXED,
            content='entries',
            content_rowid='id',
            tokenize='trigram'
        )
        """
    )

    # Triggers keep FTS5 in sync with the base table.
    cur.executescript(
        """
        CREATE TRIGGER entries_ai AFTER INSERT ON entries BEGIN
            INSERT INTO entries_fts(rowid, name, cn_desc, causes, keywords, code)
            VALUES (new.id, new.name, new.cn_desc, new.causes, new.keywords, new.code);
        END;
        CREATE TRIGGER entries_ad AFTER DELETE ON entries BEGIN
            INSERT INTO entries_fts(entries_fts, rowid, name, cn_desc, causes, keywords, code)
            VALUES ('delete', old.id, old.name, old.cn_desc, old.causes, old.keywords, old.code);
        END;
        CREATE TRIGGER entries_au AFTER UPDATE ON entries BEGIN
            INSERT INTO entries_fts(entries_fts, rowid, name, cn_desc, causes, keywords, code)
            VALUES ('delete', old.id, old.name, old.cn_desc, old.causes, old.keywords, old.code);
            INSERT INTO entries_fts(rowid, name, cn_desc, causes, keywords, code)
            VALUES (new.id, new.name, new.cn_desc, new.causes, new.keywords, new.code);
        END;
        """
    )

    # Metadata table -- schema version + build timestamp + embedding info.
    cur.execute(
        """
        CREATE TABLE db_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )
        """
    )
    cur.execute("INSERT INTO db_meta(key, value) VALUES ('schema_version', '1')")
    cur.execute("INSERT INTO db_meta(key, value) VALUES ('has_embeddings', '0')")

    conn.commit()
    return conn


def insert_fixtures(conn: sqlite3.Connection, fixtures_path: Path) -> int:
    """Insert all entries from the fixture JSON. Returns count inserted."""
    payload = json.loads(fixtures_path.read_text(encoding="utf-8"))
    entries = payload["entries"]
    cur = conn.cursor()
    for entry in entries:
        cur.execute(
            """
            INSERT INTO entries (kind, code, name, cn_desc, causes, docs_url, keywords)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (
                entry["kind"],
                entry.get("code", "") or None,
                entry.get("name", "") or None,
                entry["cn_desc"],
                entry.get("causes", "") or None,
                entry.get("docs_url", "") or None,
                entry.get("keywords", "") or None,
            ),
        )
    conn.commit()
    return len(entries)


def embedding_text(entry_row: tuple) -> str:
    """Build the concatenated text we feed to the embedding model.

    Layout: `<name>. <cn_desc>. <causes>. <keywords>` -- name first
    so vector similarity weights mnemonic matches highly.
    """
    name, cn_desc, causes, keywords = entry_row
    parts = [p for p in (name, cn_desc, causes, keywords) if p]
    return ". ".join(parts)


def fetch_embedding(endpoint: str, model: str, text: str) -> list[float]:
    """POST to OpenAI-compatible /v1/embeddings; return float vector."""
    url = endpoint.rstrip("/") + "/v1/embeddings"
    body = json.dumps({"model": model, "input": text}).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            data = json.loads(resp.read().decode("utf-8"))
    except urllib.error.URLError as e:
        raise RuntimeError(f"embedding endpoint {url} unreachable: {e}") from e
    return data["data"][0]["embedding"]


def pack_vec(vec: list[float]) -> bytes:
    """Pack float list into little-endian f32 BLOB."""
    return struct.pack(f"<{len(vec)}f", *vec)


def populate_embeddings(conn: sqlite3.Connection, endpoint: str, model: str) -> int:
    """For each entry without an embedding, call /v1/embeddings and write BLOB."""
    cur = conn.cursor()
    cur.execute("SELECT id, name, cn_desc, causes, keywords FROM entries WHERE embedding IS NULL")
    rows = cur.fetchall()
    if not rows:
        return 0

    print(f"[embed] {len(rows)} entries to embed via {endpoint}")
    update_cur = conn.cursor()
    count = 0
    for row in rows:
        rid, name, cn_desc, causes, keywords = row
        text = embedding_text((name, cn_desc, causes, keywords))
        vec = fetch_embedding(endpoint, model, text)
        update_cur.execute(
            "UPDATE entries SET embedding = ? WHERE id = ?",
            (pack_vec(vec), rid),
        )
        count += 1
        if count % 10 == 0:
            conn.commit()
            print(f"[embed]   {count}/{len(rows)}")

    conn.commit()
    # Mark in metadata that this db has real embeddings.
    cur.execute("UPDATE db_meta SET value = '1' WHERE key = 'has_embeddings'")
    cur.execute(
        "INSERT OR REPLACE INTO db_meta(key, value) VALUES ('embedding_dim', ?)",
        (str(len(vec)),),
    )
    cur.execute(
        "INSERT OR REPLACE INTO db_meta(key, value) VALUES ('embedding_model', ?)",
        (model,),
    )
    conn.commit()
    return count


def crawl_microsoft_docs(conn: sqlite3.Connection) -> int:
    """Phase 2 TODO: crawl Microsoft Learn for BugCheck + Win32 error codes.

    Will add `requests` + `beautifulsoup4` to a venv-only optional dep set
    and emit ~17k rows. Skipped in Phase 1.
    """
    print("[crawl] Phase 2 work, not implemented yet -- skipping. Use --fixtures-only.")
    return 0


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("-o", "--output", default="errordb.sqlite", help="output sqlite path")
    p.add_argument("--fixtures-only", action="store_true", help="only load fixtures (no crawl)")
    p.add_argument("--crawl", action="store_true", help="crawl Microsoft docs (Phase 2 only, NYI)")
    p.add_argument("--no-embed", action="store_true", help="skip embedding generation")
    p.add_argument(
        "--embedding-url",
        default="http://127.0.0.1:8080",
        help="OpenAI-compatible /v1/embeddings host (llama-server with --embedding)",
    )
    p.add_argument(
        "--embedding-model",
        default="qwen3-embedding-0.6b",
        help="model name passed to /v1/embeddings request body",
    )
    p.add_argument(
        "--fixtures",
        default=str(Path(__file__).parent / "fixtures" / "error-fixtures.json"),
        help="path to fixture JSON",
    )
    args = p.parse_args()

    if not args.fixtures_only and not args.crawl:
        p.error("must pass --fixtures-only or --crawl (or both)")

    out_path = Path(args.output).resolve()
    fixtures_path = Path(args.fixtures).resolve()

    if args.fixtures_only and not fixtures_path.is_file():
        p.error(f"fixtures file not found: {fixtures_path}")

    print(f"[init] creating fresh db at {out_path}")
    conn = init_db(out_path)

    inserted_fixtures = 0
    inserted_crawl = 0
    embedded = 0

    if args.fixtures_only:
        inserted_fixtures = insert_fixtures(conn, fixtures_path)
        print(f"[fixtures] inserted {inserted_fixtures} entries from {fixtures_path.name}")

    if args.crawl:
        inserted_crawl = crawl_microsoft_docs(conn)

    if not args.no_embed:
        try:
            embedded = populate_embeddings(conn, args.embedding_url, args.embedding_model)
        except RuntimeError as e:
            print(f"[embed] FAILED: {e}", file=sys.stderr)
            print("[embed] continuing without embeddings (db will be FTS5-only)", file=sys.stderr)

    conn.close()

    final_size_kb = out_path.stat().st_size / 1024
    print(
        f"[done] {out_path.name} = {final_size_kb:.1f} KB "
        f"(fixtures: {inserted_fixtures}, crawl: {inserted_crawl}, embedded: {embedded})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
