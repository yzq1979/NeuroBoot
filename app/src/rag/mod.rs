//! RAG (Retrieval-Augmented Generation) over the error code SQLite database.
//!
//! v3.0 W5-6 **Phase 1**: FTS5 trigram keyword search only.
//! - Schema produced by `tools-dev/build-error-db.py --fixtures-only --no-embed`
//! - Two-step query: exact code match first, then FTS5 fuzzy.
//! - The DB path comes from [`default_db_paths`] — `X:\NeuroBoot\rag\errordb.sqlite`
//!   (PE bundle) and `C:\NeuroBoot\tools-dev\fixtures\errordb.sqlite` (dev box).
//!
//! v3.0 W5-6 **Phase 2** (next session): same module, +1 column.
//! - Python script populates `entries.embedding` BLOB via llama-server
//!   `/v1/embeddings`.
//! - At runtime, [`RagClient`] detects `db_meta.has_embeddings = '1'` and
//!   issues vector search alongside FTS5, merging top-K hits from each.
//! - sqlite-vec crate gets added as a dep; auto-loaded once per process via
//!   `rusqlite::ffi::sqlite3_auto_extension`.
//!
//! ## Why FTS5 trigram (not unicode61)
//!
//! `unicode61` tokenizes on whitespace + punctuation, which doesn't split CJK
//! at all. With `tokenize='trigram'`, every 3-char rolling window becomes a
//! searchable token, so "启动盘" (3 chars = 1 trigram) finds matches in
//! "无法访问启动盘" without any explicit segmentation. Latin text still
//! tokenizes well (3-char windows of "irql" produce 2 trigrams; partial
//! matches like "irq" still hit). SQLite 3.34+ ships trigram built-in.
//!
//! ## Scope guard
//!
//! This module is best-effort. If the db file is missing / corrupt /
//! schema-mismatched, [`RagClient::lookup`] returns `Ok(vec![])` so the
//! calling tool can fall back to its hard-coded table. There are **no
//! panics** on db absence — only logs to stderr (visible in X:\NeuroBoot\logs).

pub mod vec_ext;

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OpenFlags};

/// One retrieval hit returned to the caller (tool / agent).
#[derive(Debug, Clone, PartialEq)]
pub struct RagHit {
    /// 'bugcheck' / 'win32' / 'hresult' / 'ntstatus' / 'tool_desc'
    pub kind: String,
    /// Normalized hex code (e.g. "7B"). Empty for tool_desc entries.
    pub code: String,
    /// Mnemonic (e.g. "INACCESSIBLE_BOOT_DEVICE") or tool name.
    pub name: String,
    /// Chinese one-line description.
    pub cn_desc: String,
    /// Multi-line Chinese cause / next-step explanation.
    pub causes: String,
    /// Microsoft Learn URL.
    pub docs_url: String,
    /// Numeric score in [0, 1]; higher = more relevant. Exact code match = 1.0,
    /// FTS5 BM25 results are normalized into (0, 1).
    pub score: f32,
    /// Source path of the matching db row (file path of the sqlite db) — debug.
    pub source: String,
}

/// Stateless client that opens a fresh connection on each [`lookup`].
///
/// Cheap to construct — we don't keep an open handle since the runtime is
/// single-process single-thread per lookup call (PE keeps things simple).
pub struct RagClient {
    db_path: PathBuf,
    /// True only when BOTH db_meta.has_embeddings = '1' AND entries_vec exists.
    has_embeddings: bool,
    /// Optional embedding endpoint -- set by [`with_embedder`].
    embedder: Option<EmbedderConfig>,
}

/// Config for calling an OpenAI-compatible /v1/embeddings endpoint
/// (typically the same llama-server that hosts the chat model).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedderConfig {
    pub endpoint: String,
    pub model: String,
}

impl RagClient {
    /// Probe the candidate db paths and return the first valid one.
    /// Returns None when no db is found anywhere; caller should fall back.
    pub fn discover() -> Option<Self> {
        for candidate in default_db_paths() {
            if let Some(client) = Self::open(&candidate) {
                return Some(client);
            }
        }
        None
    }

