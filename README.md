# evm-sqlite - SQLite helper functions for EVM chains data

[![Latest Version](https://img.shields.io/crates/v/evm-sqlite.svg)](https://crates.io/crates/evm-sqlite) [![GH Actions](https://github.com/pawurb/evm-sqlite-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/pawurb/evm-sqlite-rs/actions)

Custom SQLite functions for exact uint256 arithmetic over big-endian BLOBs.

EVM `amount` values are 256-bit integers, but SQLite has no native 256-bit type. Storing them as 32-byte big-endian BLOBs keeps byte order equal to numeric order, so `MAX`/`MIN`/`COUNT`/`GROUP BY`/threshold comparisons work natively. Arithmetic does not - a plain `SUM` coerces the BLOB to a float and returns garbage. This crate registers functions that do real 256-bit math and render the results as readable decimals.

## Functions

- `u256_sum(blob)` - aggregate. Sums big-endian uint256 BLOBs and returns a 32-byte big-endian BLOB. Skips `NULL`. Accepts any blob up to 32 bytes, left-padding shorter ones. Returns `NULL` over an empty set. Raises on a blob longer than 32 bytes (not a valid uint256) and if the total overflows `U256::MAX`.
- `u256_to_dec(blob)` - scalar. Decodes a big-endian uint256 BLOB to its full-precision decimal string (e.g. `"2014847014830705"`), since a u256 overflows SQLite's signed 64-bit `INTEGER`. Accepts any blob up to 32 bytes, left-padding shorter ones. `NULL` passes through to `NULL`. Raises on a non-blob argument or a blob longer than 32 bytes (not a valid uint256).

## Usage

```rust
use rusqlite::Connection;

let conn = Connection::open_in_memory()?;
evm_sqlite::register_functions(&conn)?;

// Wrap the aggregate in `u256_to_dec` to get a readable decimal string
// instead of a raw 32-byte BLOB.
let total: Option<String> = conn.query_row(
    "SELECT u256_to_dec(u256_sum(amount)) FROM transfers",
    [],
    |r| r.get(0),
)?;
```

Functions are per-connection, so call `register_functions` on every connection that needs them.

## License

[MIT](LICENSE.txt)
