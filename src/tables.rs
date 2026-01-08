use crate::pb::database::{table_change::Operation, field::UpdateOp, DatabaseChanges, Field, TableChange};
use std::collections::{BTreeMap, HashMap};
use substreams::{
    scalar::{BigDecimal, BigInt},
    Hex,
};

#[derive(Debug)]
pub struct Tables {
    // Map from table name to the primary keys within that table
    pub tables: HashMap<String, Rows>,

    // Ordinal is used to track the order of changes, it is incremented for each row
    // in such way that at the end, we can correctly order the changes back correctly.
    ordinal: Ordinal,
}

impl Tables {
    pub fn new() -> Self {
        Tables {
            tables: HashMap::new(),
            ordinal: Ordinal::new(),
        }
    }

    /// Returns the number of rows in all tables.
    pub fn all_row_count(&self) -> usize {
        self.tables.values().map(|rows| rows.pks.len()).sum()
    }

    /// Create a new row in the table with the given primary key.
    ///
    /// ```
    /// // With a Primary Key of type `Single`
    /// use crate::substreams_database_change::tables::Tables;
    /// let mut tables = Tables::new();
    /// tables.create_row("myevent", "my_key",);
    /// ```
    ///
    /// ```
    /// // With a Primary Key of type `Composite`
    /// use crate::substreams_database_change::tables::Tables;
    /// let mut tables = Tables::new();
    /// tables.create_row("myevent", [("evt_tx_hash", String::from("hello")), ("evt_index", String::from("world"))]);
    /// ```
    pub fn create_row<K: Into<PrimaryKey>>(&mut self, table: &str, key: K) -> &mut Row {
        let rows: &mut Rows = self.tables.entry(table.to_string()).or_insert(Rows::new());
        let k = key.into();
        let key_debug = format!("{:?}", k);
        let row = rows
            .pks
            .entry(k)
            .or_insert(Row::new_ordered(self.ordinal.next()));
        match row.operation {
            Operation::Unspecified => {
                row.operation = Operation::Create;
            }
            Operation::Create => { /* Already the right operation */ }
            Operation::Upsert => {
                panic!(
                    "cannot create a row after a scheduled upsert operation, create and upsert are exclusive - table: {} key: {}",
                    table, key_debug,
                )
            }
            Operation::Update => {
                panic!("cannot create a row that was marked for update")
            }
            Operation::Delete => {
                panic!(
                    "cannot create a row after a scheduled delete operation - table: {} key: {}",
                    table, key_debug,
                )
            }
        }
        row
    }

    /// Upsert (insert or update) a new row in the table with the given primary key.
    ///
    /// *Note* Ensure that the SQL sink driver you use supports upsert operations.
    ///
    /// ```
    /// // With a Primary Key of type `Single`
    /// use crate::substreams_database_change::tables::Tables;
    /// let mut tables = Tables::new();
    /// tables.upsert_row("myevent", "my_key",);
    /// ```
    ///
    /// ```
    /// // With a Primary Key of type `Composite`
    /// use crate::substreams_database_change::tables::Tables;
    /// let mut tables = Tables::new();
    /// tables.upsert_row("myevent", [("evt_tx_hash", String::from("hello")), ("evt_index", String::from("world"))]);
    /// ```
    pub fn upsert_row<K: Into<PrimaryKey>>(&mut self, table: &str, key: K) -> &mut Row {
        let rows = self.tables.entry(table.to_string()).or_insert(Rows::new());
        let k = key.into();
        let key_debug = format!("{:?}", k);
        let row = rows
            .pks
            .entry(k)
            .or_insert(Row::new_ordered(self.ordinal.next()));
        match row.operation {
            Operation::Unspecified => {
                row.operation = Operation::Upsert;
            }
            Operation::Create => {
                panic!(
                    "cannot upsert a row after a scheduled create operation, create and upsert are exclusive - table: {} key: {}",
                    table, key_debug,
                )
            }
            Operation::Upsert => { /* Already the right operation */ }
            Operation::Update => {
                panic!(
                    "cannot upsert a row after a scheduled update operation, update and upsert are exclusive - table: {} key: {}",
                    table, key_debug,
                )
            }
            Operation::Delete => {
                panic!(
                    "cannot upsert a row after a scheduled delete operation - table: {} key: {}",
                    table, key_debug,
                )
            }
        }
        row
    }

    pub fn update_row<K: Into<PrimaryKey>>(&mut self, table: &str, key: K) -> &mut Row {
        let rows = self.tables.entry(table.to_string()).or_insert(Rows::new());
        let k = key.into();
        let key_debug = format!("{:?}", k);
        let row = rows
            .pks
            .entry(k)
            .or_insert(Row::new_ordered(self.ordinal.next()));
        match row.operation {
            Operation::Unspecified => {
                row.operation = Operation::Update;
            }
            Operation::Create => { /* Fine, updated columns will be part of Insert operation */ }
            Operation::Upsert => { /* Fine, updated columns will be part of Upsert operation */ }
            Operation::Update => { /* Already the right operation */ }
            Operation::Delete => {
                panic!(
                    "cannot update a row after a scheduled delete operation - table: {} key: {}",
                    table, key_debug,
                )
            }
        }
        row
    }