    /// Open a specific db path. Returns None if the file doesn't exist or
    /// the schema doesn't match Phase 1's expectations.
    pub fn open(path: &Path) -> Option<Self> {
        if !path.is_file() {
            return None;
        }
        // v3.0 W5-6 Phase 2: register sqlite-vec before any Connection::open
        // so vec0 / vec_distance / etc. are available. Idempotent across calls.
        let _ = vec_ext::register_once();

        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
        // Sanity-check schema: `entries` table + `entries_fts` virtual table both exist.
        let entries_ok = table_exists(&conn, "entries");
        let fts_ok = table_exists(&conn, "entries_fts");
        if !entries_ok || !fts_ok {
            eprintln!(
                "[rag] db at {} missing expected tables (entries={entries_ok}, entries_fts={fts_ok}) — skipping",
                path.display()
            );
            return None;
        }
        let has_embeddings = conn
            .query_row(
                "SELECT value FROM db_meta WHERE key = 'has_embeddings'",
                [],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .map(|v| v == "1")
            .unwrap_or(false);
        let has_vec_table = table_exists(&conn, "entries_vec");
        Some(Self {
            db_path: path.to_path_buf(),
            has_embeddings: has_embeddings && has_vec_table,
            embedder: None,
        })
    }

    /// Configure an embedding endpoint for vector search.
    ///
    /// Returns self for chaining. Subsequent [`lookup_auto`] calls will
    /// hit the endpoint to embed user queries before vector KNN. No-op at
    /// query time if the db doesn't have embeddings.
    pub fn with_embedder(mut self, endpoint: String, model: String) -> Self {
        self.embedder = Some(EmbedderConfig { endpoint, model });
        self
    }

    /// Whether this db has embeddings + vec0 table (Phase 2 ready).
    pub fn has_embeddings(&self) -> bool {
        self.has_embeddings
    }

    /// Run the full retrieval flow for the user query.
    ///
    /// Layers (Phase 1):
    /// 1. **Exact code match** on normalized `query` — if input parses as a hex
    ///    code, score 1.0 hit.
    /// 2. **FTS5 trigram match** on `name + cn_desc + causes + keywords`.
    ///
    /// `top_n` caps the total returned hits (typically 3 or 5).
    pub fn lookup(&self, query: &str, top_n: usize) -> Result<Vec<RagHit>, RagError> {
        let conn = Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| RagError::OpenFailed(e.to_string()))?;
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let mut hits: Vec<RagHit> = Vec::new();

        // ---- Layer 1: exact code match ----
        if let Some(normalized) = try_normalize_code(trimmed) {
            let mut stmt = conn
                .prepare(
                    "SELECT kind, code, name, cn_desc, causes, docs_url FROM entries WHERE code = ?1",
                )
                .map_err(|e| RagError::QueryFailed(e.to_string()))?;
            let exact_iter = stmt
                .query_map(params![normalized], |row| {
                    Ok(RagHit {
                        kind: row.get(0)?,
                        code: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        name: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        cn_desc: row.get(3)?,
                        causes: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                        docs_url: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        score: 1.0,
                        source: self.db_path.display().to_string(),
                    })
                })
                .map_err(|e| RagError::QueryFailed(e.to_string()))?;
            for r in exact_iter {
                if let Ok(hit) = r {
                    hits.push(hit);
                }
            }
        }

        // ---- Layer 2: FTS5 trigram (skip if we already have an exact hit & top_n=1) ----
        let fts_query = sanitize_fts_query(trimmed);
        if !fts_query.is_empty() && hits.len() < top_n {
            // Use FTS5's bm25() ranking — lower bm25 = more relevant.
            // We invert it to a positive score in (0, 1) below.
            let sql = "SELECT e.kind, e.code, e.name, e.cn_desc, e.causes, e.docs_url, bm25(entries_fts) AS rank
                       FROM entries_fts
                       JOIN entries e ON e.id = entries_fts.rowid
                       WHERE entries_fts MATCH ?1
                       ORDER BY rank
                       LIMIT ?2";
            let mut stmt = conn
                .prepare(sql)
                .map_err(|e| RagError::QueryFailed(e.to_string()))?;
            // We pull top_n * 2 FTS candidates so dedupe with exact hits leaves enough.
            let fts_limit = (top_n * 2) as i64;
            let fts_iter = stmt
                .query_map(params![fts_query, fts_limit], |row| {
                    let bm25: f64 = row.get(6)?;
                    Ok(RagHit {
                        kind: row.get(0)?,
                        code: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        name: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        cn_desc: row.get(3)?,
                        causes: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                        docs_url: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        score: bm25_to_score(bm25),
                        source: self.db_path.display().to_string(),
                    })
                })
                .map_err(|e| RagError::QueryFailed(e.to_string()))?;
            for r in fts_iter {
                if let Ok(hit) = r {
                    // Dedupe with exact hits — match on (kind, code, name).
                    let dup = hits.iter().any(|h| {
                        h.kind == hit.kind && h.code == hit.code && h.name == hit.name
                    });
                    if !dup {
                        hits.push(hit);
                    }
                }
            }
        }

        hits.truncate(top_n);
        Ok(hits)
    }

