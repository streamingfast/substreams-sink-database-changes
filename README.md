# Substreams Sink Database Changes

[<img alt="github" src="https://img.shields.io/badge/Github-substreams.database-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/streamingfast/substreams-sink-database-changes)
[<img alt="crates.io" src="https://img.shields.io/crates/v/substreams-database-change.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/substreams-database-change)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-substreams.database-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/substreams-database-change)
[<img alt="GitHub Workflow Status" src="https://img.shields.io/github/actions/workflow/status/streamingfast/substreams-sink-database-changes/ci.yml?branch=develop&style=for-the-badge" height="20">](https://github.com/streamingfast/substreams-sink-database-changes/actions?query=branch%3Adevelop)

> `substreams-sink-database-changes` contains all the definitions for database changes which can be emitted by a substream.

## To be consumed by

- [substreams-sink-sql](https://github.com/streamingfast/substreams-sink-sql)

## Install

```bash
# The Rust crate is named substreams-database-change for historical reasons
cargo add substreams-database-change
```

## Quickstart

**Cargo.toml**

```toml
[dependencies]
substreams = "0.7"
substreams-database-change = "4.0"
```

**src/lib.rs**

```rust
use substreams::errors::Error;
use substreams_database_change::tables::Tables;
use substreams_database_change::pb::database::DatabaseChanges;

#[substreams::handlers::map]
fn db_out(
    ... some stores ...
) -> Result<DatabaseChanges, Error> {
    let mut tables = Tables::new();

    // Create a row and set fields
    tables
        .create_row("transfer", "some-id")
        .set("key1", "value1")
        .set("key2", "value2");

    // Update a row (for example, change key2)
    tables
        .update_row("transfer", "some-id")
        .set("key2", "new_value2");

    Ok(tables.to_database_changes())
}
```

### Reference

#### Table Operations

- **Create a row**
  ```rust
  tables.create_row("table_name", "primary_key")
      .set("field", "value");
  ```
  Creates a new row. Panics if the row is already scheduled for upsert, update, or delete.

- **Upsert a row**
  ```rust
  tables.upsert_row("table_name", "primary_key")
      .set("field", "value");
  ```
  Schedules an insert or update (upsert) for the row. Panics if the row is already scheduled for create, update, or delete.

- **Update a row**
  ```rust
  tables.update_row("table_name", "primary_key")
      .set("field", "new_value");
  ```
  Schedules an update for the row. Panics if the row is already scheduled for delete.

- **Delete a row**
  ```rust
  tables.delete_row("table_name", "primary_key");
  ```
  Schedules a delete for the row. Clears any previously set fields.

All methods support both single and composite primary keys:
```rust
tables.create_row("table", [("key1", "v1".to_string()), ("key2", "v2".to_string())]);
```

#### Automatic Type Transformations

The `.set()` method automatically converts many Rust types to database-compatible strings, including:
- Integers: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`
- `bool`
- `String`, `&str`
- `BigInt`, `BigDecimal` (from `substreams::scalar`)
- `prost_types::Timestamp`
- `Vec<u8>`, `Hex<T>` (as hex strings)

Custom types can implement the `ToDatabaseValue` trait for custom conversion.

For advanced use, `.set_raw()` allows setting a field to a raw string value.

### Re-generate Protobuf

Be sure to have `buf` CLI installed (https://buf.build/docs/installation/) and run:

```bash
buf generate proto
```

## Release

```bash
sfreleaser release
```

Follow instructions the CLI is asking, the process is now automatic and version bump and Substreams package building is now all done automatically.