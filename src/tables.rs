use crate::numeric::{NumericAddable, NumericComparable};
use crate::pb::sf::substreams::sink::database::v1::{
    field::UpdateOp, table_change::Operation, DatabaseChanges, Field, TableChange,
};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use substreams::{
    scalar::{BigDecimal, BigInt},
    Hex,
};

#[derive(Debug)]
pub struct Tables {
    /// Map from table name to the primary keys within that table
    tables: HashMap<String, Rows>,

    /// Ordinal is used to track the order of changes, it is incremented for each row
    /// in such way that at the end, we can correctly order the changes back correctly.
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
        let row = rows.pks.entry(k).or_insert(Row::new(self.ordinal.next()));
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
        let row = rows.pks.entry(k).or_insert(Row::new(self.ordinal.next()));
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
        let row = rows.pks.entry(k).or_insert(Row::new(self.ordinal.next()));
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
            .or_insert(Row::new(self.ordinal.next()));

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
                        value: field_value.value.into_string(),
                        update_op: field_value.update_op.into(),
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
struct Ordinal(u64);

impl Ordinal {
    fn new() -> Self {
        Ordinal(0)
    }

    fn next(&mut self) -> u64 {
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
struct Rows {
    // Map of primary keys within this table, to the fields within
    pks: HashMap<PrimaryKey, Row>,
}

impl Rows {
    fn new() -> Self {
        Rows {
            pks: HashMap::new(),
        }
    }
}

/// Holds field value and its update operation for UPSERT handling.
#[derive(Debug, Clone)]
struct FieldValue {
    value: FieldInnerValue,
    update_op: UpdateOp,
}

impl FieldValue {
    fn new(value: String) -> Self {
        FieldValue {
            value: FieldInnerValue::from_string(value),
            update_op: UpdateOp::Set,
        }
    }

    fn new_numeric(value: BigDecimal, op: UpdateOp) -> Self {
        FieldValue {
            value: FieldInnerValue::Numeric(value),
            update_op: op,
        }
    }

    fn with_op(value: String, update_op: UpdateOp) -> Self {
        FieldValue {
            value: FieldInnerValue::from_string(value),
            update_op,
        }
    }
}

/// Internal representation of field values that can be either text or numeric.
/// Numeric values are stored as BigDecimal to avoid repeated parsing during
/// accumulation operations (add/sub/max/min).
#[derive(Debug, Clone)]
enum FieldInnerValue {
    /// Text representation - used for non-numeric values and initial set() calls
    Text(String),
    /// Numeric representation - used after first numeric operation to avoid re-parsing
    Numeric(BigDecimal),
}

impl FieldInnerValue {
    /// Create from a string value
    fn from_string(s: String) -> Self {
        FieldInnerValue::Text(s)
    }

    #[cfg(test)]
    fn as_string(&self) -> String {
        match self {
            FieldInnerValue::Text(s) => s.clone(),
            FieldInnerValue::Numeric(bd) => bd.to_string(),
        }
    }

    /// Convert to string for database output
    fn into_string(self) -> String {
        match self {
            FieldInnerValue::Text(s) => s,
            FieldInnerValue::Numeric(bd) => bd.to_string(),
        }
    }

    /// Get or convert to BigDecimal for numeric operations
    /// Returns a mutable reference to the internal BigDecimal,
    /// converting from Text if necessary
    fn as_numeric_mut(&mut self) -> &mut BigDecimal {
        use std::str::FromStr;

        // If currently Text, parse and convert to Numeric
        if let FieldInnerValue::Text(s) = self {
            let bd = BigDecimal::from_str(s).expect("existing value should be valid BigDecimal");
            *self = FieldInnerValue::Numeric(bd);
        }

        match self {
            FieldInnerValue::Numeric(bd) => bd,
            FieldInnerValue::Text(_) => unreachable!("already converted to Numeric, impossible"),
        }
    }
}

#[derive(Debug, Default)]
pub struct Row {
    /// Verify that we don't try to delete the same row as we're creating it
    operation: Operation,
    /// Map of field name to its value and update operation
    columns: HashMap<String, FieldValue>,
    /// First ordinal this row was created/modified at, for sorting
    ordinal: u64,
}

impl Row {
    fn new(ordinal: u64) -> Self {
        Row {
            operation: Operation::Unspecified,
            columns: HashMap::new(),
            ordinal,
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
        // set() can always be called - it resets the field to a new value
        // This allows sequences like: set -> add -> set (resets the accumulated value)
        self.columns
            .insert(name.to_string(), FieldValue::new(value.to_value()));
        self
    }

    /// Add to the existing value: column = COALESCE(column, 0) + value
    /// Used with upsert_row() for accumulating values like counters or balances.
    /// If called multiple times for the same column within a block, values are accumulated.
    pub fn add<T: NumericAddable>(&mut self, name: &str, value: T) -> &mut Self {
        if self.operation == Operation::Delete {
            panic!("cannot set fields on a delete operation")
        }
        self.accumulate_add(name, value, false);
        self
    }

    /// Subtract from the existing value: column = COALESCE(column, 0) - value
    /// Convenience method that negates the value and uses ADD operation.
    /// If called multiple times for the same column within a block, values are accumulated.
    pub fn sub<T: NumericAddable>(&mut self, name: &str, value: T) -> &mut Self {
        if self.operation == Operation::Delete {
            panic!("cannot set fields on a delete operation")
        }
        self.accumulate_add(name, value, true);
        self
    }

    /// Internal helper to accumulate ADD values for the same column within a block.
    /// - Set + Add/Sub: accumulate values, keep Set op (full value for INSERT)
    /// - Add + Add/Sub: accumulate values, keep Add op (delta for UPDATE)
    /// - No existing: store as Add op (delta)
    /// - Other existing ops (Max/Min/SetIfNull): PANIC (invalid transition)
    fn accumulate_add<T: NumericAddable>(&mut self, name: &str, value: T, subtract: bool) {
        match self.columns.get_mut(name) {
            Some(existing) => {
                match existing.update_op {
                    UpdateOp::Unspecified => panic!(
                        "cannot call add/sub() on field '{}' after unspecified - incompatible operations",
                        name
                    ),
                    UpdateOp::Set | UpdateOp::Add => {
                        // Get or convert to BigDecimal (only parses once!)
                        let target = existing.value.as_numeric_mut();

                        if subtract {
                            value.sub_assign_from(target);
                        } else {
                            value.add_assign_to(target);
                        }
                        // No .to_string() call - stays as BigDecimal!
                    }
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
            None => {
                let mut target = value.to_big_decimal();

                if subtract {
                    target = -target;
                }

                // Store directly as Numeric
                self.columns.insert(
                    name.to_string(),
                    FieldValue::new_numeric(target, UpdateOp::Add),
                );
            }
        }
    }

    /// Set to the maximum of existing and new: column = GREATEST(COALESCE(column, value), value)
    /// Used with upsert_row() for tracking high values.
    /// Can only follow set() or another max() call on the same field.
    pub fn max<T: NumericComparable>(&mut self, name: &str, value: T) -> &mut Self {
        if self.operation == Operation::Delete {
            panic!("cannot set fields on a delete operation")
        }

        // Use get_mut to check, validate, and update in one pass
        match self.columns.get_mut(name) {
            Some(existing) => {
                match existing.update_op {
                    UpdateOp::Unspecified => panic!(
                        "cannot call max() on field '{}' after unspecified - incompatible operations",
                        name
                    ),
                    UpdateOp::Set | UpdateOp::Max => {
                        // Get or convert to BigDecimal
                        let target = existing.value.as_numeric_mut();

                        if value.cmp_to_big_decimal(target) == Ordering::Greater {
                            *target = value.to_big_decimal();
                        }
                        existing.update_op = UpdateOp::Max;
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
            None => {
                // No existing value - insert new entry with Max operation
                self.columns.insert(
                    name.to_string(),
                    FieldValue::new_numeric(value.to_big_decimal(), UpdateOp::Max),
                );
            }
        }
        self
    }

    /// Set to the minimum of existing and new: column = LEAST(COALESCE(column, value), value)
    /// Used with upsert_row() for tracking low values.
    /// Can only follow set() or another min() call on the same field.
    pub fn min<T: NumericComparable>(&mut self, name: &str, value: T) -> &mut Self {
        if self.operation == Operation::Delete {
            panic!("cannot set fields on a delete operation")
        }

        // Use get_mut to check, validate, and update in one pass
        match self.columns.get_mut(name) {
            Some(existing) => {
                match existing.update_op {
                    UpdateOp::Unspecified => panic!(
                        "cannot call min() on field '{}' after unspecified - incompatible operations",
                        name
                    ),
                    UpdateOp::Set | UpdateOp::Min => {
                        // Get or convert to BigDecimal
                        let target = existing.value.as_numeric_mut();
                        if value.cmp_to_big_decimal(target) == Ordering::Less {
                            *target = value.to_big_decimal();
                        }
                        existing.update_op = UpdateOp::Min;
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
            None => {
                // No existing value - insert new entry with Min operation
                self.columns.insert(
                    name.to_string(),
                    FieldValue::new_numeric(value.to_big_decimal(), UpdateOp::Min),
                );
            }
        }
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

        // Parse the value first
        let new_value = value.to_value();

        // Use get_mut to check for existing value
        match self.columns.get_mut(name) {
            Some(existing) => {
                match existing.update_op {
                    UpdateOp::Unspecified => panic!(
                        "cannot call set_if_null() on field '{}' after unspecified - incompatible operations",
                        name
                    ),
                    UpdateOp::Set | UpdateOp::SetIfNull => {
                        // After set() or set_if_null(), the value is already determined
                        // Keep the existing value - subsequent set_if_null() calls are no-op
                    }
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
            None => {
                // No existing value - insert new entry with SetIfNull operation
                self.columns.insert(
                    name.to_string(),
                    FieldValue::with_op(new_value, UpdateOp::SetIfNull),
                );
            }
        }
        self
    }

    /// Set a field to a raw value, this is useful for setting values that are not
    /// normalized across all databases. In there, you can put the raw value as you
    /// would in a SQL statement of the database you are targeting.
    ///
    /// This will be pass as a string to the database which will interpret it itself.
    pub fn set_raw(&mut self, name: &str, value: String) -> &mut Self {
        self.columns
            .insert(name.to_string(), FieldValue::new(value));
        self
    }

    /// Set a field to an array of values compatible with PostgresSQL database,
    /// this method is currently experimental and hidden as we plan to support
    /// array natively in the model.
    ///
    /// For now, this method should be used with great care as it ties the model
    /// to the database implementation.
    pub fn set_psql_array<T: ToDatabaseValue>(&mut self, name: &str, value: Vec<T>) -> &mut Row {
        if self.operation == Operation::Delete {
            panic!("cannot set fields on a delete operation")
        }

        let values = value
            .into_iter()
            .map(|x| x.to_value())
            .collect::<Vec<_>>()
            .join(",");

        self.columns.insert(
            name.to_string(),
            FieldValue::new(format!("'{{{}}}'", values)),
        );
        self
    }

    /// Set a field to an array of values compatible with Clickhouse database,
    /// this method is currently experimental and hidden as we plan to support
    /// array natively in the model.
    ///
    /// For now, this method should be used with great care as it ties the model
    /// to the database implementation.
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
impl ToDatabaseValue for ::buffa_types::google::protobuf::Timestamp {
    fn to_value(self) -> String {
        crate::change::timestamp_to_string(&self)
    }
}

impl ToDatabaseValue for &::buffa_types::google::protobuf::Timestamp {
    fn to_value(self) -> String {
        crate::change::timestamp_to_string(self)
    }
}
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
    use crate::pb::sf::substreams::sink::database::v1::table_change::PrimaryKey as PrimaryKeyProto;
    use crate::pb::sf::substreams::sink::database::v1::CompositePrimaryKey as CompositePrimaryKeyProto;
    use crate::pb::sf::substreams::sink::database::v1::{DatabaseChanges, TableChange};
    use crate::tables::PrimaryKey;
    use crate::tables::Tables;
    use crate::tables::ToDatabaseValue;
    use pretty_assertions::assert_eq;

    #[test]
    fn to_database_value_proto_timestamp() {
        assert_eq!(
            ToDatabaseValue::to_value(::buffa_types::google::protobuf::Timestamp {
                seconds: 60 * 60 + 60 + 1,
                nanos: 1,
                ..Default::default()
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
            operation: crate::pb::sf::substreams::sink::database::v1::table_change::Operation::OPERATION_CREATE.into(),
            fields: [].into(),
            primary_key: Some(match key.into() {
                PrimaryKey::Single(pk) => PrimaryKeyProto::Pk(pk),
                PrimaryKey::Composite(keys) => {
                    PrimaryKeyProto::CompositePk(Box::new(CompositePrimaryKeyProto {
                        keys: keys.into_iter().collect(),
                    }))
                }
            }),
        }
    }
}

#[cfg(test)]
mod update_op_tests {
    use super::*;
    use crate::pb::sf::substreams::sink::database::v1::field::UpdateOp;

    // ============================================================
    // Basic set() operation tests
    // ============================================================

    #[test]
    fn set_stores_value_with_set_op() {
        let mut tables = Tables::new();
        let row = tables.create_row("test", "pk1");
        row.set("balance", "1000");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value.as_string(), "1000");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    #[test]
    fn set_overwrites_previous_set() {
        let mut tables = Tables::new();
        let row = tables.create_row("test", "pk1");
        row.set("balance", "1000");
        row.set("balance", "2000");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value.as_string(), "2000");
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
        assert_eq!(field.value.as_string(), "100");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    #[test]
    fn add_alone_with_bigdecimal() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("balance", "123.456789");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value.as_string(), "123.456789");
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
        assert_eq!(field.value.as_string(), "-100");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    #[test]
    fn sub_negates_negative_to_positive() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.sub("balance", "-100");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value.as_string(), "100");
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
        assert_eq!(field.value.as_string(), "150");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    #[test]
    fn add_plus_sub_accumulates_keeps_add_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("balance", "100");
        row.sub("balance", "30");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value.as_string(), "70");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    #[test]
    fn sub_plus_add_accumulates_keeps_add_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.sub("balance", "100");
        row.add("balance", "30");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value.as_string(), "-70");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    #[test]
    fn sub_plus_sub_accumulates_keeps_add_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.sub("balance", "100");
        row.sub("balance", "50");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value.as_string(), "-150");
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
        assert_eq!(field.value.as_string(), "1000000500");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    #[test]
    fn set_plus_sub_accumulates_keeps_set_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("total_supply", "1000000000");
        row.sub("total_supply", "500");

        let field = row.columns.get("total_supply").unwrap();
        assert_eq!(field.value.as_string(), "999999500");
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
        assert_eq!(field.value.as_string(), "1000000250");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    // ============================================================
    // set() can always reset a field - allowed transitions from any op
    // ============================================================

    #[test]
    fn add_then_set_resets_value() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("balance", "100");
        row.set("balance", "999");

        // set() resets the field completely
        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value.as_string(), "999");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    #[test]
    fn max_then_set_resets_value() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("price", 100i64);
        row.set("price", "999");

        // set() resets the field completely
        let field = row.columns.get("price").unwrap();
        assert_eq!(field.value.as_string(), "999");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    #[test]
    fn min_then_set_resets_value() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("price", 100i64);
        row.set("price", "999");

        // set() resets the field completely
        let field = row.columns.get("price").unwrap();
        assert_eq!(field.value.as_string(), "999");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    #[test]
    fn set_if_null_then_set_resets_value() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set_if_null("created", "2024-01-01");
        row.set("created", "2024-02-01");

        // set() resets the field completely
        let field = row.columns.get("created").unwrap();
        assert_eq!(field.value.as_string(), "2024-02-01");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    // ============================================================
    // Cross-operation incompatibility tests
    // ============================================================

    #[test]
    #[should_panic(expected = "cannot call add/sub() on field 'x' after max()")]
    fn max_then_add_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("x", 100i64);
        row.add("x", "50"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call add/sub() on field 'x' after min()")]
    fn min_then_add_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("x", 100i64);
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
        row.max("x", 50i64); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call max() on field 'x' after min()")]
    fn min_then_max_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("x", 100i64);
        row.max("x", 50i64); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call min() on field 'x' after add/sub()")]
    fn add_then_min_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("x", "100");
        row.min("x", 50i64); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call min() on field 'x' after max()")]
    fn max_then_min_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("x", 100i64);
        row.min("x", 50i64); // Should panic
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
        row.max("x", 100i64);
        row.set_if_null("x", "50"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call set_if_null() on field 'x' after min()")]
    fn min_then_set_if_null_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("x", 100i64);
        row.set_if_null("x", "50"); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call max() on field 'x' after set_if_null()")]
    fn set_if_null_then_max_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set_if_null("x", "100");
        row.max("x", 50i64); // Should panic
    }

    #[test]
    #[should_panic(expected = "cannot call min() on field 'x' after set_if_null()")]
    fn set_if_null_then_min_panics() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set_if_null("x", "100");
        row.min("x", 50i64); // Should panic
    }

    // ============================================================
    // Valid set -> other op transitions
    // ============================================================

    #[test]
    fn set_then_max_computes_maximum() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("price", "100");
        row.max("price", 50i64);

        // max() computes max(100, 50) = 100
        let field = row.columns.get("price").unwrap();
        assert_eq!(field.value.as_string(), "100");
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    #[test]
    fn set_then_max_updates_when_new_is_greater() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("price", "50");
        row.max("price", 100i64);

        // max() computes max(50, 100) = 100
        let field = row.columns.get("price").unwrap();
        assert_eq!(field.value.as_string(), "100");
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    #[test]
    fn set_then_min_computes_minimum() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("price", "100");
        row.min("price", 50i64);

        // min() computes min(100, 50) = 50
        let field = row.columns.get("price").unwrap();
        assert_eq!(field.value.as_string(), "50");
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    #[test]
    fn set_then_min_keeps_existing_when_smaller() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("price", "50");
        row.min("price", 100i64);

        // min() computes min(50, 100) = 50
        let field = row.columns.get("price").unwrap();
        assert_eq!(field.value.as_string(), "50");
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    #[test]
    fn set_then_set_if_null_is_noop() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("created", "2024-01-01");
        row.set_if_null("created", "2024-02-01"); // Should be no-op

        // set() value is kept, set_if_null() is ignored
        let field = row.columns.get("created").unwrap();
        assert_eq!(field.value.as_string(), "2024-01-01");
        assert_eq!(field.update_op, UpdateOp::Set);
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
        assert_eq!(balance.value.as_string(), "900");
        assert_eq!(balance.update_op, UpdateOp::Set);

        let tx_count = row.columns.get("tx_count").unwrap();
        assert_eq!(tx_count.value.as_string(), "2");
        assert_eq!(tx_count.update_op, UpdateOp::Add);
    }

    // ============================================================
    // MAX operation tests
    // ============================================================

    #[test]
    fn max_stores_with_max_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("high_price", 100i64);

        let field = row.columns.get("high_price").unwrap();
        assert_eq!(field.value.as_string(), "100");
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    #[test]
    fn max_computes_maximum_value() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("high_price", 100i64);
        row.max("high_price", 50i64);

        // max() now computes the actual maximum in database-changes
        let field = row.columns.get("high_price").unwrap();
        assert_eq!(field.value.as_string(), "100"); // Keeps 100 since it's greater than 50
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    #[test]
    fn max_updates_when_value_is_greater() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("high_price", 50i64);
        row.max("high_price", 100i64);

        let field = row.columns.get("high_price").unwrap();
        assert_eq!(field.value.as_string(), "100"); // Updates to 100 since it's greater than 50
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    // ============================================================
    // MIN operation tests
    // ============================================================

    #[test]
    fn min_stores_with_min_op() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("low_price", 100i64);

        let field = row.columns.get("low_price").unwrap();
        assert_eq!(field.value.as_string(), "100");
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    #[test]
    fn min_computes_minimum_value() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("low_price", 50i64);
        row.min("low_price", 100i64);

        // min() now computes the actual minimum in database-changes
        let field = row.columns.get("low_price").unwrap();
        assert_eq!(field.value.as_string(), "50"); // Keeps 50 since it's less than 100
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    #[test]
    fn min_updates_when_value_is_smaller() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("low_price", 100i64);
        row.min("low_price", 50i64);

        let field = row.columns.get("low_price").unwrap();
        assert_eq!(field.value.as_string(), "50"); // Updates to 50 since it's less than 100
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
        assert_eq!(field.value.as_string(), "2024-01-01");
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
        assert_eq!(field.value.as_string(), "2024-01-01");
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
        assert_eq!(field.value.as_string(), "1234567890123456789.123456790");
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
        assert_eq!(field.value.as_string(), "0.3");
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
        assert_eq!(field.value.as_string(), "100");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    #[test]
    fn set_then_add_zero() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("balance", "100");
        row.add("balance", "0");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value.as_string(), "100");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    #[test]
    fn add_resulting_in_zero() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.add("balance", "100");
        row.sub("balance", "100");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value.as_string(), "0");
        assert_eq!(field.update_op, UpdateOp::Add);
    }

    #[test]
    fn set_then_sub_to_zero() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("balance", "100");
        row.sub("balance", "100");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value.as_string(), "0");
        assert_eq!(field.update_op, UpdateOp::Set);
    }

    #[test]
    fn negative_result_from_accumulation() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.set("balance", "100");
        row.sub("balance", "200");

        let field = row.columns.get("balance").unwrap();
        assert_eq!(field.value.as_string(), "-100");
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
        assert_eq!(balance_field.value, "100");
        assert_eq!(balance_field.update_op, UpdateOp::Add);

        // Find name field
        let name_field = change.fields.iter().find(|f| f.name == "name").unwrap();
        assert_eq!(name_field.value, "MyToken");
        assert_eq!(name_field.update_op, UpdateOp::Set);
    }

    // ============================================================
    // NumericComparable tests - verifying zero-allocation integer comparisons
    // ============================================================

    #[test]
    fn max_with_i64_computes_correctly() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("value", 100i64);
        row.max("value", 50i64);
        row.max("value", 200i64);

        let field = row.columns.get("value").unwrap();
        assert_eq!(field.value.as_string(), "200");
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    #[test]
    fn min_with_i64_computes_correctly() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("value", 100i64);
        row.min("value", 50i64);
        row.min("value", 200i64);

        let field = row.columns.get("value").unwrap();
        assert_eq!(field.value.as_string(), "50");
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    #[test]
    fn max_with_u32_computes_correctly() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("value", 100u32);
        row.max("value", 50u32);
        row.max("value", 200u32);

        let field = row.columns.get("value").unwrap();
        assert_eq!(field.value.as_string(), "200");
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    #[test]
    fn min_with_u32_computes_correctly() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("value", 100u32);
        row.min("value", 50u32);
        row.min("value", 200u32);

        let field = row.columns.get("value").unwrap();
        assert_eq!(field.value.as_string(), "50");
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    #[test]
    fn max_with_i32_computes_correctly() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("value", 100i32);
        row.max("value", -50i32);
        row.max("value", 200i32);

        let field = row.columns.get("value").unwrap();
        assert_eq!(field.value.as_string(), "200");
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    #[test]
    fn min_with_i32_computes_correctly() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("value", 100i32);
        row.min("value", -50i32);
        row.min("value", 200i32);

        let field = row.columns.get("value").unwrap();
        assert_eq!(field.value.as_string(), "-50");
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    #[test]
    fn max_with_mixed_integer_types() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("value", 100i64);
        row.max("value", 50u32);
        row.max("value", 200i32);

        let field = row.columns.get("value").unwrap();
        assert_eq!(field.value.as_string(), "200");
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    #[test]
    fn min_with_mixed_integer_types() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("value", 100i64);
        row.min("value", 50u32);
        row.min("value", 200i32);

        let field = row.columns.get("value").unwrap();
        assert_eq!(field.value.as_string(), "50");
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    #[test]
    fn max_with_bigdecimal() {
        use std::str::FromStr;
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("value", BigDecimal::from_str("100.5").unwrap());
        row.max("value", BigDecimal::from_str("50.25").unwrap());
        row.max("value", BigDecimal::from_str("200.75").unwrap());

        let field = row.columns.get("value").unwrap();
        assert_eq!(field.value.as_string(), "200.75");
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    #[test]
    fn min_with_bigdecimal() {
        use std::str::FromStr;
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("value", BigDecimal::from_str("100.5").unwrap());
        row.min("value", BigDecimal::from_str("50.25").unwrap());
        row.min("value", BigDecimal::from_str("200.75").unwrap());

        let field = row.columns.get("value").unwrap();
        assert_eq!(field.value.as_string(), "50.25");
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    #[test]
    fn max_with_bigint() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.max("value", BigInt::from(100));
        row.max("value", BigInt::from(50));
        row.max("value", BigInt::from(200));

        let field = row.columns.get("value").unwrap();
        assert_eq!(field.value.as_string(), "200");
        assert_eq!(field.update_op, UpdateOp::Max);
    }

    #[test]
    fn min_with_bigint() {
        let mut tables = Tables::new();
        let row = tables.upsert_row("test", "pk1");
        row.min("value", BigInt::from(100));
        row.min("value", BigInt::from(50));
        row.min("value", BigInt::from(200));

        let field = row.columns.get("value").unwrap();
        assert_eq!(field.value.as_string(), "50");
        assert_eq!(field.update_op, UpdateOp::Min);
    }

    // ============================================================
    // Verification tests for comparison semantics
    // ============================================================

    #[test]
    fn verify_integer_comparison_semantics() {
        use crate::numeric::NumericComparable;
        use std::str::FromStr;

        let bd_100 = BigDecimal::from_str("100").unwrap();

        // Test: 50 < 100 should return Less
        assert_eq!(50i64.cmp_to_big_decimal(&bd_100), Ordering::Less);

        // Test: 200 > 100 should return Greater
        assert_eq!(200i64.cmp_to_big_decimal(&bd_100), Ordering::Greater);

        // Test: 100 == 100 should return Equal
        assert_eq!(100i64.cmp_to_big_decimal(&bd_100), Ordering::Equal);

        // Test with negative numbers
        assert_eq!((-50i32).cmp_to_big_decimal(&bd_100), Ordering::Less);
    }
}