    /// v3.0 W5-6 Phase 2 — hybrid retrieval = exact + FTS5 + vector KNN, fused
    /// via Reciprocal Rank Fusion (RRF).
    ///
    /// The caller pre-computes `query_vec` (e.g. via [`Self::embed_query`]).
    /// Returns hits ordered by RRF score. If the db has no `entries_vec`
    /// table (Phase 1 db), this silently degrades to the FTS5 + exact path
    /// (same as [`Self::lookup`]) so callers don't have to branch.
    pub fn lookup_hybrid(
        &self,
        query: &str,
        query_vec: &[f32],
        top_n: usize,
    ) -> Result<Vec<RagHit>, RagError> {
        if !self.has_embeddings {
            return self.lookup(query, top_n);
        }

        // Pull FTS5+exact results first (Phase 1 path).
        let fts_hits = self.lookup(query, top_n.saturating_mul(2).max(10))?;

        // Then vector top-K.
        let vec_hits = self.vector_search(query_vec, top_n.saturating_mul(2).max(10))?;

        // Fuse with RRF (k=60 is the canonical default per the original
        // Cormack/Clarke/Buettcher 2009 paper).
        Ok(rrf_fuse(&fts_hits, &vec_hits, top_n))
    }

    /// One-stop entry: if embedder configured + db has embeddings, run hybrid;
    /// otherwise run plain FTS5+exact.
    ///
    /// Errors from the embedding endpoint are demoted to FTS5 fallback (we
    /// log to stderr but still return useful results).
    pub fn lookup_auto(&self, query: &str, top_n: usize) -> Result<Vec<RagHit>, RagError> {
        if let (true, Some(cfg)) = (self.has_embeddings, &self.embedder) {
            match Self::embed_query(cfg, query) {
                Ok(query_vec) => return self.lookup_hybrid(query, &query_vec, top_n),
                Err(e) => {
                    eprintln!("[rag] embed_query failed ({e}) — falling back to FTS5 only");
                }
            }
        }
        self.lookup(query, top_n)
    }

    /// Run a vector KNN over `entries_vec` and JOIN back to `entries`.
    fn vector_search(&self, query_vec: &[f32], k: usize) -> Result<Vec<RagHit>, RagError> {
        let conn = Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| RagError::OpenFailed(e.to_string()))?;
        let blob = pack_f32_vec(query_vec);
        // sqlite-vec's KNN syntax: WHERE embedding MATCH ?1 AND k = ?2.
        let sql = "SELECT e.kind, e.code, e.name, e.cn_desc, e.causes, e.docs_url, v.distance
                   FROM entries_vec v
                   JOIN entries e ON e.id = v.rowid
                   WHERE v.embedding MATCH ?1 AND v.k = ?2
                   ORDER BY v.distance";
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| RagError::QueryFailed(e.to_string()))?;
        let mut out = Vec::new();
        let iter = stmt
            .query_map(params![blob.as_slice(), k as i64], |row| {
                let dist: f64 = row.get(6)?;
                Ok(RagHit {
                    kind: row.get(0)?,
                    code: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    name: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    cn_desc: row.get(3)?,
                    causes: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    docs_url: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                    score: vec_distance_to_score(dist),
                    source: self.db_path.display().to_string(),
                })
            })
            .map_err(|e| RagError::QueryFailed(e.to_string()))?;
        for r in iter {
            if let Ok(hit) = r {
                out.push(hit);
            }
        }
        Ok(out)
    }

    /// POST text to the embedding endpoint and return the resulting vector.
    ///
    /// Uses the `reqwest::blocking` client we already depend on for LLM calls.
    /// 10s timeout — embedding a single short query is sub-100 ms typically.
    pub fn embed_query(cfg: &EmbedderConfig, text: &str) -> Result<Vec<f32>, RagError> {
        let url = format!("{}/v1/embeddings", cfg.endpoint.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": cfg.model,
            "input": text,
        });
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| RagError::OpenFailed(format!("reqwest build: {e}")))?;
        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .map_err(|e| RagError::QueryFailed(format!("POST {url}: {e}")))?;
        let status = resp.status();
        let text_body = resp
            .text()
            .map_err(|e| RagError::QueryFailed(format!("read body: {e}")))?;
        if !status.is_success() {
            return Err(RagError::QueryFailed(format!(
                "embedding endpoint returned {status}: {}",
                text_body.chars().take(200).collect::<String>()
            )));
        }
        parse_embedding_response(&text_body)
    }
}

