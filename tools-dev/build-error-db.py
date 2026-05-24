#!/usr/bin/env python3
"""build-error-db.py --- NeuroBoot v3.0 W5-6 errordb.sqlite build script.

## Execution modes

1. **Phase 1 (FTS5 skeleton, no model needed)**:
       python build-error-db.py --fixtures-only --no-embed -o errordb.sqlite
   Loads `tools-dev/fixtures/error-fixtures.json` (~50 entries) into a
   `entries` + `entries_fts` (trigram) sqlite.

2. **Phase 2 fixture + embeddings**:
       python build-error-db.py --fixtures-only \\
                                --embedding-url http://127.0.0.1:8080 \\
                                --embedding-model qwen3-embedding-0.6b \\
                                -o errordb.sqlite
   Same as Phase 1 but additionally calls /v1/embeddings for each entry and
   populates a `entries_vec` USING vec0(...) virtual table. Requires the
   sqlite-vec extension at query time (the Rust side loads it via
   rag::vec_ext::register_once). Run sqlite-vec's CLI loadable extension
   yourself if you want to inspect the db directly.

3. **Phase 2 full crawl + embeddings**:
       python build-error-db.py --crawl \\
                                --embedding-url http://127.0.0.1:8080 \\
                                -o errordb.sqlite
   Adds ~512 BugCheck codes + ~17k Win32/NTSTATUS/HRESULT codes scraped
   from Microsoft Learn. Optional `--with-fixtures` keeps the hand-written
   fixture entries too (with UPSERT semantics keyed on (kind, code)).

   Crawl requires `pip install beautifulsoup4` (the script gracefully
   complains if missing).

## Notes

- Fixtures and crawler use INSERT OR IGNORE on (kind, code) so re-runs
  are idempotent and crawl never overwrites curated Chinese fixtures.
- Embeddings call /v1/embeddings sequentially with a 60s timeout each.
  17k embeds at ~100 ms each = 25 minutes; allow 1-2 hours wall clock.
- The script writes a fresh db every run (deletes existing). Add
  `--resume` once we hit that pain point.
"""

from __future__ import annotations

import argparse
import json
import sqlite3
import sys
import time
import urllib.request
import urllib.error
import struct
from pathlib import Path

DEFAULT_FIXTURE_DIR = Path(__file__).parent / "fixtures"
DEFAULT_FIXTURES_PATH = DEFAULT_FIXTURE_DIR / "error-fixtures.json"

# Polite delay between Microsoft Learn requests to avoid getting rate-limited.
CRAWL_DELAY_S = 0.5


def init_db(db_path: Path) -> sqlite3.Connection:
    """Open / create the db and apply Phase 1+2 schema (no vec0 yet)."""
    db_path.parent.mkdir(parents=True, exist_ok=True)
    if db_path.exists():
        db_path.unlink()

    conn = sqlite3.connect(str(db_path))
    cur = conn.cursor()

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
    # Unique on (kind, code) for kinds that carry a code -- prevents
    # duplicate inserts from crawler runs. tool_desc entries (code IS NULL)
    # get a separate unique-by-name index below.
    cur.execute(
        "CREATE UNIQUE INDEX idx_unique_kind_code ON entries(kind, code) "
        "WHERE code IS NOT NULL AND code != ''"
    )
    cur.execute(
        "CREATE UNIQUE INDEX idx_unique_tool_name ON entries(name) "
        "WHERE kind = 'tool_desc'"
    )

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

    cur.execute(
        """
        CREATE TABLE db_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )
        """
    )
    cur.execute("INSERT INTO db_meta(key, value) VALUES ('schema_version', '2')")
    cur.execute("INSERT INTO db_meta(key, value) VALUES ('has_embeddings', '0')")

    conn.commit()
    return conn


