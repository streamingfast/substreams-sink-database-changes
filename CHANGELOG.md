# Change log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [4.0.0]

* The `crate::pb::sf::substreams::sink::database::v1` package has been deprecated, use the full name like any other Protobuf packages we usually provide and which is now at `crate::pb::sf

  Usually migrating is a single matter of a search/replace across your codebase:
  - Replace all `substreams_database_change::pb::database::` by `substreams_database_change::pb::sf::substreams::sink::database::v1::`

    So for example this code:

    ```rust
    use substreams_database_change::pb::database::{
      field::UpdateOp, table_change::Operation, DatabaseChanges, Field, TableChange,
    };
    ```

    Needs to become:

    ```rust
    use substreams_database_change::pb::sf::substreams::sink::database::v1::{
      field::UpdateOp, table_change::Operation, DatabaseChanges, Field, TableChange,
    };
    ```

    *Note*: There is currently no deprecation notice for this as it triggers deprecation notices even inside our own code.

* Add support for delta update operations (`add`/`sub`/`min`/`max`/`set_if_null`) on rows:

  ```rust
  tables.upsert_row("Account", id)
      .set("owner", owner)
      .add("balance", 100i64)      // column = COALESCE(column, 0) + 100
      .sub("debt", 50i64)          // column = COALESCE(column, 0) - 50
      .max("high_score", score)    // column = GREATEST(column, score)
      .min("best_time", duration)  // column = LEAST(column, duration)
      .set_if_null("created_at", timestamp); // column = COALESCE(column, timestamp)
  ```

  Requires latest [substreams-sink-sql](https://github.com/streamingfast/substreams-sink-sql) version for this to be supported correctly.

## [3.0.0]

* Bump substreams to 0.7.0

## [2.1.2]

* Fixed `delete_row` method in `Tables` abstraction: now properly accepts an `Into<PrimaryKey>`.

## [2.1.1]

* Fixed `Tables` abstraction not respecting ordering of operations leading to foreign constraints to often fail because executed out of order.

## [2.1.0]

* Added support for `upsert_row` in the `Tables` abstraction, allowing upsert (insert or update) operations on rows. This requires a development version of `substreams-sink-sql` to operate correctly.

  For usage and reference, see the [Table Operations](README.md#table-operations) section in the documentation.

## [2.0.0]

* Bumped dependencies to `substreams` to 0.6 and `prost` to 0.13 (see [Upgrade notes](https://github.com/streamingfast/substreams-rs/releases/tag/v0.6.0))

## [1.3.1]

Maintenance release introducing experimental hidden methods to deal with arrays.

## [1.3.0]

Better support for using composite primary keys:

* New enum in this crate: `tables::PrimaryKey`
    * `Single(String)`: `let single: PrimaryKey = "hello world".into()`
    * `Composite(BTreeMap<String, String>)`: `let composite: PrimaryKey = [("evt_tx_hash","hello".to_string()),("evt_index","world".to_string())].into()`

Breaking changes:

* The `Rows.pks` field is not public anymore.
* `create_row()`, `update_row()` and `delete_row()` now require a `PrimaryKey` instead of a `String`. This should work directly with a `String`, `&String` or `&str`.

## [1.2.1]

* Changed imports in `substreams.yaml` definition so that packaged `.spkg` can you the expect path `sf/substreams/sink/database/v1` to exclude and generating from the `.spkg` will generate data on the right path.

If you were using `--exclude-paths` on `substreams protogen` and your path was `database.proto`, you should change to `sf/substreams/sink/database/v1`. We think this affects no one has everyone is probably using the pre-rendered Protobuf found in Rust crate `substreams-database-change`.

## [1.2.0]

* **Breaking** Renamed `substreams_database_change::pb::database::table_change::Operation::Unset` to be become `...::Operation::Unspecified` to make the Protobuf conforms to buf lint rules.

  We take the liberty to change it because we expect that almost everyone is using the abstraction provided by this library.

## [1.1.3]

* Removed some useless dependencies.

## [1.1.2]

* Added some missing conversion for `ToDatabaseValue` trait.

## [1.1.1]

* Fixed encoding of `Vec<u8>` to be in `hex` format as expected by Postgres (and hopefully other database). This is technically a breaking change but it was probably never actually working, so we are changing. If you were relying on `base64` decoding, please let use know and we will find a solution.

## [1.1.0]

* Added `tables` module so that you can use it as a better abstraction to build up your entity changes.

  ```rust
  use substreams_database_change::tables::Tables;

  let mut tables = Tables::new();

  // Create a row (<entity_name>, <id>).[set(<column>, <value>), ...]
  tables
    .create_row("Factory", id)
    .set("poolCount", &bigint0)
    .set("txCount", &bigint0)
    .set("totalVolumeUSD", &bigdecimal0)
    .set("totalVolumeETH", &bigdecimal0)
    .set("totalFeesUSD", &bigdecimal0)
    .set("totalFeesETH", &bigdecimal0)
    .set("untrackedVolumeUSD", &bigdecimal0)
    .set("totalValueLockedUSD", &bigdecimal0)
    .set("totalValueLockedETH", &bigdecimal0)
    .set("totalValueLockedUSDUntracked", &bigdecimal0)
    .set("totalValueLockedETHUntracked", &bigdecimal0)
    .set("owner", &format!("0x{}", Hex(utils::ZERO_ADDRESS).to_string()));

  // Update a row (<entity_name>, <id>).[set(<column>, <value>), ...]
  tables
    .update_row("Bundle", "1").set("ethPriceUSD", &delta.new_value);
  ```

* Add composite primary keys for Protobuf with `oneof` and add `push_change_composite()` to `helpers.rs`.

* **Note** Changed database protobuf location path from ~`substreams/sink/database/v1`~ to `sf/substreams/sink/database/v1`, this is **not** a change of the Protobuf package id which is still `sf.substreams.sink.database.v1`. This will have an impact if you use `buf gen --exclude-path` as the path here is the Protobuf original path location, so add `sf/` if you were excluding `substreams/sink/database`.

* GitHub repository has been renamed to `substreams-sink-database-changes`, **note** the Rust crate's name is still `substreams-database-change` to avoid breaking change when importing the crate. We are planning a future rename of the crate, but that might come with a v2 of the data protocol.

## [1.0.1]

* Separate generic types for table name and private key

## [1.0.0]

* **BREAKING** Changed database proto package path from ~substreams.sink.database.v1~ to `sf.substreams.sink.database.v1`

## [0.2.0]

* **BREAKING** Updated `substreams` dependencies to `0.5.0`.

## [0.1.2]

* Added support for `substreams::Hex` type which converts to `string` in hexadecimal form.

## [0.1.1](https://github.com/streamingfast/substreams-sink-database-changes/releases/tag/v0.1.1)

* Added support for `prost::Timestamp` type.

* Made `AsString` public so you can implement on your own custom types.

## [0.1.0](https://github.com/streamingfast/substreams-sink-database-changes/releases/tag/v0.1.0)

* Added support for `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64` types.

* Added possibility to record a `change` using `(new: AsString, old: AsString)` to simulate a value update.

* Added possibility to record a `change` using `(old: AsString, new: Option<Into<Typed>>)` to simulate a value deletion.

* Added possibility to record a `change` using `(old: Option<AsString>, new: AsString)` to simulate a value creation.

* Refactored to allow delta to be taken from any `AsString` which makes it much easier to extend when there is missing types.

* Introduced our own `AsString` type because `Into<String>` usage and `ToString` usage lead to Rust compiler errors on our desired change typings.

* Refactored to reduce amount of `clone` perform.

* Added `database.proto` containing proto message definitions