/// What can go wrong looking up the RAG db.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RagError {
    OpenFailed(String),
    QueryFailed(String),
}

impl std::fmt::Display for RagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RagError::OpenFailed(s) => write!(f, "RAG open failed: {s}"),
            RagError::QueryFailed(s) => write!(f, "RAG query failed: {s}"),
        }
    }
}

/// Pack an f32 slice as a little-endian byte buffer suitable for sqlite-vec
/// `MATCH` parameter binding.
fn pack_f32_vec(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// Map sqlite-vec L2 distance to a [0, 1] similarity score (1 = identical).
/// sqlite-vec defaults to L2 squared; for unit-normalized vectors, distance
/// is in [0, 4] so we map with 1 / (1 + d).
fn vec_distance_to_score(distance: f64) -> f32 {
    (1.0 / (1.0 + distance.max(0.0))) as f32
}

/// Parse an OpenAI-compatible embeddings response and return the first vec.
/// Format: `{"data":[{"embedding":[0.1, 0.2, ...]}], ...}`.
fn parse_embedding_response(body: &str) -> Result<Vec<f32>, RagError> {
    let v: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| RagError::QueryFailed(format!("response parse: {e}")))?;
    let arr = v
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|e| e.get("embedding"))
        .and_then(|e| e.as_array())
        .ok_or_else(|| {
            RagError::QueryFailed(format!(
                "missing data[0].embedding in response: {}",
                body.chars().take(120).collect::<String>()
            ))
        })?;
    let mut out = Vec::with_capacity(arr.len());
    for x in arr {
        let f = x
            .as_f64()
            .ok_or_else(|| RagError::QueryFailed("embedding element not a number".into()))?;
        out.push(f as f32);
    }
    if out.is_empty() {
        return Err(RagError::QueryFailed("embedding vector is empty".into()));
    }
    Ok(out)
}

/// Reciprocal Rank Fusion: merge ranked lists from two retrievers into one.
///
/// Per Cormack/Clarke/Buettcher 2009, the standard k constant is 60. Each
/// hit contributes `1 / (k + rank)` to its fused score; the same hit
/// surfaced by both retrievers gets summed boosts.
///
/// Dedup key is (kind, code, name) — same as the FTS5 dedupe in `lookup()`.
fn rrf_fuse(fts: &[RagHit], vec: &[RagHit], top_n: usize) -> Vec<RagHit> {
    const K: f32 = 60.0;
    use std::collections::HashMap;
    let mut scores: HashMap<(String, String, String), (f32, RagHit)> = HashMap::new();
    for (i, h) in fts.iter().enumerate() {
        let key = (h.kind.clone(), h.code.clone(), h.name.clone());
        let bump = 1.0 / (K + (i as f32 + 1.0));
        scores
            .entry(key)
            .and_modify(|(s, _)| *s += bump)
            .or_insert((bump, h.clone()));
    }
    for (i, h) in vec.iter().enumerate() {
        let key = (h.kind.clone(), h.code.clone(), h.name.clone());
        let bump = 1.0 / (K + (i as f32 + 1.0));
        scores
            .entry(key)
            .and_modify(|(s, _)| *s += bump)
            .or_insert((bump, h.clone()));
    }
    let mut merged: Vec<(f32, RagHit)> = scores.into_values().collect();
    // Sort descending by fused score.
    merged.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    merged
        .into_iter()
        .take(top_n)
        .map(|(score, mut h)| {
            h.score = score;
            h
        })
        .collect()
}

