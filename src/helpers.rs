use std::collections::HashMap;

use crate::change::ToField;
use crate::pb::sf::substreams::sink::database::v1::{
    table_change::{Operation, PrimaryKey},
    CompositePrimaryKey, DatabaseChanges, TableChange,
};

impl DatabaseChanges {
    pub fn push_change<T: AsRef<str>, K: AsRef<str>>(
        &mut self,
        table: T,
        pk: K,
        ordinal: u64,
        operation: Operation,
    ) -> &mut TableChange {
        let table_change = TableChange::new(table, pk, ordinal, operation);
        self.table_changes.push(table_change);
        return self.table_changes.last_mut().unwrap();
    }

    pub fn push_change_composite<T: AsRef<str>>(
        &mut self,
        table: T,
        keys: HashMap<String, String>,
        ordinal: u64,
        operation: Operation,
    ) -> &mut TableChange {
        let table_change = TableChange::new_composite(table, keys, ordinal, operation);
        self.table_changes.push(table_change);
        return self.table_changes.last_mut().unwrap();
    }
}

impl TableChange {
    pub fn new<T: AsRef<str>, K: AsRef<str>>(
        entity: T,
        pk: K,
        ordinal: u64,
        operation: Operation,
    ) -> TableChange {
        TableChange {
            table: entity.as_ref().to_string(),
            primary_key: Some(PrimaryKey::Pk(pk.as_ref().to_string())),
            ordinal,
            operation: operation.into(),
            fields: vec![],
        }
    }

    pub fn new_composite<T: AsRef<str>>(
        entity: T,
        keys: HashMap<String, String>,
        ordinal: u64,
        operation: Operation,
    ) -> TableChange {
        TableChange {
            table: entity.as_ref().to_string(),
            primary_key: Some(PrimaryKey::CompositePk(Box::new(CompositePrimaryKey {
                keys: keys.into_iter().collect(),
            }))),
            ordinal,
            operation: operation.into(),
            fields: vec![],
        }
    }

    pub fn change<N: AsRef<str>, T: ToField>(&mut self, name: N, change: T) -> &mut TableChange {
        self.fields.push(change.to_field(name));
        self
    }
}
