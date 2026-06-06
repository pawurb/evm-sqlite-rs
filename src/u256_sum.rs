//! Custom SQLite functions for exact uint256 arithmetic over big-endian BLOBs.
//!
//! `amount` columns store a uint256 as a 32-byte big-endian BLOB. Byte order
//! equals numeric order, so `MAX`/`MIN`/`COUNT`/`GROUP BY`/threshold comparisons
//! work natively, but SQLite has no 256-bit arithmetic — a plain `SUM` over the
//! BLOB coerces to a float and returns garbage. `u256_sum` folds the blobs with
//! real 256-bit math and returns a 32-byte big-endian BLOB.

use ruint::aliases::U256;
use rusqlite::functions::{Aggregate, Context, FunctionFlags};
use rusqlite::{Connection, Error, Result};

use crate::args::arg_to_u256;

/// Aggregate: sum uint256 operands into a 32-byte big-endian BLOB.
///
/// Each operand may be a non-negative `INTEGER` or a big-endian `BLOB` of at
/// most 32 bytes. Skips `NULL`. Returns SQL `NULL` when no rows matched. Raises
/// on an invalid operand and if the running total would exceed `U256::MAX`.
struct U256Sum;

impl Aggregate<U256, Option<Vec<u8>>> for U256Sum {
    fn init(&self, _: &mut Context<'_>) -> Result<U256> {
        Ok(U256::ZERO)
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut U256) -> Result<()> {
        if let Some(value) = arg_to_u256(ctx.get_raw(0), "u256_sum")? {
            *acc = acc.checked_add(value).ok_or_else(|| {
                Error::UserFunctionError("u256_sum: overflow past U256::MAX".into())
            })?;
        }
        Ok(())
    }

    fn finalize(&self, _: &mut Context<'_>, acc: Option<U256>) -> Result<Option<Vec<u8>>> {
        // `None` => `step` never ran => empty set => SQL NULL (not a zero blob).
        Ok(acc.map(|s| s.to_be_bytes::<32>().to_vec()))
    }
}

/// Register the `u256_sum` aggregate on a connection.
pub(crate) fn register(conn: &Connection) -> Result<()> {
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
        register(&conn).unwrap();
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
    fn sums_integer_operands() {
        let conn = setup();
        // Integer column values are summed (not silently treated as zero).
        for n in [10i64, 20, 12] {
            conn.execute("INSERT INTO t VALUES (?1)", [n]).unwrap();
        }
        conn.execute("INSERT INTO t VALUES (NULL)", []).unwrap();
        assert_eq!(sum(&conn), Some(be(42)));
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
    fn short_blob_left_padded() {
        let conn = setup();
        conn.execute("INSERT INTO t VALUES (?1)", [be(7)]).unwrap();
        // 4-byte trimmed encoding of 5: left-padded and summed, not skipped.
        conn.execute("INSERT INTO t VALUES (?1)", [vec![0, 0, 0, 5]])
            .unwrap();
        assert_eq!(sum(&conn), Some(be(12)));
    }

    #[test]
    fn oversized_blob_raises() {
        let conn = setup();
        conn.execute("INSERT INTO t VALUES (?1)", [be(7)]).unwrap();
        // 33-byte blob can't be a uint256: must raise, not skip or panic.
        conn.execute("INSERT INTO t VALUES (?1)", [vec![0u8; 33]])
            .unwrap();
        let err = conn
            .query_row("SELECT u256_sum(x) FROM t", [], |r| {
                r.get::<_, Option<Vec<u8>>>(0)
            })
            .unwrap_err();
        assert!(err.to_string().contains("too large"), "{err}");
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
    fn overflow_raises() {
        let conn = setup();
        let max = U256::MAX.to_be_bytes::<32>().to_vec();
        conn.execute("INSERT INTO t VALUES (?1)", [max]).unwrap();
        conn.execute("INSERT INTO t VALUES (?1)", [be(3)]).unwrap();
        // MAX + 3 overflows: must raise, not wrap.
        let err = conn
            .query_row("SELECT u256_sum(x) FROM t", [], |r| {
                r.get::<_, Option<Vec<u8>>>(0)
            })
            .unwrap_err();
        assert!(err.to_string().contains("overflow"), "{err}");
    }

    #[test]
    fn sums_to_u256_max_without_raising() {
        let conn = setup();
        // MAX - 1, then + 1 lands exactly on MAX: boundary must not raise.
        let max_minus_one = (U256::MAX - U256::from(1u8)).to_be_bytes::<32>().to_vec();
        conn.execute("INSERT INTO t VALUES (?1)", [max_minus_one])
            .unwrap();
        conn.execute("INSERT INTO t VALUES (?1)", [be(1)]).unwrap();
        assert_eq!(sum(&conn), Some(U256::MAX.to_be_bytes::<32>().to_vec()));
    }
}