def create_vec_table(conn: sqlite3.Connection, dim: int) -> None:
    """Create the sqlite-vec entries_vec virtual table for the given dim.

    Called from populate_embeddings() once we discover the dim from the
    first /v1/embeddings response. The Python sqlite3 module cannot LOAD
    the sqlite-vec extension itself (would need `conn.enable_load_extension`
    and a vec0.dll path), so we issue a CREATE that's only EVALUATED at
    runtime by the Rust side which has the extension auto-registered.

    Workaround for the chicken-and-egg: we use sqlite3's normal CREATE
    VIRTUAL TABLE syntax. Without the extension loaded in this Python
    process, the table creation fails -- so we need to load the extension
    here too.
    """
    # Try to load the sqlite-vec extension from a few common spots.
    try:
        conn.enable_load_extension(True)
    except sqlite3.OperationalError:
        # Some Python builds disable extension loading at compile time.
        raise RuntimeError(
            "Python's sqlite3 was built without enable_load_extension; "
            "install a Python that supports it (most do) or build the db with "
            "--no-embed and let the Rust side populate entries_vec on first run"
        )
    candidates = [
        # Prefer co-located sqlite-vec build the user dropped here.
        Path(__file__).parent / "sqlite-vec" / "vec0",
        Path(__file__).parent / "sqlite-vec" / "vec0.dll",
        # PyPI install:
        Path(sys.prefix) / "Lib" / "site-packages" / "sqlite_vec" / "vec0",
        Path(sys.prefix) / "Lib" / "site-packages" / "sqlite_vec" / "vec0.dll",
    ]
    loaded = False
    last_err = None
    for c in candidates:
        try:
            conn.load_extension(str(c))
            loaded = True
            break
        except sqlite3.OperationalError as e:
            last_err = e
    if not loaded:
        raise RuntimeError(
            f"Could not load sqlite-vec extension from any of {candidates}. "
            f"Last error: {last_err}. Either run `pip install sqlite-vec` or drop "
            f"vec0.dll into {Path(__file__).parent / 'sqlite-vec'}/"
        )

    conn.execute(f"CREATE VIRTUAL TABLE entries_vec USING vec0(embedding float[{dim}])")
    conn.commit()


def insert_fixtures(conn: sqlite3.Connection, fixtures_path: Path) -> int:
    payload = json.loads(fixtures_path.read_text(encoding="utf-8"))
    entries = payload["entries"]
    cur = conn.cursor()
    inserted = 0
    for entry in entries:
        try:
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
            inserted += 1
        except sqlite3.IntegrityError:
            # Duplicate (kind, code) -- expected when re-running with --with-fixtures.
            continue
    conn.commit()
    return inserted


def embedding_text(name: str | None, cn_desc: str, causes: str | None, keywords: str | None) -> str:
    parts = [p for p in (name, cn_desc, causes, keywords) if p]
    return ". ".join(parts)


def fetch_embedding(endpoint: str, model: str, text: str) -> list[float]:
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
    return struct.pack(f"<{len(vec)}f", *vec)


def populate_embeddings(conn: sqlite3.Connection, endpoint: str, model: str) -> int:
    """Embed every entry without an embedding; INSERT INTO entries_vec too."""
    cur = conn.cursor()
    cur.execute(
        "SELECT id, name, cn_desc, causes, keywords FROM entries WHERE embedding IS NULL"
    )
    rows = cur.fetchall()
    if not rows:
        print("[embed] nothing to embed")
        return 0

    print(f"[embed] {len(rows)} entries to embed via {endpoint}")

    # First request: discover the embedding dim, then create entries_vec.
    first = rows[0]
    first_text = embedding_text(first[1], first[2], first[3], first[4])
    first_vec = fetch_embedding(endpoint, model, first_text)
    dim = len(first_vec)
    print(f"[embed] detected dim={dim}")

    # Create vec0 table now that we know dim.
    create_vec_table(conn, dim)

    update_cur = conn.cursor()
    vec_cur = conn.cursor()

    def store(rid: int, vec: list[float]) -> None:
        blob = pack_vec(vec)
        update_cur.execute("UPDATE entries SET embedding = ? WHERE id = ?", (blob, rid))
        vec_cur.execute(
            "INSERT INTO entries_vec(rowid, embedding) VALUES (?, ?)",
            (rid, blob),
        )

    # Write the first one we already embedded.
    store(first[0], first_vec)
    count = 1
    for row in rows[1:]:
        rid, name, cn_desc, causes, keywords = row
        text = embedding_text(name, cn_desc, causes, keywords)
        vec = fetch_embedding(endpoint, model, text)
        if len(vec) != dim:
            raise RuntimeError(
                f"embedding dim mismatch on entry id={rid}: got {len(vec)} expected {dim}"
            )
        store(rid, vec)
        count += 1
        if count % 25 == 0:
            conn.commit()
            print(f"[embed]   {count}/{len(rows)}")

    conn.commit()
    cur.execute("UPDATE db_meta SET value = '1' WHERE key = 'has_embeddings'")
    cur.execute(
        "INSERT OR REPLACE INTO db_meta(key, value) VALUES ('embedding_dim', ?)",
        (str(dim),),
    )
    cur.execute(
        "INSERT OR REPLACE INTO db_meta(key, value) VALUES ('embedding_model', ?)",
        (model,),
    )
    conn.commit()
    return count


