//! Shared argument decoding for the uint256 SQL functions.

use ruint::aliases::U256;
use rusqlite::types::ValueRef;
use rusqlite::{Error, Result};

/// Decode a function argument into a `U256`.
///
/// Accepts a non-negative `INTEGER` or a big-endian `BLOB` of at most 32 bytes
/// (shorter blobs are left-padded). `NULL` decodes to `None` so callers can
/// propagate it. A negative integer, an oversized blob, or any other type
/// raises an error tagged with `fname`.
pub(crate) fn arg_to_u256(value: ValueRef<'_>, fname: &str) -> Result<Option<U256>> {
    match value {
        ValueRef::Null => Ok(None),
        ValueRef::Integer(n) => {
            if n < 0 {
                return Err(Error::UserFunctionError(
                    format!("{fname}: negative integer {n} cannot be a uint256").into(),
                ));
            }
            Ok(Some(U256::from(n as u64)))
        }
        ValueRef::Blob(b) => {
            if b.len() > 32 {
                return Err(Error::UserFunctionError(
                    format!(
                        "{fname}: blob too large, expected <= 32 bytes, got {}",
                        b.len()
                    )
                    .into(),
                ));
            }
            Ok(Some(U256::from_be_slice(b)))
        }
        other => Err(Error::UserFunctionError(
            format!(
                "{fname}: expected an integer or a blob of at most 32 bytes, got {}",
                other.data_type()
            )
            .into(),
        )),
    }
}
