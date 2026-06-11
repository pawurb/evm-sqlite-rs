//! SQLite helper functions for working with EVM chains data.

mod args;
mod erc20_to_real;
mod format;
mod u256_add;
mod u256_mul;
mod u256_sum;
mod u256_to_dec;

use rusqlite::{Connection, Result};

/// Register all of this crate's custom SQLite functions on a connection.
///
/// Functions are per-connection, so call this on every connection that needs
/// them.
pub fn register_functions(conn: &Connection) -> Result<()> {
    u256_sum::register(conn)?;
    u256_to_dec::register(conn)?;
    u256_mul::register(conn)?;
    u256_add::register(conn)?;
    format::register(conn)?;
    erc20_to_real::register(conn)?;
    Ok(())
}
