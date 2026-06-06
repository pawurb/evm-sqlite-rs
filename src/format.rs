//! Scalar SQLite functions that render uint256 wei amounts as display strings.
//!
//! These mirror the display formatting an EVM explorer applies to raw wei:
//! - `format_ether(x)` -> ETH with 6 decimals, e.g. `"0.000141 ETH"`.
//! - `format_gwei(x)`  -> gwei with 2 decimals, e.g. `"30.00 gwei"`.
//! - `format_usd(x, price)` -> `"$"` + USD value with 2 decimals, approximated
//!   as `ether(x) * price` via `f64` (the price is itself a float).
//!
//! `format_ether`/`format_gwei` use exact integer math, so even large uint256
//! values render with correctly-rounded decimals.
//!
//! Each operand may be a non-negative `INTEGER` or a big-endian `BLOB` (<= 32
//! bytes); a `NULL` amount (or, for `format_usd`, a `NULL` price) yields `NULL`.

use ruint::aliases::U256;
use rusqlite::functions::{Context, FunctionFlags};
use rusqlite::types::{Value, ValueRef};
use rusqlite::{Connection, Error, Result};

use crate::args::arg_to_u256;

const WEI_PER_ETHER: f64 = 1e18;

const ETHER_DECIMALS: u32 = 18;
const GWEI_DECIMALS: u32 = 9;

/// Render `value / 10^decimals` to `display` decimal places, rounded half-up,
/// using exact integer math so large uint256 values don't lose precision.
fn format_units(value: U256, decimals: u32, display: u32) -> String {
    let scale = U256::from(10u64).pow(U256::from(decimals));
    let divisor = U256::from(10u64).pow(U256::from(decimals - display));

    let mut int_part = value / scale;
    let fractional = value % scale;
    let mut frac = fractional / divisor;
    if (fractional % divisor) * U256::from(2u64) >= divisor {
        frac += U256::from(1u64);
        if frac == U256::from(10u64).pow(U256::from(display)) {
            frac = U256::ZERO;
            int_part += U256::from(1u64);
        }
    }

    format!(
        "{int_part}.{:0width$}",
        frac.to::<u64>(),
        width = display as usize
    )
}

/// Lossy `U256` -> `f64` via its decimal string.
fn to_f64(value: U256) -> f64 {
    value.to_string().parse::<f64>().unwrap_or(f64::INFINITY)
}

fn format_ether(ctx: &Context<'_>) -> Result<Value> {
    match arg_to_u256(ctx.get_raw(0), "format_ether")? {
        None => Ok(Value::Null),
        Some(wei) => Ok(Value::Text(format!(
            "{} ETH",
            format_units(wei, ETHER_DECIMALS, 6)
        ))),
    }
}

fn format_gwei(ctx: &Context<'_>) -> Result<Value> {
    match arg_to_u256(ctx.get_raw(0), "format_gwei")? {
        None => Ok(Value::Null),
        Some(wei) => Ok(Value::Text(format!(
            "{} gwei",
            format_units(wei, GWEI_DECIMALS, 2)
        ))),
    }
}

fn format_usd(ctx: &Context<'_>) -> Result<Value> {
    let wei = match arg_to_u256(ctx.get_raw(0), "format_usd")? {
        None => return Ok(Value::Null),
        Some(wei) => wei,
    };

    let price = match ctx.get_raw(1) {
        ValueRef::Null => return Ok(Value::Null),
        ValueRef::Real(p) => p,
        ValueRef::Integer(p) => p as f64,
        other => {
            return Err(Error::UserFunctionError(
                format!(
                    "format_usd: price must be a real or integer, got {}",
                    other.data_type()
                )
                .into(),
            ));
        }
    };

    let ether = to_f64(wei) / WEI_PER_ETHER;
    Ok(Value::Text(format!("${:.2}", ether * price)))
}

/// Register the `format_ether`, `format_gwei`, and `format_usd` functions.
pub(crate) fn register(conn: &Connection) -> Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    conn.create_scalar_function("format_ether", 1, flags, format_ether)?;
    conn.create_scalar_function("format_gwei", 1, flags, format_gwei)?;
    conn.create_scalar_function("format_usd", 2, flags, format_usd)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn be(n: u128) -> Vec<u8> {
        U256::from(n).to_be_bytes::<32>().to_vec()
    }

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register(&conn).unwrap();
        conn
    }

    fn call1(conn: &Connection, f: &str, x: Value) -> Result<Option<String>> {
        conn.query_row(&format!("SELECT {f}(?1)"), [x], |r| r.get(0))
    }

    #[test]
    fn ether_formats_six_decimals() {
        let conn = setup();
        // 141166860953425 wei -> 0.000141 ETH.
        assert_eq!(
            call1(&conn, "format_ether", Value::Blob(be(141_166_860_953_425))).unwrap(),
            Some("0.000141 ETH".to_string())
        );
        assert_eq!(
            call1(
                &conn,
                "format_ether",
                Value::Blob(be(1_000_000_000_000_000_000))
            )
            .unwrap(),
            Some("1.000000 ETH".to_string())
        );
    }

    #[test]
    fn ether_rounds_large_values_exactly() {
        let conn = setup();
        // 10000000.000002501 ETH rounds to 6 dp as ...000003. The old f64 path
        // lost precision at this magnitude and rendered ...000002.
        assert_eq!(
            call1(
                &conn,
                "format_ether",
                Value::Blob(be(10_000_000_000_002_501_000_000_000))
            )
            .unwrap(),
            Some("10000000.000003 ETH".to_string())
        );
    }

    #[test]
    fn gwei_formats_two_decimals() {
        let conn = setup();
        assert_eq!(
            call1(&conn, "format_gwei", Value::Integer(30_000_000_000)).unwrap(),
            Some("30.00 gwei".to_string())
        );
    }

    #[test]
    fn null_amount_yields_null() {
        let conn = setup();
        assert_eq!(call1(&conn, "format_ether", Value::Null).unwrap(), None);
        assert_eq!(call1(&conn, "format_gwei", Value::Null).unwrap(), None);
    }

    #[test]
    fn usd_prefixes_dollar_and_rounds() {
        let conn = setup();
        // 0.000141166860953425 ETH * 2500.5 = $0.35.
        let usd: Option<String> = conn
            .query_row(
                "SELECT format_usd(?1, ?2)",
                rusqlite::params![Value::Blob(be(141_166_860_953_425)), 2500.5_f64],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(usd, Some("$0.35".to_string()));
    }

    #[test]
    fn usd_null_amount_or_price_yields_null() {
        let conn = setup();
        let null_amount: Option<String> = conn
            .query_row(
                "SELECT format_usd(?1, ?2)",
                rusqlite::params![Value::Null, 2500.5_f64],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(null_amount, None);

        let null_price: Option<String> = conn
            .query_row(
                "SELECT format_usd(?1, ?2)",
                rusqlite::params![Value::Blob(be(1_000)), Value::Null],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(null_price, None);
    }
}