    pub fn delete_row<K: Into<PrimaryKey>>(&mut self, table: &str, key: K) -> &mut Row {
        let rows = self.tables.entry(table.to_string()).or_insert(Rows::new());
        let row = rows
            .pks
            .entry(key.into())
            .or_insert(Row::new_ordered(self.ordinal.next()));

        row.columns = HashMap::new();
        row.operation = match row.operation {
            Operation::Unspecified => Operation::Delete,
            Operation::Create => {
                // We are creating the row in this block, there is no need to emit a DELETE statement,
                // we specify Unspecified and the row will be skipped when comes the time to emit the
                // changes.
                Operation::Unspecified
            }
            Operation::Upsert => {
                // We cannot know if the row was created within that block or already present
                // in the database. As such, we must emit a DELETE statement in the sink
                // for this. Worst case, the DELETE will hit no row and be a no-op.
                Operation::Delete
            }
            Operation::Update => {
                // The row must be deleted, emit the operation
                Operation::Delete
            }
            Operation::Delete => {
                // Already delete type, continue using that as the operation
                Operation::Delete
            }
        };

        row
    }

    // Convert Tables into an DatabaseChanges protobuf object
    pub fn to_database_changes(self) -> DatabaseChanges {
        let mut changes = DatabaseChanges::default();

        for (table, rows) in self.tables.into_iter() {
            for (pk, row) in rows.pks.into_iter() {
                if row.operation == Operation::Unspecified {
                    continue;
                }

                let mut change = match pk {
                    PrimaryKey::Single(pk) => {
                        TableChange::new(table.clone(), pk, row.ordinal, row.operation)
                    }
                    PrimaryKey::Composite(keys) => TableChange::new_composite(
                        table.clone(),
                        keys.into_iter().collect(),
                        row.ordinal,
                        row.operation,
                    ),
                };

                for (field, field_value) in row.columns.into_iter() {
                    change.fields.push(Field {
                        name: field,
                        new_value: field_value.value,
                        old_value: "".to_string(),
                        update_op: field_value.update_op as i32,
                    });
                }

                changes.table_changes.push(change);
            }
        }

        changes.table_changes.sort_by_key(|change| change.ordinal);
        changes
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Ordinal(u64);

impl Ordinal {
    pub fn new() -> Self {
        Ordinal(0)
    }

    pub fn next(&mut self) -> u64 {
        let current = self.0;
        self.0 += 1;
        current
    }
}

#[derive(Hash, Debug, Eq, PartialEq)]
pub enum PrimaryKey {
    Single(String),
    Composite(BTreeMap<String, String>),
}

impl From<&str> for PrimaryKey {
    fn from(x: &str) -> Self {
        Self::Single(x.to_string())
    }
}

impl From<&String> for PrimaryKey {
    fn from(x: &String) -> Self {
        Self::Single(x.clone())
    }
}

impl From<String> for PrimaryKey {
    fn from(x: String) -> Self {
        Self::Single(x)
    }
}

impl<K: AsRef<str>, const N: usize> From<[(K, String); N]> for PrimaryKey {
    fn from(arr: [(K, String); N]) -> Self {
        if N == 0 {
            return Self::Composite(BTreeMap::new());
        }

        let string_arr = arr.map(|(k, v)| (k.as_ref().to_string(), v));
        Self::Composite(BTreeMap::from(string_arr))
    }
}

impl<K: AsRef<str>, const N: usize> From<[(K, &str); N]> for PrimaryKey {
    fn from(arr: [(K, &str); N]) -> Self {
        if N == 0 {
            return Self::Composite(BTreeMap::new());
        }

        let string_arr = arr.map(|(k, v)| (k.as_ref().to_string(), v.to_string()));
        Self::Composite(BTreeMap::from(string_arr))
    }
}

#[derive(Debug)]
pub struct Rows {
    // Map of primary keys within this table, to the fields within
    pks: HashMap<PrimaryKey, Row>,
}

impl Rows {
    pub fn new() -> Self {
        Rows {
            pks: HashMap::new(),
        }
    }
}

/// Holds field value and its update operation for UPSERT handling.
#[derive(Debug, Clone, Default)]
struct FieldValue {
    #[allow(dead_code)]
    value: String,
    #[allow(dead_code)]
    update_op: UpdateOp,
}

impl FieldValue {
    fn new(value: String) -> Self {
        FieldValue {
            value,
            update_op: UpdateOp::Set,
        }
    }

    fn with_op(value: String, update_op: UpdateOp) -> Self {
        FieldValue { value, update_op }
    }
}

#[derive(Debug, Default)]
pub struct Row {
    /// Verify that we don't try to delete the same row as we're creating it
    pub operation: Operation,
    /// Map of field name to its value and update operation
    #[allow(private_interfaces)]
    pub columns: HashMap<String, FieldValue>,
    /// Finalized: Last update or delete
    #[deprecated(
        note = "The finalization state is now implicitly handled by the `operation` field."
    )]
    pub finalized: bool,

