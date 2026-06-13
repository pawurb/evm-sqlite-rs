# evm-sqlite - SQLite helper functions for EVM chains data

[![Latest Version](https://img.shields.io/crates/v/evm-sqlite.svg)](https://crates.io/crates/evm-sqlite) [![GH Actions](https://github.com/pawurb/evm-sqlite-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/pawurb/evm-sqlite-rs/actions)

Custom SQLite functions for exact uint256 arithmetic over big-endian BLOBs, plus display helpers that render wei amounts as ether/gwei/USD strings.

EVM `amount` values are 256-bit integers, but SQLite has no native 256-bit type. Storing them as 32-byte big-endian BLOBs keeps byte order equal to numeric order, so `MAX`/`MIN`/`COUNT`/`GROUP BY`/threshold comparisons work natively. Arithmetic does not - a plain `SUM` coerces the BLOB to a float and returns garbage. This crate registers functions that do real 256-bit math and render the results as readable decimals.

Most functions accept each operand either as a non-negative `INTEGER` or as a big-endian `BLOB` of at most 32 bytes (shorter blobs are left-padded). Unless noted otherwise, a `NULL` operand propagates to `NULL`, and a blob longer than 32 bytes (not a valid uint256) or a negative integer raises an error.

## Arithmetic

- `u256_sum(x)` - aggregate. Sums uint256 operands and returns a 32-byte big-endian BLOB. Skips `NULL`. Returns `NULL` over an empty set. Raises if the total overflows `U256::MAX`.
- `u256_mul(a, b)` - scalar. Multiplies two uint256 operands and returns a 32-byte big-endian BLOB. Needed because a product such as `gas_used * effective_gas_price` overflows SQLite's signed 64-bit `INTEGER`. Raises on overflow past `U256::MAX`.
- `u256_add(a, b)` - scalar. Adds two uint256 operands and returns a 32-byte big-endian BLOB. A `NULL` operand yields `NULL`, so an optional addend (e.g. an absent coinbase transfer) nulls the result. Raises on overflow past `U256::MAX`.

## Rendering

- `u256_to_dec(x)` - scalar. Decodes a uint256 to its full-precision decimal string (e.g. `"2014847014830705"`), since a u256 overflows SQLite's signed 64-bit `INTEGER`.
- `format_ether(x)` - scalar. Renders a wei amount as ether with 6 decimals, e.g. `"0.000141 ETH"`.
- `format_gwei(x)` - scalar. Renders a wei amount as gwei with 2 decimals, e.g. `"30.00 gwei"`.
- `convert_usd(x, price)` - scalar. Returns the USD value of a wei amount as a `REAL`, computed as `ether(x) * price`. `price` is a real (or integer) USD price per token; a `NULL` amount or `NULL` price yields `NULL`. Approximate: it goes through `f64`, since the price is itself a float.
- `format_usd(x)` - scalar. Renders a USD value (a real or integer) as `"$"` plus a 2-decimal value with US-style thousands commas, e.g. `"$1,234,567.89"`. This is a pure formatter and does *not* convert from wei; compose it with `convert_usd`, e.g. `format_usd(convert_usd(wei, price))`. A `NULL` value yields `NULL`.
- `erc20_to_real(amount, decimals)` - scalar. Divides a uint256 token amount by `10^decimals` and returns a `REAL`, so numeric SQL works directly on it, e.g. `ROUND(erc20_to_real(u256_sum(amount), 6), 2)` for a USDC total. `decimals` must be an `INTEGER` in `0..=77`; a `NULL` amount or `NULL` decimals yields `NULL`. Approximate (`f64`, ~15-16 significant digits) - use `u256_to_dec` when the exact value matters.

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

// Compose arithmetic and rendering: gas cost in ether and in USD.
let (eth, usd): (String, String) = conn.query_row(
    "SELECT format_ether(u256_mul(gas_used, effective_gas_price)), \
            format_usd(convert_usd(u256_mul(gas_used, effective_gas_price), 2500.5)) \
     FROM transactions WHERE tx_hash = ?1",
    [tx_hash],
    |r| Ok((r.get(0)?, r.get(1)?)),
)?;
```

Functions are per-connection, so call `register_functions` on every connection that needs them.

## License

[MIT](LICENSE.txt)
