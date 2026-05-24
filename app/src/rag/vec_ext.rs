//! sqlite-vec extension registration.
//!
//! Calls `rusqlite::ffi::sqlite3_auto_extension` exactly once per process so
//! every `Connection::open` afterwards has the vec0 virtual table, `vec_version()`,
//! `vec_distance()`, etc. registered automatically.
//!
//! Mirrors the pattern from sqlite-vec's official Rust binding tests:
//! <https://github.com/asg017/sqlite-vec/blob/main/bindings/rust/src/lib.rs>
//!
//! The `OnceLock` guard makes this safe to call from anywhere -- the FFI
//! function only fires once. Subsequent calls are no-ops (the OnceLock
//! returns the cached `Ok(())` / `Err(...)`).

use std::sync::OnceLock;

use rusqlite::ffi::sqlite3_auto_extension;
use sqlite_vec::sqlite3_vec_init;

static REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();

/// Register the sqlite-vec extension with SQLite. Idempotent.
///
/// Returns Ok the first time it's actually registered, or the cached prior
/// result on subsequent calls. Errors are extremely unlikely (SQLite returns
/// non-OK only on OOM), but we surface them rather than panic so RAG fails
/// gracefully to FTS5-only.
pub fn register_once() -> Result<(), String> {
    REGISTERED
        .get_or_init(|| {
            // SAFETY: We're forwarding the C function pointer through SQLite's
            // C-API as the upstream binding does. The transmute reinterprets
            // sqlite3_vec_init's `extern "C" fn()` as the auto-extension
            // callback type that takes (db*, errmsg**, api*) and returns int.
            // sqlite-vec is implemented to be ABI-compatible (declares
            // SQLITE_CORE which adapts its entry to that signature).
            let rc = unsafe {
                sqlite3_auto_extension(Some(std::mem::transmute(
                    sqlite3_vec_init as *const (),
                )))
            };
            if rc == 0 {
                Ok(())
            } else {
                Err(format!("sqlite3_auto_extension returned {rc}"))
            }
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn register_is_idempotent() {
        assert!(register_once().is_ok());
        assert!(register_once().is_ok());
        assert!(register_once().is_ok());
    }

    #[test]
    fn vec_version_available_after_register() {
        register_once().expect("register");
        let conn = Connection::open_in_memory().expect("in-memory db");
        let v: String = conn
            .query_row("SELECT vec_version()", [], |r| r.get(0))
            .expect("vec_version should work after registering extension");
        assert!(
            v.starts_with("v"),
            "vec_version() should return a v-prefixed string, got: {v}"
        );
    }

    #[test]
    fn vec0_virtual_table_creatable() {
        register_once().expect("register");
        let conn = Connection::open_in_memory().expect("in-memory db");
        // 8-dim vector table for testing -- production uses 1024 for Qwen3-Embedding-0.6B.
        conn.execute(
            "CREATE VIRTUAL TABLE v USING vec0(embedding float[8])",
            [],
        )
        .expect("vec0 virtual table should create");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM v", [], |r| r.get(0))
            .expect("count empty vec0 table");
        assert_eq!(count, 0);
    }

    #[test]
    fn vec_distance_orders_correctly() {
        register_once().expect("register");
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute("CREATE VIRTUAL TABLE v USING vec0(embedding float[4])", [])
            .unwrap();
        // 3 unit-ish vectors; query [1,0,0,0] should rank row 1 (identical) first.
        let pack = |v: &[f32]| -> Vec<u8> {
            v.iter().flat_map(|x| x.to_le_bytes()).collect()
        };
        let r1 = pack(&[1.0, 0.0, 0.0, 0.0]);
        let r2 = pack(&[0.0, 1.0, 0.0, 0.0]);
        let r3 = pack(&[0.7, 0.7, 0.0, 0.0]);
        for (i, blob) in [&r1, &r2, &r3].iter().enumerate() {
            conn.execute(
                "INSERT INTO v(rowid, embedding) VALUES (?1, ?2)",
                rusqlite::params![(i + 1) as i64, blob.as_slice()],
            )
            .unwrap();
        }
        let q = pack(&[1.0, 0.0, 0.0, 0.0]);

        // KNN query: vec0 tables expose a `MATCH` syntax with `k` constraint.
        // Returns the closest rows ordered by distance.
        let mut stmt = conn
            .prepare(
                "SELECT rowid, distance FROM v
                 WHERE embedding MATCH ?1 AND k = 3
                 ORDER BY distance",
            )
            .unwrap();
        let rows: Vec<(i64, f64)> = stmt
            .query_map(rusqlite::params![q], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert_eq!(rows.len(), 3, "should return all 3 rows");
        assert_eq!(rows[0].0, 1, "identical vector should be closest");
        // Row 3 (0.7, 0.7, ...) is closer to q than row 2 (0, 1, ...).
        assert_eq!(rows[1].0, 3);
        assert_eq!(rows[2].0, 2);
    }
}