    ordinal: u64,
}

impl Row {
    /// **Do not use** Now broken, use the `Tables` API instead like `create_row`, `upsert_row`, `update_row`, or `delete_row`.
    /// Kept for code compilation but it's expected that this was never used in practice.
    #[deprecated(
        note = "Do now create a new row manually, use the `Tables` API instead like `create_row`, `upsert_row`, `update_row`, or `delete_row`"
    )]
    pub fn new() -> Self {
        Row {
            operation: Operation::Unspecified,
            columns: HashMap::new(),
            ..Default::default()
        }
    }

    pub(crate) fn new_ordered(ordinal: u64) -> Self {
        Row {
            operation: Operation::Unspecified,
            columns: HashMap::new(),
            ordinal,
            ..Default::default()
        }
    }

    /// Set a field to a value, this is the standard method for setting fields in a row.
    ///
    /// This method ensures that the value is converted to a database-compatible format
    /// using the `ToDatabaseValue` trait. It is the primary way to set fields in a row
    /// for most use cases.
    ///
    /// The `ToDatabaseValue` trait is implemented for various types, including primitive
    /// types, strings, and custom types. This allows you to set fields with different
    /// types of values without worrying about the underlying conversion. Check example
    /// for more details.
    ///
    /// Check [ToDatabaseValue] for implemented automatic conversions.
    ///
    /// # Panics
    ///
    /// This method will panic if called on a row marked for deletion.
    ///
    /// # Example
    ///
    /// ```
    /// use substreams::scalar::{BigInt, BigDecimal};
    /// use crate::substreams_database_change::tables::Tables;
    /// let mut tables = Tables::new();
    /// let row = tables.create_row("myevent", "my_key");
    /// row.set("name", "asset name");
    /// row.set("decimals", 42);
    /// row.set("count", BigDecimal::from(42));
    /// row.set("value", BigInt::from(42));
    /// ```
    pub fn set<T: ToDatabaseValue>(&mut self, name: &str, value: T) -> &mut Self {
        if self.operation == Operation::Delete {
            panic!("cannot set fields on a delete operation")
        }
        // Check if field already has a non-Set operation - can't go back to Set
        if let Some(existing) = self.columns.get(name) {
            if existing.update_op != UpdateOp::Set {
                panic!(
                    "cannot call set() on field '{}' after {}() - set() must be called first",
                    name,
                    match existing.update_op {
                        UpdateOp::Add => "add/sub",
                        UpdateOp::Max => "max",
                        UpdateOp::Min => "min",
                        UpdateOp::SetIfNull => "set_if_null",
                        UpdateOp::Set => unreachable!(),
                    }
                )
            }
        }
        self.columns.insert(name.to_string(), FieldValue::new(value.to_value()));
        self
    }

    /// Add to the existing value: column = COALESCE(column, 0) + new_value
    /// Used with upsert_row() for accumulating values like counters or balances.
    /// If called multiple times for the same column within a block, values are accumulated.
    pub fn add<T: ToDatabaseValue>(&mut self, name: &str, value: T) -> &mut Self {
        if self.operation == Operation::Delete {
            panic!("cannot set fields on a delete operation")
        }
        self.accumulate_add(name, &value.to_value(), false);
        self
    }

    /// Subtract from the existing value: column = COALESCE(column, 0) - new_value
    /// Convenience method that negates the value and uses ADD operation.
    /// If called multiple times for the same column within a block, values are accumulated.
    pub fn sub<T: ToDatabaseValue>(&mut self, name: &str, value: T) -> &mut Self {
        if self.operation == Operation::Delete {
            panic!("cannot set fields on a delete operation")
        }
        self.accumulate_add(name, &value.to_value(), true);
        self
    }

    /// Internal helper to accumulate ADD values for the same column within a block.
    /// - Set + Add/Sub: accumulate values, keep Set op (full value for INSERT)
    /// - Add + Add/Sub: accumulate values, keep Add op (delta for UPDATE)
    /// - No existing: store as Add op (delta)
    /// - Other existing ops (Max/Min/SetIfNull): PANIC (invalid transition)
    fn accumulate_add(&mut self, name: &str, value: &str, negate: bool) {
        use std::str::FromStr;

        // Check for invalid transitions first
        if let Some(existing) = self.columns.get(name) {
            match existing.update_op {
                UpdateOp::Set | UpdateOp::Add => {} // Valid transitions
                UpdateOp::Max => panic!(
                    "cannot call add/sub() on field '{}' after max() - incompatible operations",
                    name
                ),
                UpdateOp::Min => panic!(
                    "cannot call add/sub() on field '{}' after min() - incompatible operations",
                    name
                ),
                UpdateOp::SetIfNull => panic!(
                    "cannot call add/sub() on field '{}' after set_if_null() - incompatible operations",
                    name
                ),
            }
        }

        let value_str = if negate {
            if value.starts_with('-') {
                value[1..].to_string()
            } else {
                format!("-{}", value)
            }
        } else {
            value.to_string()
        };

        let new_decimal = BigDecimal::from_str(&value_str)
            .unwrap_or_else(|_| panic!("add/sub() requires a valid numeric value for field '{}', got: {}", name, value));

        if let Some(existing) = self.columns.get(name) {
            if existing.update_op == UpdateOp::Set || existing.update_op == UpdateOp::Add {
                let existing_decimal = BigDecimal::from_str(&existing.value)
                    .expect("existing value should be valid BigDecimal");
                let result = existing_decimal + new_decimal.clone();
                // Keep existing op: Set stays Set (full value), Add stays Add (delta)
                self.columns.insert(name.to_string(), FieldValue::with_op(result.to_string(), existing.update_op));
                return;
            }
        }

        self.columns.insert(name.to_string(), FieldValue::with_op(new_decimal.to_string(), UpdateOp::Add));
    }

    /// Set to the maximum of existing and new: column = GREATEST(COALESCE(column, new_value), new_value)
    /// Used with upsert_row() for tracking high values.
    /// Can only follow set() or another max() call on the same field.
    pub fn max<T: ToDatabaseValue>(&mut self, name: &str, value: T) -> &mut Self {
        use std::str::FromStr;
        if self.operation == Operation::Delete {
            panic!("cannot set fields on a delete operation")
        }
        let new_value = value.to_value();
        let new_decimal = BigDecimal::from_str(&new_value)
            .unwrap_or_else(|_| panic!("max() requires a valid numeric value for field '{}', got: {}", name, new_value));
        // Check for invalid transitions and compute maximum if there's an existing value
        if let Some(existing) = self.columns.get(name) {
            match existing.update_op {
                UpdateOp::Set | UpdateOp::Max => {
                    // Compute the maximum of existing and new values
                    let existing_decimal = BigDecimal::from_str(&existing.value)
                        .expect("existing value should be valid BigDecimal");
                    let max_val = if new_decimal > existing_decimal { new_value } else { existing.value.clone() };
                    self.columns.insert(name.to_string(), FieldValue::with_op(max_val, UpdateOp::Max));
                    return self;
                }
                UpdateOp::Add => panic!(
                    "cannot call max() on field '{}' after add/sub() - incompatible operations",
                    name
                ),
                UpdateOp::Min => panic!(
                    "cannot call max() on field '{}' after min() - incompatible operations",
                    name
                ),
                UpdateOp::SetIfNull => panic!(
                    "cannot call max() on field '{}' after set_if_null() - incompatible operations",
                    name
                ),
            }
        }
        self.columns.insert(name.to_string(), FieldValue::with_op(new_value, UpdateOp::Max));
        self
    }

    /// Set to the minimum of existing and new: column = LEAST(COALESCE(column, new_value), new_value)
    /// Used with upsert_row() for tracking low values.
    /// Can only follow set() or another min() call on the same field.
    pub fn min<T: ToDatabaseValue>(&mut self, name: &str, value: T) -> &mut Self {
        use std::str::FromStr;
        if self.operation == Operation::Delete {
            panic!("cannot set fields on a delete operation")
        }
        let new_value = value.to_value();
        let new_decimal = BigDecimal::from_str(&new_value)
            .unwrap_or_else(|_| panic!("min() requires a valid numeric value for field '{}', got: {}", name, new_value));
        // Check for invalid transitions and compute minimum if there's an existing value
        if let Some(existing) = self.columns.get(name) {
            match existing.update_op {
                UpdateOp::Set | UpdateOp::Min => {
                    // Compute the minimum of existing and new values
                    let existing_decimal = BigDecimal::from_str(&existing.value)
                        .expect("existing value should be valid BigDecimal");
                    let min_val = if new_decimal < existing_decimal { new_value } else { existing.value.clone() };
                    self.columns.insert(name.to_string(), FieldValue::with_op(min_val, UpdateOp::Min));
                    return self;
                }
                UpdateOp::Add => panic!(
                    "cannot call min() on field '{}' after add/sub() - incompatible operations",
                    name
                ),
                UpdateOp::Max => panic!(
                    "cannot call min() on field '{}' after max() - incompatible operations",
                    name
                ),
                UpdateOp::SetIfNull => panic!(
                    "cannot call min() on field '{}' after set_if_null() - incompatible operations",
                    name
                ),
            }
        }
        self.columns.insert(name.to_string(), FieldValue::with_op(new_value, UpdateOp::Min));
        self
    }

    /// Set only if column is null: column = COALESCE(column, new_value)
    /// Used with upsert_row() for setting initial values that should not be overwritten.
    /// When called multiple times, the FIRST value is kept (subsequent calls are no-ops).
    /// Cannot be mixed with other operations on the same field.
    pub fn set_if_null<T: ToDatabaseValue>(&mut self, name: &str, value: T) -> &mut Self {
        if self.operation == Operation::Delete {
            panic!("cannot set fields on a delete operation")
        }
        // Check for invalid transitions
        if let Some(existing) = self.columns.get(name) {
            match existing.update_op {
                UpdateOp::SetIfNull => return self, // Keep first value - subsequent calls are no-op
                UpdateOp::Set => panic!(
                    "cannot call set_if_null() on field '{}' after set() - incompatible operations",
                    name
                ),
                UpdateOp::Add => panic!(
                    "cannot call set_if_null() on field '{}' after add/sub() - incompatible operations",
                    name
                ),
                UpdateOp::Max => panic!(
                    "cannot call set_if_null() on field '{}' after max() - incompatible operations",
                    name
                ),
                UpdateOp::Min => panic!(
                    "cannot call set_if_null() on field '{}' after min() - incompatible operations",
                    name
                ),
            }
        }
        self.columns.insert(name.to_string(), FieldValue::with_op(value.to_value(), UpdateOp::SetIfNull));
        self
    }

    /// Set a field to a raw value, this is useful for setting values that are not
    /// normalized across all databases. In there, you can put the raw value as you
    /// would in a SQL statement of the database you are targeting.
    ///
    /// This will be pass as a string to the database which will interpret it itself.
    pub fn set_raw(&mut self, name: &str, value: String) -> &mut Self {
        self.columns.insert(name.to_string(), FieldValue::new(value));
        self
    }

    /// Set a field to an array of values compatible with PostgresSQL database,
    /// this method is currently experimental and hidden as we plan to support
    /// array natively in the model.
    ///
    /// For now, this method should be used with great care as it ties the model
    /// to the database implementation.
    #[doc(hidden)]
    pub fn set_psql_array<T: ToDatabaseValue>(&mut self, name: &str, value: Vec<T>) -> &mut Row {
        if self.operation == Operation::Delete {
            panic!("cannot set fields on a delete operation")
        }

        let values = value
            .into_iter()
            .map(|x| x.to_value())
            .collect::<Vec<_>>()
            .join(",");

        self.columns
            .insert(name.to_string(), FieldValue::new(format!("'{{{}}}'", values)));
        self
    }

    /// Set a field to an array of values compatible with Clickhouse database,
    /// this method is currently experimental and hidden as we plan to support
    /// array natively in the model.
    ///
    /// For now, this method should be used with great care as it ties the model
    /// to the database implementation.
    #[doc(hidden)]
    pub fn set_clickhouse_array<T: ToDatabaseValue>(
        &mut self,
        name: &str,
        value: Vec<T>,
    ) -> &mut Row {
        if self.operation == Operation::Delete {
            panic!("cannot set fields on a delete operation")
        }

        let values = value
            .into_iter()
            .map(|x| x.to_value())
            .collect::<Vec<_>>()
            .join(",");

        self.columns
            .insert(name.to_string(), FieldValue::new(format!("[{}]", values)));
        self
    }
}

