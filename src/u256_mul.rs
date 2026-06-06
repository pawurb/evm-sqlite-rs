//! Scalar SQLite function for exact uint256 multiplication.
//!
//! SQLite's `INTEGER` is signed 64-bit, so a product like
//! `gas_used * effective_gas_price` overflows a native multiply. `u256_mul`
//! folds both operands with real 256-bit math and returns a 32-byte big-endian
//! BLOB, which `u256_to_dec` / `format_ether` can then render.

use rusqlite::functions::{Context, FunctionFlags};
use rusqlite::types::Value;
use rusqlite::{Connection, Error, Result};

use crate::args::arg_to_u256;

/// Multiply two uint256 operands, returning a 32-byte big-endian BLOB.
///
/// Each operand may be a non-negative `INTEGER` or a `BLOB` of at most 32 bytes.
/// A `NULL` operand yields `NULL`. Raises if the product exceeds `U256::MAX`.
fn u256_mul(ctx: &Context<'_>) -> Result<Value> {
    let a = arg_to_u256(ctx.get_raw(0), "u256_mul")?;
    let b = arg_to_u256(ctx.get_raw(1), "u256_mul")?;

    match (a, b) {
        (Some(a), Some(b)) => {
            let product = a.checked_mul(b).ok_or_else(|| {
                Error::UserFunctionError("u256_mul: overflow past U256::MAX".into())
            })?;
            Ok(Value::Blob(product.to_be_bytes::<32>().to_vec()))
        }
        _ => Ok(Value::Null),
    }
}

/// Register the `u256_mul` scalar function on a connection.
pub(crate) fn register(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "u256_mul",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        u256_mul,
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

    fn mul(conn: &Connection, a: Value, b: Value) -> Result<Option<Vec<u8>>> {
        conn.query_row("SELECT u256_mul(?1, ?2)", [a, b], |r| r.get(0))
    }

    #[test]
    fn multiplies_integers() {
        let conn = setup();
        assert_eq!(
            mul(
                &conn,
                Value::Integer(21_000),
                Value::Integer(30_000_000_000)
            )
            .unwrap(),
            Some(be(630_000_000_000_000))
        );
    }

    #[test]
    fn product_exceeds_i64() {
        let conn = setup();
        // 30_000_000 gas * 2e13 wei = 6e20, well past i64::MAX.
        let gas = 30_000_000u128;
        let price = 20_000_000_000_000u128;
        assert_eq!(
            mul(
                &conn,
                Value::Integer(gas as i64),
                Value::Integer(price as i64)
            )
            .unwrap(),
            Some(be(gas * price))
        );
    }

    #[test]
    fn accepts_blob_operands() {
        let conn = setup();
        assert_eq!(
            mul(&conn, Value::Blob(be(7)), Value::Blob(be(6))).unwrap(),
            Some(be(42))
        );
    }

    #[test]
    fn null_operand_yields_null() {
        let conn = setup();
        assert_eq!(mul(&conn, Value::Null, Value::Integer(5)).unwrap(), None);
        assert_eq!(mul(&conn, Value::Integer(5), Value::Null).unwrap(), None);
    }

    #[test]
    fn overflow_raises() {
        let conn = setup();
        let max = U256::MAX.to_be_bytes::<32>().to_vec();
        let err = mul(&conn, Value::Blob(max), Value::Integer(2)).unwrap_err();
        assert!(err.to_string().contains("overflow"), "{err}");
    }

    #[test]
    fn negative_integer_raises() {
        let conn = setup();
        let err = mul(&conn, Value::Integer(-1), Value::Integer(2)).unwrap_err();
        assert!(err.to_string().contains("negative"), "{err}");
    }
}
