//! Scalar SQLite function to render a uint256 BLOB as a decimal string.
//!
//! `amount` columns store a uint256 as a 32-byte big-endian BLOB. SQLite has no
//! 256-bit type and its `INTEGER` is signed 64-bit, so large values can't be
//! coerced to a native number. `u256_to_dec` decodes the BLOB and returns the
//! full-precision decimal string, e.g. `"2014847014830705"`.

use rusqlite::functions::{Context, FunctionFlags};
use rusqlite::types::Value;
use rusqlite::{Connection, Result};

use crate::args::arg_to_u256;

/// Decode the first argument as a uint256 and return its decimal string.
///
/// Accepts a non-negative `INTEGER` or a big-endian `BLOB` of at most 32 bytes
/// (left-padding shorter ones). `NULL` propagates to `NULL`; any other argument,
/// or a blob longer than 32 bytes, raises an error.
fn u256_to_dec(ctx: &Context<'_>) -> Result<Value> {
    match arg_to_u256(ctx.get_raw(0), "u256_to_dec")? {
        None => Ok(Value::Null),
        Some(value) => Ok(Value::Text(value.to_string())),
    }
}

/// Register the `u256_to_dec` scalar function on a connection.
pub(crate) fn register(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "u256_to_dec",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        u256_to_dec,
    )
}

#[cfg(test)]
mod tests {
    use ruint::aliases::U256;

    use super::*;

    /// 32-byte big-endian encoding of a `u128`, for building test rows.
    fn be(n: u128) -> Vec<u8> {
        U256::from(n).to_be_bytes::<32>().to_vec()
    }

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register(&conn).unwrap();
        conn
    }

    fn dec(conn: &Connection, v: Value) -> Result<Option<String>> {
        conn.query_row("SELECT u256_to_dec(?1)", [v], |r| r.get(0))
    }

    #[test]
    fn decodes_known_value() {
        let conn = setup();
        assert_eq!(
            dec(&conn, Value::Blob(be(2014847014830705))).unwrap(),
            Some("2014847014830705".to_string())
        );
    }

    #[test]
    fn zero() {
        let conn = setup();
        assert_eq!(
            dec(&conn, Value::Blob(be(0))).unwrap(),
            Some("0".to_string())
        );
    }

    #[test]
    fn u256_max() {
        let conn = setup();
        let max = U256::MAX.to_be_bytes::<32>().to_vec();
        assert_eq!(
            dec(&conn, Value::Blob(max)).unwrap(),
            Some(U256::MAX.to_string())
        );
    }

    #[test]
    fn null_passes_through() {
        let conn = setup();
        assert_eq!(dec(&conn, Value::Null).unwrap(), None);
    }

    #[test]
    fn short_blob_left_padded() {
        let conn = setup();
        // 7 stored in only 4 bytes decodes the same as a full 32-byte blob.
        assert_eq!(
            dec(&conn, Value::Blob(vec![0, 0, 0, 7])).unwrap(),
            Some("7".to_string())
        );
    }

    #[test]
    fn empty_blob_is_zero() {
        let conn = setup();
        // Zero-length blob left-pads to all zeros.
        assert_eq!(
            dec(&conn, Value::Blob(vec![])).unwrap(),
            Some("0".to_string())
        );
    }

    #[test]
    fn oversized_blob_raises() {
        let conn = setup();
        // 33 bytes: `from_be_slice` would panic, so this must be rejected first.
        let err = dec(&conn, Value::Blob(vec![0u8; 33])).unwrap_err();
        assert!(err.to_string().contains("got 33"), "{err}");
    }

    #[test]
    fn integer_arg_decodes() {
        let conn = setup();
        assert_eq!(
            dec(&conn, Value::Integer(42)).unwrap(),
            Some("42".to_string())
        );
    }

    #[test]
    fn negative_integer_raises() {
        let conn = setup();
        let err = dec(&conn, Value::Integer(-1)).unwrap_err();
        assert!(err.to_string().contains("negative"), "{err}");
    }

    #[test]
    fn real_arg_raises() {
        let conn = setup();
        let err = dec(&conn, Value::Real(1.5)).unwrap_err();
        assert!(err.to_string().contains("u256_to_dec"), "{err}");
    }

    #[test]
    fn text_arg_raises() {
        let conn = setup();
        let err = dec(&conn, Value::Text("123".to_string())).unwrap_err();
        assert!(err.to_string().contains("u256_to_dec"), "{err}");
    }
}