macro_rules! impl_to_database_value_proxy_to_ref {
    ($name:ty) => {
        impl ToDatabaseValue for $name {
            fn to_value(self) -> String {
                ToDatabaseValue::to_value(&self)
            }
        }
    };
}

macro_rules! impl_to_database_value_proxy_to_string {
    ($name:ty) => {
        impl ToDatabaseValue for $name {
            fn to_value(self) -> String {
                ToString::to_string(&self)
            }
        }
    };
}

pub trait ToDatabaseValue {
    fn to_value(self) -> String;
}

impl_to_database_value_proxy_to_string!(i8);
impl_to_database_value_proxy_to_string!(i16);
impl_to_database_value_proxy_to_string!(i32);
impl_to_database_value_proxy_to_string!(i64);
impl_to_database_value_proxy_to_string!(u8);
impl_to_database_value_proxy_to_string!(u16);
impl_to_database_value_proxy_to_string!(u32);
impl_to_database_value_proxy_to_string!(u64);
impl_to_database_value_proxy_to_string!(bool);
impl_to_database_value_proxy_to_string!(::prost_types::Timestamp);
impl_to_database_value_proxy_to_string!(&::prost_types::Timestamp);
impl_to_database_value_proxy_to_string!(&str);
impl_to_database_value_proxy_to_string!(BigDecimal);
impl_to_database_value_proxy_to_string!(&BigDecimal);
impl_to_database_value_proxy_to_string!(BigInt);
impl_to_database_value_proxy_to_string!(&BigInt);

