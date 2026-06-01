//! Scalar SQLite function to render a uint256 BLOB as a decimal string.
//!
//! `amount` columns store a uint256 as a 32-byte big-endian BLOB. SQLite has no
//! 256-bit type and its `INTEGER` is signed 64-bit, so large values can't be
//! coerced to a native number. `u256_to_dec` decodes the BLOB and returns the
//! full-precision decimal string, e.g. `"2014847014830705"`.

use ruint::aliases::U256;
use rusqlite::functions::{Context, FunctionFlags};
use rusqlite::types::{Value, ValueRef};
use rusqlite::{Connection, Error, Result};

/// Decode the first argument as a big-endian uint256 BLOB and return its
/// decimal string.
///
/// Accepts any blob up to 32 bytes, left-padding shorter ones (a 32-byte BE
/// uint256 with leading zero bytes trimmed decodes to the same value). `NULL`
/// propagates to `NULL`. A non-blob argument, or a blob longer than 32 bytes
/// raises an error.
fn u256_to_dec(ctx: &Context<'_>) -> Result<Value> {
    match ctx.get_raw(0) {
        ValueRef::Null => Ok(Value::Null),
        ValueRef::Blob(b) => {
            // `from_be_slice` left-pads slices <= 32 bytes but panics on >32,
            // so the upper-bound guard is required.
            if b.len() > 32 {
                return Err(Error::UserFunctionError(
                    format!(
                        "u256_to_dec: blob too large, expected <= 32 bytes, got {}",
                        b.len()
                    )
                    .into(),
                ));
            }
            Ok(Value::Text(U256::from_be_slice(b).to_string()))
        }
        other => Err(Error::UserFunctionError(
            format!(
                "u256_to_dec: expected a blob of at most 32 bytes, got {}",
                other.data_type()
            )
            .into(),
        )),
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
    fn integer_arg_raises() {
        let conn = setup();
        let err = dec(&conn, Value::Integer(42)).unwrap_err();
        assert!(err.to_string().contains("u256_to_dec"), "{err}");
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
