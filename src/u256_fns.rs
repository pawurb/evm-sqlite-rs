//! Custom SQLite functions for exact uint256 arithmetic over big-endian BLOBs.
//!
//! `amount` columns store a uint256 as a 32-byte big-endian BLOB. Byte order
//! equals numeric order, so `MAX`/`MIN`/`COUNT`/`GROUP BY`/threshold comparisons
//! work natively, but SQLite has no 256-bit arithmetic — a plain `SUM` over the
//! BLOB coerces to a float and returns garbage. `u256_sum` folds the blobs with
//! real 256-bit math and returns a 32-byte big-endian BLOB.

use ruint::aliases::U256;
use rusqlite::functions::{Aggregate, Context, FunctionFlags};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, Result};

/// Aggregate: sum 32-byte big-endian uint256 BLOBs into a 32-byte BE BLOB.
///
/// Skips `NULL` and any blob that is not exactly 32 bytes (e.g. non-transfer
/// logs) rather than erroring. Returns SQL `NULL` when no rows matched.
struct U256Sum;

impl Aggregate<U256, Option<Vec<u8>>> for U256Sum {
    fn init(&self, _: &mut Context<'_>) -> Result<U256> {
        Ok(U256::ZERO)
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut U256) -> Result<()> {
        // `from_be_slice` panics on >32 bytes, so the length guard is required,
        // not just a correctness filter.
        if let ValueRef::Blob(b) = ctx.get_raw(0)
            && b.len() == 32
        {
            *acc = acc.wrapping_add(U256::from_be_slice(b));
        }
        Ok(())
    }

    fn finalize(&self, _: &mut Context<'_>, acc: Option<U256>) -> Result<Option<Vec<u8>>> {
        // `None` => `step` never ran => empty set => SQL NULL (not a zero blob).
        Ok(acc.map(|s| s.to_be_bytes::<32>().to_vec()))
    }
}

/// Register all u256 SQLite functions on a connection. Functions are
/// per-connection, so call this on every connection that needs them.
pub fn register_u256_fns(conn: &Connection) -> Result<()> {
    conn.create_aggregate_function(
        "u256_sum",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        U256Sum,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 32-byte big-endian encoding of a `u128`, for building test rows.
    fn be(n: u128) -> Vec<u8> {
        U256::from(n).to_be_bytes::<32>().to_vec()
    }

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register_u256_fns(&conn).unwrap();
        conn.execute("CREATE TABLE t (x BLOB)", []).unwrap();
        conn
    }

    fn sum(conn: &Connection) -> Option<Vec<u8>> {
        conn.query_row("SELECT u256_sum(x) FROM t", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn basic_sum() {
        let conn = setup();
        for n in [1u128, 2, 3] {
            conn.execute("INSERT INTO t VALUES (?1)", [be(n)]).unwrap();
        }
        assert_eq!(sum(&conn), Some(be(6)));
    }

    #[test]
    fn null_skipped() {
        let conn = setup();
        conn.execute("INSERT INTO t VALUES (?1)", [be(10)]).unwrap();
        conn.execute("INSERT INTO t VALUES (NULL)", []).unwrap();
        conn.execute("INSERT INTO t VALUES (?1)", [be(5)]).unwrap();
        assert_eq!(sum(&conn), Some(be(15)));
    }

    #[test]
    fn wrong_size_blob_skipped() {
        let conn = setup();
        conn.execute("INSERT INTO t VALUES (?1)", [be(7)]).unwrap();
        // 16-byte blob: ignored, must not error or panic.
        conn.execute("INSERT INTO t VALUES (?1)", [vec![0u8; 16]])
            .unwrap();
        assert_eq!(sum(&conn), Some(be(7)));
    }

    #[test]
    fn empty_set_is_null() {
        let conn = setup();
        let v: Option<Vec<u8>> = conn
            .query_row("SELECT u256_sum(x) FROM t WHERE 1 = 0", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, None);
    }

    #[test]
    fn wraps_at_u256_max() {
        let conn = setup();
        let max = U256::MAX.to_be_bytes::<32>().to_vec();
        conn.execute("INSERT INTO t VALUES (?1)", [max]).unwrap();
        conn.execute("INSERT INTO t VALUES (?1)", [be(3)]).unwrap();
        // MAX + 3 wraps to 2.
        assert_eq!(sum(&conn), Some(be(2)));
    }
}
