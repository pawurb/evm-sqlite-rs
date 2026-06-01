//! SQLite helper functions for working with EVM chains data.

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
    Ok(())
}