impl_to_database_value_proxy_to_ref!(Vec<u8>);

impl ToDatabaseValue for &String {
    fn to_value(self) -> String {
        self.clone()
    }
}

impl ToDatabaseValue for String {
    fn to_value(self) -> String {
        self
    }
}

impl ToDatabaseValue for &Vec<u8> {
    fn to_value(self) -> String {
        Hex::encode(self)
    }
}

impl<T: AsRef<[u8]>> ToDatabaseValue for Hex<T> {
    fn to_value(self) -> String {
        ToString::to_string(&self)
    }
}

impl<T: AsRef<[u8]>> ToDatabaseValue for &Hex<T> {
    fn to_value(self) -> String {
        ToString::to_string(self)
    }
}

#[cfg(test)]
mod test {
    use crate::pb::database::table_change::PrimaryKey as PrimaryKeyProto;
    use crate::pb::database::CompositePrimaryKey as CompositePrimaryKeyProto;
    use crate::pb::database::{DatabaseChanges, TableChange};
    use crate::tables::PrimaryKey;
    use crate::tables::Tables;
    use crate::tables::ToDatabaseValue;
    use pretty_assertions::assert_eq;

    #[test]
    fn to_database_value_proto_timestamp() {
        assert_eq!(
            ToDatabaseValue::to_value(::prost_types::Timestamp {
                seconds: 60 * 60 + 60 + 1,
                nanos: 1
            }),
            "1970-01-01T01:01:01.000000001Z"
        );
    }

