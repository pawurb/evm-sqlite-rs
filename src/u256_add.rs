//! Scalar SQLite function for exact uint256 addition.
//!
//! Adds two uint256 operands (e.g. gas cost + coinbase transfer) with real
//! 256-bit math and returns a 32-byte big-endian BLOB. Unlike [`u256_sum`], this
//! is a scalar over two arguments, and a `NULL` operand propagates to `NULL` so
//! an optional addend (such as an untraced coinbase transfer) nulls the result.
//!
//! [`u256_sum`]: crate::register_functions

use rusqlite::functions::{Context, FunctionFlags};
use rusqlite::types::Value;
use rusqlite::{Connection, Error, Result};

use crate::args::arg_to_u256;

/// Add two uint256 operands, returning a 32-byte big-endian BLOB.
///
/// Each operand may be a non-negative `INTEGER` or a `BLOB` of at most 32 bytes.
/// A `NULL` operand yields `NULL`. Raises if the sum exceeds `U256::MAX`.
fn u256_add(ctx: &Context<'_>) -> Result<Value> {
    let a = arg_to_u256(ctx.get_raw(0), "u256_add")?;
    let b = arg_to_u256(ctx.get_raw(1), "u256_add")?;

    match (a, b) {
        (Some(a), Some(b)) => {
            let sum = a.checked_add(b).ok_or_else(|| {
                Error::UserFunctionError("u256_add: overflow past U256::MAX".into())
            })?;
            Ok(Value::Blob(sum.to_be_bytes::<32>().to_vec()))
        }
        _ => Ok(Value::Null),
    }
}

/// Register the `u256_add` scalar function on a connection.
pub(crate) fn register(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "u256_add",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        u256_add,
    )
}

#[cfg(test)]
mod tests {
    use ruint::aliases::U256;

    use super::*;

    fn be(n: u128) -> Vec<u8> {
        U256::from(n).to_be_bytes::<32>().to_vec()
    }

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register(&conn).unwrap();
        conn
    }

    fn add(conn: &Connection, a: Value, b: Value) -> Result<Option<Vec<u8>>> {
        conn.query_row("SELECT u256_add(?1, ?2)", [a, b], |r| r.get(0))
    }

    #[test]
    fn adds_mixed_operands() {
        let conn = setup();
        assert_eq!(
            add(&conn, Value::Blob(be(1_000)), Value::Integer(337)).unwrap(),
            Some(be(1_337))
        );
    }

    #[test]
    fn null_operand_yields_null() {
        let conn = setup();
        // full_tx_cost = u256_add(tx_cost, coinbase) must null out when untraced.
        assert_eq!(add(&conn, Value::Blob(be(100)), Value::Null).unwrap(), None);
        assert_eq!(add(&conn, Value::Null, Value::Blob(be(100))).unwrap(), None);
    }

    #[test]
    fn overflow_raises() {
        let conn = setup();
        let max = U256::MAX.to_be_bytes::<32>().to_vec();
        let err = add(&conn, Value::Blob(max), Value::Integer(1)).unwrap_err();
        assert!(err.to_string().contains("overflow"), "{err}");
    }

    #[test]
    fn sums_to_u256_max_without_raising() {
        let conn = setup();
        let max_minus_one = (U256::MAX - U256::from(1u8)).to_be_bytes::<32>().to_vec();
        assert_eq!(
            add(&conn, Value::Blob(max_minus_one), Value::Integer(1)).unwrap(),
            Some(U256::MAX.to_be_bytes::<32>().to_vec())
        );
    }
}
