//! Scalar SQLite function that converts an ERC20 token amount to a `REAL`.
//!
//! Token amounts are stored in base units (e.g. USDC has 6 decimals, so
//! `1500000` means `1.5` USDC). `erc20_to_real(amount, decimals)` divides the
//! uint256 amount by `10^decimals` and returns a SQLite `REAL`, so standard
//! numeric SQL (`ROUND`, `printf`, comparisons) works directly on the result.
//!
//! This is approximate: it goes through `f64`, which holds ~15-16 significant
//! digits. That is plenty for display, but use `u256_to_dec` when the exact
//! integer value matters.

use rusqlite::functions::{Context, FunctionFlags};
use rusqlite::types::{Value, ValueRef};
use rusqlite::{Connection, Error, Result};

use crate::args::arg_to_u256;
use crate::format::to_f64;

/// 10^78 exceeds `U256::MAX`, so larger scales are nonsensical.
const MAX_DECIMALS: i64 = 77;

/// Divide a uint256 token amount by `10^decimals`, returning a `REAL`.
///
/// `amount` may be a non-negative `INTEGER` or a `BLOB` of at most 32 bytes;
/// `decimals` must be an `INTEGER` in `0..=77`. A `NULL` amount or `NULL`
/// decimals yields `NULL`.
fn erc20_to_real(ctx: &Context<'_>) -> Result<Value> {
    let amount = match arg_to_u256(ctx.get_raw(0), "erc20_to_real")? {
        None => return Ok(Value::Null),
        Some(amount) => amount,
    };

    let decimals = match ctx.get_raw(1) {
        ValueRef::Null => return Ok(Value::Null),
        ValueRef::Integer(d) if (0..=MAX_DECIMALS).contains(&d) => d,
        ValueRef::Integer(d) => {
            return Err(Error::UserFunctionError(
                format!("erc20_to_real: decimals must be in 0..={MAX_DECIMALS}, got {d}").into(),
            ));
        }
        other => {
            return Err(Error::UserFunctionError(
                format!(
                    "erc20_to_real: decimals must be an integer, got {}",
                    other.data_type()
                )
                .into(),
            ));
        }
    };

    Ok(Value::Real(to_f64(amount) / 10f64.powi(decimals as i32)))
}

/// Register the `erc20_to_real` scalar function on a connection.
pub(crate) fn register(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "erc20_to_real",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        erc20_to_real,
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

    fn call(conn: &Connection, amount: Value, decimals: Value) -> Result<Option<f64>> {
        conn.query_row("SELECT erc20_to_real(?1, ?2)", [amount, decimals], |r| {
            r.get(0)
        })
    }

    #[test]
    fn converts_usdc_six_decimals() {
        let conn = setup();
        assert_eq!(
            call(&conn, Value::Blob(be(1_500_000)), Value::Integer(6)).unwrap(),
            Some(1.5)
        );
    }

    #[test]
    fn accepts_integer_amount() {
        let conn = setup();
        assert_eq!(
            call(&conn, Value::Integer(2_750_000), Value::Integer(6)).unwrap(),
            Some(2.75)
        );
    }

    #[test]
    fn zero_decimals_is_identity() {
        let conn = setup();
        assert_eq!(
            call(&conn, Value::Blob(be(1_337)), Value::Integer(0)).unwrap(),
            Some(1337.0)
        );
    }

    #[test]
    fn null_amount_or_decimals_yields_null() {
        let conn = setup();
        assert_eq!(call(&conn, Value::Null, Value::Integer(6)).unwrap(), None);
        assert_eq!(
            call(&conn, Value::Blob(be(1_000)), Value::Null).unwrap(),
            None
        );
    }

    #[test]
    fn large_value_approximates() {
        let conn = setup();
        // 10^30 base units with 18 decimals -> 10^12.
        let amount = U256::from(10u8).pow(U256::from(30u8)).to_be_bytes::<32>();
        let got = call(&conn, Value::Blob(amount.to_vec()), Value::Integer(18))
            .unwrap()
            .unwrap();
        assert!((got - 1e12).abs() < 1e-3, "{got}");
    }

    #[test]
    fn out_of_range_decimals_raises() {
        let conn = setup();
        let err = call(&conn, Value::Blob(be(1_000)), Value::Integer(-1)).unwrap_err();
        assert!(
            err.to_string().contains("decimals must be in 0..=77"),
            "{err}"
        );

        let err = call(&conn, Value::Blob(be(1_000)), Value::Integer(78)).unwrap_err();
        assert!(
            err.to_string().contains("decimals must be in 0..=77"),
            "{err}"
        );
    }

    #[test]
    fn non_integer_decimals_raises() {
        let conn = setup();
        let err = call(&conn, Value::Blob(be(1_000)), Value::Real(6.0)).unwrap_err();
        assert!(err.to_string().contains("must be an integer"), "{err}");

        let err = call(&conn, Value::Blob(be(1_000)), Value::Text("6".into())).unwrap_err();
        assert!(err.to_string().contains("must be an integer"), "{err}");
    }
}