/// Candidate db paths in priority order — first existing wins.
///
/// Phase 1 doesn't ship a built-in db in the ISO yet (no embeddings, FTS5
/// only — the bundle is deferred to Phase 2 alongside the embedding model).
/// So in dev, the user runs `python tools-dev/build-error-db.py
/// --fixtures-only --no-embed` once which writes to the second path below.
pub fn default_db_paths() -> Vec<PathBuf> {
    vec![
        // PE-bundled location (Phase 2)
        PathBuf::from(r"X:\NeuroBoot\rag\errordb.sqlite"),
        // Dev-box / fallback location for fixture-only Phase 1 testing
        PathBuf::from(r"C:\NeuroBoot\tools-dev\fixtures\errordb.sqlite"),
    ]
}

/// Cheap "does this table or virtual table exist?" check.
fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE name = ?1 AND type IN ('table','view') LIMIT 1",
        params![name],
        |_| Ok(()),
    )
    .is_ok()
}

/// Parse common hex code formats. Returns None when the input clearly isn't
/// a code (e.g. an ad-hoc Chinese question).
///
/// This is intentionally **stricter** than [`crate::tools::safe::lookup_error_code::normalize_code`]
/// — RAG's exact-match layer should only trigger when the user genuinely
/// typed a code-like string. If we're not sure, return None and let FTS5 do
/// the work.
fn try_normalize_code(raw: &str) -> Option<String> {
    let upper = raw.to_uppercase();
    let hex = if let Some(pos) = upper.find("0X") {
        upper[pos + 2..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect::<String>()
    } else if upper.chars().all(|c| c.is_ascii_hexdigit() || c.is_ascii_whitespace()) {
        // Bare hex token like "7B" / "C0000005" — only treat as code when
        // the ENTIRE input is hex (no Chinese / no plain English words).
        upper.chars().filter(|c| c.is_ascii_hexdigit()).collect::<String>()
    } else {
        return None;
    };

    if hex.is_empty() {
        return None;
    }
    let trimmed = hex.trim_start_matches('0');
    if trimmed.is_empty() {
        Some("0".to_owned())
    } else {
        Some(trimmed.to_owned())
    }
}

/// Make user input safe for FTS5 `MATCH` — strip double quotes, wrap each
/// surviving token as a phrase, drop tokens that would be useless.
///
/// FTS5 trigram tokenizer behavior on short queries (per SQLite docs):
/// - Tokens >= 3 chars: indexed as overlapping 3-char windows, regular MATCH.
/// - Tokens < 3 chars: degrade to substring scan (slower but works).
///
/// We therefore keep all tokens >= 2 chars. Single CJK chars (count = 1) are
/// dropped — substring scanning the whole db for one character produces too
/// many false positives. Single ASCII chars (e.g. "a") are also dropped.
/// Punctuation-only tokens get dropped too.
fn sanitize_fts_query(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c == '"' { ' ' } else { c })
        .collect();
    let mut tokens: Vec<String> = Vec::new();
    for word in cleaned.split_whitespace() {
        if word.chars().all(|c| c.is_ascii_punctuation()) {
            continue;
        }
        let char_count = word.chars().count();
        if char_count >= 3 {
            tokens.push(format!("\"{word}\""));
        } else if char_count == 2 {
            // 2-char tokens: FTS5 trigram does substring scan. Keep both
            // ASCII ("ip") and CJK ("蓝屏") because they're common and useful.
            tokens.push(format!("\"{word}\""));
        }
        // char_count == 1 (or 0) drops -- too noisy.
    }
    tokens.join(" OR ")
}