    #[test]
    fn create_row_single_pk_direct() {
        let mut tables = Tables::new();
        tables.create_row("myevent", PrimaryKey::Single("myhash".to_string()));

        assert_eq!(
            tables.to_database_changes(),
            DatabaseChanges {
                table_changes: [change("myevent", "myhash", 0)].to_vec(),
            }
        );
    }

    #[test]
    fn create_row_single_pk() {
        let mut tables = Tables::new();
        tables.create_row("myevent", "myhash");

        assert_eq!(
            tables.to_database_changes(),
            DatabaseChanges {
                table_changes: [change("myevent", "myhash", 0)].to_vec(),
            }
        );
    }

    #[test]
    fn create_row_composite_pk() {
        let mut tables = Tables::new();
        tables.create_row(
            "myevent",
            [("evt_tx_hash", "hello"), ("evt_index", "world")],
        );

        assert_eq!(
            tables.to_database_changes(),
            DatabaseChanges {
                table_changes: [change(
                    "myevent",
                    [("evt_tx_hash", "hello"), ("evt_index", "world")],
                    0
                )]
                .to_vec()
            }
        );
    }

    #[test]
    fn row_ordering() {
        let mut tables = Tables::new();
        tables.create_row("tableA", "one");
        tables.create_row("tableC", "two");
        tables.create_row("tableA", "three");
        tables.create_row("tableD", "four");
        tables.create_row("tableE", "five");
        tables.create_row("tableC", "six");

        assert_eq!(
            tables.to_database_changes(),
            DatabaseChanges {
                table_changes: [
                    change("tableA", "one", 0),
                    change("tableC", "two", 1),
                    change("tableA", "three", 2),
                    change("tableD", "four", 3),
                    change("tableE", "five", 4),
                    change("tableC", "six", 5)
                ]
                .to_vec(),
            }
        );
    }

    fn change<K: Into<PrimaryKey>>(name: &str, key: K, ordinal: u64) -> TableChange {
        TableChange {
            table: name.to_string(),
            ordinal,
            operation: 1,
            fields: [].into(),
            primary_key: Some(match key.into() {
                PrimaryKey::Single(pk) => PrimaryKeyProto::Pk(pk),
                PrimaryKey::Composite(keys) => {
                    PrimaryKeyProto::CompositePk(CompositePrimaryKeyProto {
                        keys: keys.into_iter().collect(),
                    })
                }
            }),
        }
    }
}

#[cfg(test)]
mod update_op_tests {
    use super::*;
    use crate::pb::database::field::UpdateOp;

    // ============================================================
    // Basic set() operation tests
    // ============================================================

    #[test]
    fn set_stores_value_with_set_op() {
        let mut tables = Tables::new();
        let row = tables.create_row("test", "pk1");
        row.set("balance", "1000");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "1000");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    #[test]
    fn set_overwrites_previous_set() {
        let mut tables = Tables::new();
        let row = tables.create_row("test", "pk1");
        row.set("balance", "1000");
        row.set("balance", "2000");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "2000");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    // ============================================================
    // Basic add() operation tests
    // ============================================================