def crawl_microsoft_docs(conn: sqlite3.Connection) -> int:
    """Crawl Microsoft Learn for BugCheck + Win32 + NTSTATUS code references.

    Uses BeautifulSoup4 (optional dep). Inserts new (kind, code) rows; existing
    rows from fixtures are preserved (UNIQUE constraint).

    Returns number of rows newly inserted across all sources.
    """
    try:
        from bs4 import BeautifulSoup
    except ImportError:
        print(
            "[crawl] FATAL: this command needs beautifulsoup4. Run:\n"
            "  pip install -i https://pypi.tuna.tsinghua.edu.cn/simple "
            "beautifulsoup4",
            file=sys.stderr,
        )
        return 0

    total = 0
    total += _crawl_bugcheck_reference(conn, BeautifulSoup)
    total += _crawl_win32_error_codes(conn, BeautifulSoup)
    total += _crawl_ntstatus_codes(conn, BeautifulSoup)
    print(f"[crawl] total new rows: {total}")
    return total


def _fetch(url: str) -> str:
    """Polite HTTP GET with browser User-Agent + post-request delay."""
    req = urllib.request.Request(
        url,
        headers={
            "User-Agent": (
                "Mozilla/5.0 (NeuroBoot-build-error-db; +https://github.com/yzq1979/NeuroBoot)"
            )
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            html = resp.read().decode("utf-8", errors="replace")
    finally:
        time.sleep(CRAWL_DELAY_S)
    return html


def _insert_crawled(
    cur: sqlite3.Cursor, kind: str, code: str, name: str, cn_desc: str, docs_url: str
) -> bool:
    """Insert one crawled row; returns True if it was new (not a duplicate)."""
    try:
        cur.execute(
            """
            INSERT INTO entries (kind, code, name, cn_desc, causes, docs_url, keywords)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (
                kind,
                code,
                name,
                cn_desc,
                None,
                docs_url,
                # Stuff name + code into keywords so FTS5 finds them even when
                # cn_desc is still the English headline (Chinese translation TODO).
                f"{name} {code}",
            ),
        )
        return True
    except sqlite3.IntegrityError:
        return False


def _crawl_bugcheck_reference(conn: sqlite3.Connection, BS) -> int:
    """Microsoft Learn 'Bug Check Code Reference' -- ~512 codes."""
    url = (
        "https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/"
        "bug-check-code-reference2"
    )
    print(f"[crawl] BugCheck ref: {url}")
    try:
        html = _fetch(url)
    except urllib.error.URLError as e:
        print(f"[crawl]   fetch failed: {e}", file=sys.stderr)
        return 0

    soup = BS(html, "html.parser")
    # The reference page is a single big <table> with cols [Code, Symbolic name].
    table = soup.find("table")
    if table is None:
        print("[crawl]   no <table> found -- page layout may have changed")
        return 0

    cur = conn.cursor()
    inserted = 0
    for tr in table.find_all("tr"):
        cells = tr.find_all("td")
        if len(cells) < 2:
            continue
        code_cell = cells[0].get_text(strip=True)
        name_cell = cells[1].get_text(strip=True)
        # Format: '0x00000001'. Normalize -> 'NNNN' (no 0x, no leading zeros, uppercase).
        normalized = _normalize_hex(code_cell)
        if not normalized:
            continue
        # The Microsoft 'name' cell sometimes is a link with the mnemonic.
        # If the cell has an <a>, use that href for docs_url.
        a = cells[1].find("a")
        if a and a.get("href"):
            docs_url = _absolute_url(a["href"], url)
        else:
            docs_url = url
        cn_desc = f"BugCheck {code_cell} {name_cell}"  # placeholder; needs translation
        if _insert_crawled(cur, "bugcheck", normalized, name_cell, cn_desc, docs_url):
            inserted += 1
    conn.commit()
    print(f"[crawl]   bugcheck inserted: {inserted}")
    return inserted


def _crawl_win32_error_codes(conn: sqlite3.Connection, BS) -> int:
    """Walk the Win32 'System Error Codes' sub-pages.

    Microsoft splits them into ranges: 0-499, 500-999, 1000-1299, 1300-1699,
    1700-3999, 4000-5999, 6000-8199, 8200-8999, 9000-11999, 12000-15999,
    15999-* (overflow). The index page links to all of them.
    """
    index_url = "https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes"
    print(f"[crawl] Win32 index: {index_url}")
    try:
        html = _fetch(index_url)
    except urllib.error.URLError as e:
        print(f"[crawl]   fetch failed: {e}", file=sys.stderr)
        return 0

    soup = BS(html, "html.parser")
    # The index page has <a href="system-error-codes--0-499-"> style links.
    range_links: list[str] = []
    for a in soup.find_all("a"):
        href = a.get("href", "")
        if "system-error-codes--" in href and href not in range_links:
            range_links.append(_absolute_url(href, index_url))

    cur = conn.cursor()
    total = 0
    for page_url in range_links:
        print(f"[crawl]   range: {page_url}")
        try:
            page_html = _fetch(page_url)
        except urllib.error.URLError as e:
            print(f"[crawl]     skip ({e})")
            continue
        page_soup = BS(page_html, "html.parser")
        # Each error is a <h3> / <h4> heading containing the code + mnemonic,
        # followed by a <p> describing it. Robust to layout drift: any heading
        # whose first text matches /^\d+ \(/ or /^\d+ \(0x[0-9A-F]+\)/.
        count = 0
        for header in page_soup.find_all(["h3", "h4"]):
            text = header.get_text(" ", strip=True)
            parsed = _parse_win32_header(text)
            if parsed is None:
                continue
            code_decimal, mnemonic = parsed
            normalized = _normalize_hex(f"0x{int(code_decimal):X}")
            # Body text: the next <p> sibling.
            next_p = header.find_next_sibling()
            while next_p is not None and next_p.name not in ("p", "h2", "h3", "h4"):
                next_p = next_p.find_next_sibling()
            if next_p is not None and next_p.name == "p":
                cn_desc_en = next_p.get_text(" ", strip=True)
            else:
                cn_desc_en = mnemonic
            if _insert_crawled(cur, "win32", normalized, mnemonic, cn_desc_en, page_url):
                count += 1
        conn.commit()
        print(f"[crawl]     inserted: {count}")
        total += count
    return total


def _crawl_ntstatus_codes(conn: sqlite3.Connection, BS) -> int:
    """NTSTATUS code reference -- single large page with collapsible <dl>."""
    url = (
        "https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/"
        "596a1078-e883-4972-9bbc-49e60bebca55"
    )
    print(f"[crawl] NTSTATUS: {url}")
    try:
        html = _fetch(url)
    except urllib.error.URLError as e:
        print(f"[crawl]   fetch failed: {e}", file=sys.stderr)
        return 0

    soup = BS(html, "html.parser")
    cur = conn.cursor()
    inserted = 0
    # NTSTATUS page lays out each entry as <dt>STATUS_FOO</dt><dd>...</dd>
    # with the hex code mentioned earlier in a <h4> or similar.
    # Conservative selector: any <dt> whose text starts with STATUS_.
    for dt in soup.find_all("dt"):
        mnemonic = dt.get_text(" ", strip=True)
        if not mnemonic.startswith("STATUS_"):
            continue
        # The hex code is typically in the previous <h4> sibling.
        prev = dt.find_previous(["h4", "h3"])
        hex_str = None
        if prev is not None:
            txt = prev.get_text(" ", strip=True)
            normalized = _normalize_hex(txt)
            if normalized:
                hex_str = normalized
        if hex_str is None:
            continue
        dd = dt.find_next_sibling("dd")
        cn_desc_en = dd.get_text(" ", strip=True) if dd else mnemonic
        if _insert_crawled(cur, "ntstatus", hex_str, mnemonic, cn_desc_en, url):
            inserted += 1
    conn.commit()
    print(f"[crawl]   ntstatus inserted: {inserted}")
    return inserted


def _normalize_hex(s: str) -> str:
    """Extract uppercase hex token, drop leading zeros (keep at least '0')."""
    s = s.upper()
    if "0X" in s:
        s = s.split("0X", 1)[1]
    out = []
    for ch in s:
        if ch in "0123456789ABCDEF":
            out.append(ch)
        elif out:
            break
    hex_str = "".join(out)
    if not hex_str:
        return ""
    trimmed = hex_str.lstrip("0")
    return trimmed if trimmed else "0"


def _parse_win32_header(text: str) -> tuple[str, str] | None:
    """Parse 'NNN (0xXX) MNEMONIC' or 'NNN (0xXX)\\nMNEMONIC' headers."""
    import re
    m = re.match(r"^(\d+)\s*\(0x([0-9a-fA-F]+)\)\s*(.*)$", text)
    if not m:
        return None
    decimal, hex_part, rest = m.groups()
    # Mnemonic might be on a follow-up line or empty in some cases.
    mnemonic = rest.strip() or f"ERROR_{decimal}"
    return decimal, mnemonic


def _absolute_url(href: str, base: str) -> str:
    """Resolve relative hrefs against the page URL."""
    if href.startswith("http://") or href.startswith("https://"):
        return href
    if href.startswith("/"):
        return "https://learn.microsoft.com" + href
    # Same-directory link.
    base_dir = base.rsplit("/", 1)[0]
    return f"{base_dir}/{href}"


def main() -> int:
    p = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    p.add_argument("-o", "--output", default="errordb.sqlite", help="output sqlite path")
    p.add_argument("--fixtures-only", action="store_true", help="only load fixtures")
    p.add_argument("--crawl", action="store_true", help="crawl Microsoft docs")
    p.add_argument(
        "--with-fixtures",
        action="store_true",
        help="when used with --crawl, also include fixture entries",
    )
    p.add_argument("--no-embed", action="store_true", help="skip embedding generation")
    p.add_argument(
        "--embedding-url",
        default="http://127.0.0.1:8080",
        help="OpenAI-compatible /v1/embeddings host",
    )
    p.add_argument("--embedding-model", default="qwen3-embedding-0.6b")
    p.add_argument(
        "--fixtures",
        default=str(DEFAULT_FIXTURES_PATH),
        help="path to fixture JSON",
    )
    args = p.parse_args()

    if not args.fixtures_only and not args.crawl:
        p.error("must pass --fixtures-only or --crawl (or both)")

    out_path = Path(args.output).resolve()
    fixtures_path = Path(args.fixtures).resolve()

    print(f"[init] creating fresh db at {out_path}")
    conn = init_db(out_path)

    inserted_fixtures = 0
    inserted_crawl = 0
    embedded = 0

    if args.fixtures_only or args.with_fixtures:
        if not fixtures_path.is_file():
            p.error(f"fixtures file not found: {fixtures_path}")
        inserted_fixtures = insert_fixtures(conn, fixtures_path)
        print(f"[fixtures] inserted {inserted_fixtures} from {fixtures_path.name}")

    if args.crawl:
        inserted_crawl = crawl_microsoft_docs(conn)

    if not args.no_embed:
        try:
            embedded = populate_embeddings(conn, args.embedding_url, args.embedding_model)
        except RuntimeError as e:
            print(f"[embed] FAILED: {e}", file=sys.stderr)
            print("[embed] db remains FTS5-only", file=sys.stderr)

    conn.close()

    final_size_kb = out_path.stat().st_size / 1024
    print(
        f"[done] {out_path.name} = {final_size_kb:.1f} KB "
        f"(fixtures: {inserted_fixtures}, crawl: {inserted_crawl}, embedded: {embedded})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
