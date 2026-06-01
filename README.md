# evm-sqlite - SQLite helper functions for EVM chains data

[![Latest Version](https://img.shields.io/crates/v/evm-sqlite.svg)](https://crates.io/crates/evm-sqlite) [![Downloads](https://img.shields.io/crates/d/evm-sqlite.svg)](https://crates.io/crates/evm-sqlite) [![GH Actions](https://github.com/pawurb/evm-sqlite-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/pawurb/evm-sqlite-rs/actions)

Custom SQLite functions for exact uint256 arithmetic over big-endian BLOBs.

EVM `amount` values are 256-bit integers, but SQLite has no native 256-bit type. Storing them as 32-byte big-endian BLOBs keeps byte order equal to numeric order, so `MAX`/`MIN`/`COUNT`/`GROUP BY`/threshold comparisons work natively. Arithmetic does not — a plain `SUM` coerces the BLOB to a float and returns garbage. This crate registers aggregate functions that do real 256-bit math.

## Functions

- `u256_sum(blob)` - aggregate. Sums 32-byte big-endian uint256 BLOBs and returns a 32-byte big-endian BLOB. Skips `NULL` and any blob that is not exactly 32 bytes. Returns `NULL` over an empty set. Wraps at `U256::MAX`.

## Usage

```rust
use evm_sqlite::register_u256_fns;
use rusqlite::Connection;

let conn = Connection::open_in_memory()?;
register_u256_fns(&conn)?;

let total: Option<Vec<u8>> = conn.query_row(
    "SELECT u256_sum(amount) FROM transfers",
    [],
    |r| r.get(0),
)?;
```

Functions are per-connection, so call `register_u256_fns` on every connection that needs them.

## License

[MIT](LICENSE.txt)