    #[test]
    fn add_alone_stores_with_add_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("balance", "100");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "100");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    #[test]
    fn add_alone_with_bigdecimal() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("balance", "123.456789");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "123.456789");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    // ============================================================
    // Basic sub() operation tests
    // ============================================================

    #[test]
    fn sub_alone_stores_negated_with_add_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.sub("balance", "100");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "-100");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    #[test]
    fn sub_negates_negative_to_positive() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.sub("balance", "-100");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "100");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    // ============================================================
    // ADD + ADD accumulation tests
    // ============================================================

    #[test]
    fn add_plus_add_accumulates_keeps_add_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("balance", "100");
        row.add("balance", "50");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "150");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    #[test]
    fn add_plus_sub_accumulates_keeps_add_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("balance", "100");
        row.sub("balance", "30");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "70");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    #[test]
    fn sub_plus_add_accumulates_keeps_add_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.sub("balance", "100");
        row.add("balance", "30");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "-70");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    #[test]
    fn sub_plus_sub_accumulates_keeps_add_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.sub("balance", "100");
        row.sub("balance", "50");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "-150");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    // ============================================================
    // SET + ADD/SUB accumulation tests (critical for token creation + burn)
    // ============================================================

    #[test]
    fn set_plus_add_accumulates_keeps_set_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("total_supply", "1000000000");
        row.add("total_supply", "500");

        let field = row.columns.get("total_supply").unwrap();
        assert_eq!(field.value, "1000000500");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    #[test]
    fn set_plus_sub_accumulates_keeps_set_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("total_supply", "1000000000");
        row.sub("total_supply", "500");

        let field = row.columns.get("total_supply").unwrap();
        assert_eq!(field.value, "999999500");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    #[test]
    fn set_plus_multiple_adds_accumulates_keeps_set_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("total_supply", "1000000000");
        row.add("total_supply", "100");
        row.add("total_supply", "200");
        row.sub("total_supply", "50");

        let field = row.columns.get("total_supply").unwrap();
        assert_eq!(field.value, "1000000250");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    // ============================================================
    // Disallowed transitions - set() cannot follow other ops
    // ============================================================

    #[test]
    #[should_panic(expected = "cannot call set() on field 'balance' after add/sub()")]
    fn add_then_set_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("balance", "100");
        row.set("balance", "999"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call set() on field 'price' after max()")]
    fn max_then_set_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("price", "100");
        row.set("price", "999"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call set() on field 'price' after min()")]
    fn min_then_set_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("price", "100");
        row.set("price", "999"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call set() on field 'created' after set_if_null()")]
    fn set_if_null_then_set_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set_if_null("created", "2024-01-01");
        row.set("created", "2024-02-01"); // Should panic
    }

    // ============================================================
    // Cross-operation incompatibility tests
    // ============================================================

    #[test]
    #[should_panic(expected = "cannot call add/sub() on field 'x' after max()")]
    fn max_then_add_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("x", "100");
        row.add("x", "50"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call add/sub() on field 'x' after min()")]
    fn min_then_add_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("x", "100");
        row.add("x", "50"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call add/sub() on field 'x' after set_if_null()")]
    fn set_if_null_then_add_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set_if_null("x", "100");
        row.add("x", "50"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call max() on field 'x' after add/sub()")]
    fn add_then_max_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("x", "100");
        row.max("x", "50"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call max() on field 'x' after min()")]
    fn min_then_max_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("x", "100");
        row.max("x", "50"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call min() on field 'x' after add/sub()")]
    fn add_then_min_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("x", "100");
        row.min("x", "50"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call min() on field 'x' after max()")]
    fn max_then_min_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("x", "100");
        row.min("x", "50"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call set_if_null() on field 'x' after add/sub()")]
    fn add_then_set_if_null_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("x", "100");
        row.set_if_null("x", "50"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call set_if_null() on field 'x' after max()")]
    fn max_then_set_if_null_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("x", "100");
        row.set_if_null("x", "50"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call set_if_null() on field 'x' after min()")]
    fn min_then_set_if_null_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("x", "100");
        row.set_if_null("x", "50"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call max() on field 'x' after set_if_null()")]
    fn set_if_null_then_max_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set_if_null("x", "100");
        row.max("x", "50"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call min() on field 'x' after set_if_null()")]
    fn set_if_null_then_min_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set_if_null("x", "100");
        row.min("x", "50"); // Should panic
    }

    // ============================================================
    // Valid set -> other op transitions
    // ============================================================

    #[test]
    fn set_then_max_computes_maximum() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("price", "100");
        row.max("price", "50");

        // max() computes max(100, 50) = 100
        let field = row.columns.get("price").unwrap();
        assert_eq!(field.value, "100");
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    #[test]
    fn set_then_max_updates_when_new_is_greater() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("price", "50");
        row.max("price", "100");

        // max() computes max(50, 100) = 100
        let field = row.columns.get("price").unwrap();
        assert_eq!(field.value, "100");
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    #[test]
    fn set_then_min_computes_minimum() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("price", "100");
        row.min("price", "50");

        // min() computes min(100, 50) = 50
        let field = row.columns.get("price").unwrap();
        assert_eq!(field.value, "50");
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    #[test]
    fn set_then_min_keeps_existing_when_smaller() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("price", "50");
        row.min("price", "100");

        // min() computes min(50, 100) = 50
        let field = row.columns.get("price").unwrap();
        assert_eq!(field.value, "50");
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    #[test]
    #[should_panic(expected = "cannot call set_if_null() on field 'created' after set()")]
    fn set_then_set_if_null_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("created", "2024-01-01");
        row.set_if_null("created", "2024-02-01"); // Should panic
    }

    // ============================================================
    // Multiple columns independence tests
    // ============================================================

    #[test]
    fn multiple_columns_independent() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("balance", "1000");
        row.add("tx_count", "1");
        row.sub("balance", "100");
        row.add("tx_count", "1");

        let balance = row.columns.get("balance").unwrap();
        assert_eq!(balance.value, "900");
        assert_eq!(balance.update_op, UpdateOp::Set);

        let tx_count = row.columns.get("tx_count").unwrap();
        assert_eq!(tx_count.value, "2");
        assert_eq!(tx_count.update_op, UpdateOp::Add);
    }

    // ============================================================
    // MAX operation tests
    // ============================================================

    #[test]
    fn max_stores_with_max_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("high_price", "100");

        let field = row.columns.get("high_price").unwrap();
        assert_eq!(field.value, "100");
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    #[test]
    fn max_computes_maximum_value() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("high_price", "100");
        row.max("high_price", "50");

        // max() now computes the actual maximum in database-changes
        let field = row.columns.get("high_price").unwrap();
        assert_eq!(field.value, "100"); // Keeps 100 since it's greater than 50
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    #[test]
    fn max_updates_when_new_value_is_greater() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("high_price", "50");
        row.max("high_price", "100");

        let field = row.columns.get("high_price").unwrap();
        assert_eq!(field.value, "100"); // Updates to 100 since it's greater than 50
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    // ============================================================
    // MIN operation tests
    // ============================================================

    #[test]
    fn min_stores_with_min_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("low_price", "100");

        let field = row.columns.get("low_price").unwrap();
        assert_eq!(field.value, "100");
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    #[test]
    fn min_computes_minimum_value() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("low_price", "50");
        row.min("low_price", "100");

        // min() now computes the actual minimum in database-changes
        let field = row.columns.get("low_price").unwrap();
        assert_eq!(field.value, "50"); // Keeps 50 since it's less than 100
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    #[test]
    fn min_updates_when_new_value_is_smaller() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("low_price", "100");
        row.min("low_price", "50");

        let field = row.columns.get("low_price").unwrap();
        assert_eq!(field.value, "50"); // Updates to 50 since it's less than 100
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    // ============================================================
    // SET_IF_NULL operation tests
    // ============================================================

    #[test]
    fn set_if_null_stores_with_set_if_null_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set_if_null("created_at", "2024-01-01");

        let field = row.columns.get("created_at").unwrap();
        assert_eq!(field.value, "2024-01-01");
        assert_eq!(field.update_op, UpdateOp::SetIfNull);
    }

    #[test]
    fn set_if_null_keeps_first_value() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set_if_null("created_at", "2024-01-01");
        row.set_if_null("created_at", "2024-02-01");

        // set_if_null keeps the first value - subsequent calls are no-ops
        let field = row.columns.get("created_at").unwrap();
        assert_eq!(field.value, "2024-01-01");
        assert_eq!(field.update_op, UpdateOp::SetIfNull);
    }

    // ============================================================
    // BigDecimal precision tests
    // ============================================================

    #[test]
    fn bigdecimal_precision_preserved() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("amount", "1234567890123456789.123456789");
        row.add("amount", "0.000000001");

        let field = row.columns.get("amount").unwrap();
        assert_eq!(field.value, "1234567890123456789.123456790");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    #[test]
    fn multiple_adds_preserve_precision() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("amount", "0.1");
        row.add("amount", "0.1");
        row.add("amount", "0.1");

        let field = row.columns.get("amount").unwrap();
        assert_eq!(field.value, "0.3");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    // ============================================================
    // Edge cases
    // ============================================================

    #[test]
    fn add_with_zero() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("balance", "100");
        row.add("balance", "0");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "100");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    #[test]
    fn set_then_add_zero() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("balance", "100");
        row.add("balance", "0");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "100");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    #[test]
    fn add_resulting_in_zero() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("balance", "100");
        row.sub("balance", "100");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "0");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    #[test]
    fn set_then_sub_to_zero() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("balance", "100");
        row.sub("balance", "100");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "0");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    #[test]
    fn negative_result_from_accumulation() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("balance", "100");
        row.sub("balance", "200");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value, "-100");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    // ============================================================
    // Non-numeric value edge cases
    // ============================================================

    #[test]
    #[should_panic(expected = "add/sub() requires a valid numeric value")]
    fn add_non_numeric_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("name", "hello"); // Should panic
    }

    // ============================================================
    // Integration tests with to_database_changes
    // ============================================================

    #[test]
    fn to_database_changes_includes_update_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("tokens", "0xtoken");
        row.set("name", "MyToken");
        row.add("balance", "100");

        let changes = tables.to_database_changes();
        assert_eq!(changes.table_changes.len(), 1);

        let change = &changes.table_changes[0];
        assert_eq!(change.fields.len(), 2);

        // Find balance field
        let balance_field = change.fields.iter().find(|f| f.name == "balance").unwrap();
        assert_eq!(balance_field.new_value, "100");
        assert_eq!(balance_field.update_op, UpdateOp::Add as i32);

        // Find name field
        let name_field = change.fields.iter().find(|f| f.name == "name").unwrap();
        assert_eq!(name_field.new_value, "MyToken");
        assert_eq!(name_field.update_op, UpdateOp::Set as i32);
    }

}