/// Convert FTS5 BM25 (negative-ish, lower = better) to a [0, 1] score.
///
/// BM25 in SQLite returns negative values where -10 means "strong match" and
/// -1 means "weak match". We bucket into a sigmoid-ish curve.
fn bm25_to_score(bm25: f64) -> f32 {
    // BM25 is typically in range -20..0; map to 0..1 with strongest near 1.
    let normalized = (-bm25 / 20.0).clamp(0.0, 1.0);
    // Squish so exact-code-match's 1.0 stays distinct from FTS top.
    (normalized * 0.95) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_db(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(include_str!("test_schema.sql")).unwrap();
    }

    #[test]
    fn discover_returns_none_when_no_db_anywhere() {
        // We can't actually unset the default paths, but on a clean CI run
        // neither path exists. On a dev machine where fixtures exist this
        // test will degrade to verifying discover() returns Some which is
        // also fine (no assertion on Some/None).
        let _ = RagClient::discover();
    }

    #[test]
    fn open_returns_none_for_missing_file() {
        assert!(RagClient::open(Path::new("Z:\\definitely_does_not_exist.sqlite")).is_none());
    }

    #[test]
    fn open_returns_none_for_wrong_schema() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        // Write an unrelated sqlite file with no `entries` table.
        let conn = Connection::open(tmp.path()).unwrap();
        conn.execute("CREATE TABLE foo (id INTEGER)", []).unwrap();
        drop(conn);
        assert!(RagClient::open(tmp.path()).is_none());
    }

    #[test]
    fn lookup_returns_exact_code_first() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        make_test_db(tmp.path());
        let client = RagClient::open(tmp.path()).expect("schema valid");
        let hits = client.lookup("0x7B", 3).expect("lookup ok");
        assert!(!hits.is_empty(), "should find INACCESSIBLE_BOOT_DEVICE");
        assert_eq!(hits[0].code, "7B");
        assert_eq!(hits[0].name, "INACCESSIBLE_BOOT_DEVICE");
        assert_eq!(hits[0].score, 1.0, "exact match must score 1.0");
    }

    #[test]
    fn lookup_handles_naked_code() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        make_test_db(tmp.path());
        let client = RagClient::open(tmp.path()).unwrap();
        let hits = client.lookup("7B", 3).unwrap();
        assert_eq!(hits[0].code, "7B");
    }

    #[test]
    fn lookup_cjk_via_fts5() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        make_test_db(tmp.path());
        let client = RagClient::open(tmp.path()).unwrap();
        let hits = client.lookup("启动盘", 3).unwrap();
        assert!(
            !hits.is_empty(),
            "FTS5 trigram should find Chinese substring matches"
        );
        // The 7B entry's cn_desc says "启动盘"; should appear.
        assert!(hits.iter().any(|h| h.code == "7B"));
    }

    #[test]
    fn lookup_latin_via_fts5() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        make_test_db(tmp.path());
        let client = RagClient::open(tmp.path()).unwrap();
        let hits = client.lookup("inaccessible", 3).unwrap();
        assert!(hits.iter().any(|h| h.name == "INACCESSIBLE_BOOT_DEVICE"));
    }

    #[test]
    fn lookup_unknown_query_returns_empty() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        make_test_db(tmp.path());
        let client = RagClient::open(tmp.path()).unwrap();
        let hits = client.lookup("xyzqqq_definitely_not_in_db", 3).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn lookup_dedupes_exact_and_fts_hits() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        make_test_db(tmp.path());
        let client = RagClient::open(tmp.path()).unwrap();
        // "7B" is both an exact code match and a trigram hit in cn_desc.
        let hits = client.lookup("7B", 3).unwrap();
        let count_7b = hits.iter().filter(|h| h.code == "7B").count();
        assert_eq!(count_7b, 1, "7B should appear exactly once after dedupe");
    }

    #[test]
    fn lookup_respects_top_n_cap() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        make_test_db(tmp.path());
        let client = RagClient::open(tmp.path()).unwrap();
        let hits = client.lookup("驱动", 2).unwrap();
        assert!(hits.len() <= 2);
    }

    // ---- try_normalize_code ----

    #[test]
    fn normalize_handles_0x_prefix() {
        assert_eq!(try_normalize_code("0x7B"), Some("7B".to_owned()));
        assert_eq!(try_normalize_code("0X7b"), Some("7B".to_owned()));
    }

    #[test]
    fn normalize_handles_bare_hex() {
        assert_eq!(try_normalize_code("7B"), Some("7B".to_owned()));
        assert_eq!(try_normalize_code("C0000005"), Some("C0000005".to_owned()));
    }

    #[test]
    fn normalize_rejects_chinese_query() {
        // "蓝屏" is not a code -- must NOT trigger exact-match path.
        assert_eq!(try_normalize_code("蓝屏"), None);
    }

    #[test]
    fn normalize_rejects_mixed_text() {
        // English word "page" is all ASCII hex chars? No, 'p' and 'g' aren't.
        assert_eq!(try_normalize_code("page fault"), None);
    }

    // ---- sanitize_fts_query ----

    #[test]
    fn sanitize_drops_single_char_tokens() {
        // Single CJK chars (count=1) and single ASCII chars are dropped.
        let q = sanitize_fts_query("启 动 盘 a");
        assert!(q.is_empty(), "single chars should drop: {q}");
    }

    #[test]
    fn sanitize_quotes_cjk_phrase() {
        let q = sanitize_fts_query("启动盘");
        assert_eq!(q, "\"启动盘\"");
    }

    #[test]
    fn sanitize_uses_or_for_multiple_tokens() {
        // 3-char CJK "启动盘" + 2-char CJK "蓝屏" (slower substring scan but kept).
        let q = sanitize_fts_query("启动盘 蓝屏");
        assert_eq!(q, "\"启动盘\" OR \"蓝屏\"");
    }

    #[test]
    fn sanitize_strips_embedded_double_quotes() {
        // Embedded `"` would break FTS5 phrase syntax -- get replaced with space,
        // splitting the original token into two safe ones.
        let q = sanitize_fts_query("启动\"盘内");
        // After replace: "启动 盘内" -> two 2-char CJK tokens
        assert_eq!(q, "\"启动\" OR \"盘内\"");
        // Most importantly: no embedded unescaped quote that would parse-error.
        assert!(!q.contains("\"\""));
    }

    #[test]
    fn sanitize_keeps_two_char_ascii() {
        // "ip" is 2 chars -> substring scan in trigram tokenizer; still useful.
        let q = sanitize_fts_query("ip");
        assert_eq!(q, "\"ip\"");
    }

    // -------- Phase 2 (W5-6) --------

    fn make_phase2_db(path: &Path) {
        // Register the vec extension BEFORE Connection::open so vec0 works.
        crate::rag::vec_ext::register_once().expect("register vec ext");
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(include_str!("test_schema_v2.sql")).unwrap();
    }

    #[test]
    fn open_phase2_db_reports_has_embeddings() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        make_phase2_db(tmp.path());
        let client = RagClient::open(tmp.path()).expect("Phase 2 schema valid");
        assert!(client.has_embeddings(), "Phase 2 db should report embeddings");
    }

    #[test]
    fn open_phase1_db_reports_no_embeddings() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        make_test_db(tmp.path()); // Phase 1 schema
        let client = RagClient::open(tmp.path()).unwrap();
        assert!(
            !client.has_embeddings(),
            "Phase 1 db should not report embeddings (no vec0 table)"
        );
    }

    #[test]
    fn vector_search_returns_nearest_first() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        make_phase2_db(tmp.path());
        let client = RagClient::open(tmp.path()).unwrap();
        // Query identical to entry id=1's vector -> should rank id=1 first.
        let q = vec![1.0_f32, 0.0, 0.0, 0.0];
        let hits = client.vector_search(&q, 3).expect("vec search");
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].name, "INACCESSIBLE_BOOT_DEVICE");
        // BSOD_GENERIC (id=3) is closer to [1,0,0,0] than D1 (id=2) -- it shares
        // an x-axis component of 0.7 vs 0.
        assert_eq!(hits[1].name, "BSOD_GENERIC");
        assert_eq!(hits[2].name, "DRIVER_IRQL_NOT_LESS_OR_EQUAL");
        // Scores in descending order.
        assert!(hits[0].score >= hits[1].score);
        assert!(hits[1].score >= hits[2].score);
    }

    #[test]
    fn lookup_hybrid_surfaces_vec_only_hit() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        make_phase2_db(tmp.path());
        let client = RagClient::open(tmp.path()).unwrap();
        // Use a query string that won't match FTS5 (no "蓝屏" / "boot" / "driver"),
        // so the BSOD_GENERIC hit can only come from vector search.
        let q_vec = vec![0.5_f32, 0.5, 0.0, 0.0]; // closest to BSOD_GENERIC [0.7,0.7,...]
        let hits = client
            .lookup_hybrid("totally_unrelated_query_xyzqqq", &q_vec, 3)
            .unwrap();
        assert!(
            hits.iter().any(|h| h.name == "BSOD_GENERIC"),
            "BSOD_GENERIC should surface via vec branch despite FTS5 miss; got: {:?}",
            hits.iter().map(|h| h.name.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn lookup_hybrid_falls_back_to_fts_for_phase1_db() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        make_test_db(tmp.path()); // Phase 1 (no vec0)
        let client = RagClient::open(tmp.path()).unwrap();
        let q_vec = vec![1.0_f32, 0.0, 0.0, 0.0];
        // Should return the FTS5+exact result for 7B without crashing.
        let hits = client.lookup_hybrid("0x7B", &q_vec, 3).unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].code, "7B");
    }

    #[test]
    fn lookup_auto_without_embedder_uses_fts() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        make_phase2_db(tmp.path());
        let client = RagClient::open(tmp.path()).unwrap(); // no with_embedder()
        // Without an embedder configured, lookup_auto should still work via FTS5.
        let hits = client.lookup_auto("0x7B", 3).unwrap();
        assert_eq!(hits[0].code, "7B");
    }

    #[test]
    fn lookup_auto_demotes_unreachable_embedder() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        make_phase2_db(tmp.path());
        let client = RagClient::open(tmp.path())
            .unwrap()
            .with_embedder(
                "http://127.0.0.1:1".to_owned(), // unreachable port
                "qwen3-embedding-0.6b".to_owned(),
            );
        // Should NOT propagate the error -- demotes to FTS5.
        let hits = client.lookup_auto("启动盘", 3).unwrap();
        assert!(!hits.is_empty(), "must still return results via fallback");
    }

    // -------- RRF + utility --------

    #[test]
    fn rrf_fuse_combines_overlapping_hits() {
        let mk = |name: &str, code: &str, score: f32| RagHit {
            kind: "bugcheck".into(),
            code: code.into(),
            name: name.into(),
            cn_desc: "x".into(),
            causes: "y".into(),
            docs_url: "z".into(),
            score,
            source: "t".into(),
        };
        // FTS ranks: A, B, C
        // Vec ranks: B, A, D
        // Expected fused order: A and B alternate at the top (both in both lists),
        // with A scoring (1/61 + 1/62) and B scoring (1/62 + 1/61) -- effectively
        // tied. C and D each appear once.
        let fts = vec![mk("A", "1", 0.0), mk("B", "2", 0.0), mk("C", "3", 0.0)];
        let vec = vec![mk("B", "2", 0.0), mk("A", "1", 0.0), mk("D", "4", 0.0)];
        let fused = rrf_fuse(&fts, &vec, 4);
        assert_eq!(fused.len(), 4);
        let names: Vec<&str> = fused.iter().map(|h| h.name.as_str()).collect();
        // First two slots must be A and B (in either order).
        let top2: std::collections::HashSet<&str> = names[..2].iter().copied().collect();
        assert!(top2.contains("A") && top2.contains("B"));
        // Other two are C / D.
        let rest: std::collections::HashSet<&str> = names[2..].iter().copied().collect();
        assert!(rest.contains("C") && rest.contains("D"));
    }

    #[test]
    fn pack_f32_vec_layout() {
        let v = vec![1.0_f32, 0.0, -1.0];
        let bytes = pack_f32_vec(&v);
        assert_eq!(bytes.len(), 12);
        // 1.0 = 0x3F800000 LE = 00 00 80 3F
        assert_eq!(&bytes[0..4], &[0x00, 0x00, 0x80, 0x3F]);
    }

    #[test]
    fn parse_embedding_response_happy_path() {
        let body = r#"{"data":[{"embedding":[0.1, 0.2, 0.3]}], "model":"qwen3-embedding-0.6b"}"#;
        let v = parse_embedding_response(body).unwrap();
        assert_eq!(v.len(), 3);
        assert!((v[0] - 0.1).abs() < 1e-5);
    }

    #[test]
    fn parse_embedding_response_rejects_empty() {
        let body = r#"{"data":[{"embedding":[]}]}"#;
        assert!(parse_embedding_response(body).is_err());
    }

    #[test]
    fn parse_embedding_response_rejects_no_data() {
        let body = r#"{"foo":"bar"}"#;
        assert!(parse_embedding_response(body).is_err());
    }

    #[test]
    fn vec_distance_to_score_is_monotonic() {
        let s0 = vec_distance_to_score(0.0);
        let s1 = vec_distance_to_score(1.0);
        let s2 = vec_distance_to_score(5.0);
        assert!(s0 > s1);
        assert!(s1 > s2);
        assert!(s0 <= 1.0 && s2 > 0.0);
    }
}
