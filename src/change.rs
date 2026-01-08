use crate::pb::database::field::UpdateOp;
use crate::pb::database::Field;
use std::str;
use substreams::pb::substreams::store_delta::Operation;
use substreams::scalar::{BigDecimal, BigInt};
use substreams::store::{
    DeltaBigDecimal, DeltaBigInt, DeltaBool, DeltaBytes, DeltaInt32, DeltaInt64, DeltaString,
};
use substreams::Hex;

pub trait ToField {
    fn to_field<N: AsRef<str>>(self, name: N) -> Field;
}

/// We require to use a custom trait because we need to customize some of the string
/// transformation and we need to control the String conversion ourself.
pub trait AsString {
    fn as_string(self) -> String;
}

macro_rules! impl_as_string_via_to_string {
    ($name:ty) => {
        impl AsString for $name {
            fn as_string(self) -> String {
                self.to_string()
            }
        }
    };
}

impl_as_string_via_to_string!(i8);
impl_as_string_via_to_string!(i16);
impl_as_string_via_to_string!(i32);
impl_as_string_via_to_string!(i64);
impl_as_string_via_to_string!(u8);
impl_as_string_via_to_string!(u16);
impl_as_string_via_to_string!(u32);
impl_as_string_via_to_string!(u64);
impl_as_string_via_to_string!(String);
impl_as_string_via_to_string!(&String);
impl_as_string_via_to_string!(&str);
impl_as_string_via_to_string!(bool);
impl_as_string_via_to_string!(BigDecimal);
impl_as_string_via_to_string!(&BigDecimal);
impl_as_string_via_to_string!(BigInt);
impl_as_string_via_to_string!(&BigInt);
impl_as_string_via_to_string!(::prost_types::Timestamp);
impl_as_string_via_to_string!(&::prost_types::Timestamp);

impl<T: AsRef<[u8]>> AsString for Hex<T> {
    fn as_string(self) -> String {
        self.to_string()
    }
}

impl<T: AsRef<[u8]>> AsString for &Hex<T> {
    fn as_string(self) -> String {
        self.to_string()
    }
}

impl AsString for Vec<u8> {
    fn as_string(self) -> String {
        Hex::encode(self)
    }
}

impl AsString for &Vec<u8> {
    fn as_string(self) -> String {
        Hex::encode(self)
    }
}

impl<T: AsString> ToField for (T, T) {
    fn to_field<N: AsRef<str>>(self, name: N) -> Field {
        Field {
            name: name.as_ref().to_string(),
            old_value: self.0.as_string(),
            new_value: self.1.as_string(),
            update_op: UpdateOp::Set as i32,
        }
    }
}

impl<T: AsString> ToField for (Option<T>, T) {
    fn to_field<N: AsRef<str>>(self, name: N) -> Field {
        match self {
            (Some(old), new) => ToField::to_field((old, new), name),
            (None, new) => Field {
                name: name.as_ref().to_string(),
                old_value: "".to_string(),
                new_value: new.as_string(),
                update_op: UpdateOp::Set as i32,
            },
        }
    }
}

impl<T: AsString> ToField for (T, Option<T>) {
    fn to_field<N: AsRef<str>>(self, name: N) -> Field {
        match self {
            (old, Some(new)) => ToField::to_field((old, new), name),
            (old, None) => Field {
                name: name.as_ref().to_string(),
                old_value: old.as_string(),
                new_value: "".to_string(),
                update_op: UpdateOp::Set as i32,
            },
        }
    }
}

fn delta_to_field<T: AsString>(
    name: &str,
    operation: Operation,
    old_value: T,
    new_value: T,
) -> Field {
    match Operation::from(operation) {
        Operation::Update => ToField::to_field((old_value, new_value), name),
        Operation::Create => ToField::to_field((None, new_value), name),
        Operation::Delete => ToField::to_field((None, new_value), name),
        Operation::Unset => panic!("unsupported operation {:?}", Operation::from(operation)),
    }
}

macro_rules! impl_to_field_from_delta_via_move {
    ($type:ty) => {
        impl ToField for $type {
            fn to_field<N: AsRef<str>>(self, name: N) -> Field {
                delta_to_field(
                    name.as_ref(),
                    self.operation,
                    self.old_value,
                    self.new_value,
                )
            }
        }
    };
}

macro_rules! impl_to_field_from_delta_via_ref {
    ($type:ty) => {
        impl ToField for $type {
            fn to_field<N: AsRef<str>>(self, name: N) -> Field {
                delta_to_field(
                    name.as_ref(),
                    self.operation,
                    &self.old_value,
                    &self.new_value,
                )
            }
        }
    };
}

impl_to_field_from_delta_via_move!(&DeltaInt32);
impl_to_field_from_delta_via_move!(&DeltaInt64);
impl_to_field_from_delta_via_ref!(&DeltaBigDecimal);
impl_to_field_from_delta_via_ref!(&DeltaBigInt);
impl_to_field_from_delta_via_move!(&DeltaBool);
impl_to_field_from_delta_via_ref!(&DeltaBytes);
impl_to_field_from_delta_via_ref!(&DeltaString);

#[cfg(test)]
mod test {
    use crate::change::ToField;
    use crate::pb::database::Field;
    use substreams::pb::substreams::store_delta::Operation;
    use substreams::scalar::{BigDecimal, BigInt};
    use substreams::store::{DeltaBigDecimal, DeltaBigInt, DeltaBool, DeltaBytes, DeltaString};

    const FIELD_NAME: &str = "field.name.1";

    #[test]
    fn i32_change() {
        let i32_change = (None, 1i32);
        assert_eq!(
            create_expected_field(FIELD_NAME, None, Some("1".to_string())),
            i32_change.to_field(FIELD_NAME)
        );
    }

    #[test]
    fn big_decimal_change() {
        let bd_change = (None, BigDecimal::from(1 as i32));
        assert_eq!(
            create_expected_field(FIELD_NAME, None, Some("1".to_string())),
            bd_change.to_field(FIELD_NAME)
        );
    }

    #[test]
    fn delta_big_decimal_change() {
        let delta = DeltaBigDecimal {
            operation: Operation::Update,
            ordinal: 0,
            key: "change".to_string(),
            old_value: BigDecimal::from(10),
            new_value: BigDecimal::from(20),
        };

        assert_eq!(
            create_expected_field(FIELD_NAME, Some("10".to_string()), Some("20".to_string())),
            delta.to_field(FIELD_NAME)
        );
    }

    #[test]
    fn big_int_change() {
        let bi_change = (None, BigInt::from(1 as i32));
        assert_eq!(
            create_expected_field(FIELD_NAME, None, Some("1".to_string())),
            bi_change.to_field(FIELD_NAME)
        );
    }

    #[test]
    fn delta_big_int_change() {
        let delta = DeltaBigInt {
            operation: Operation::Update,
            ordinal: 0,
            key: "change".to_string(),
            old_value: BigInt::from(10),
            new_value: BigInt::from(20),
        };

        assert_eq!(
            create_expected_field(FIELD_NAME, Some("10".to_string()), Some("20".to_string())),
            delta.to_field(FIELD_NAME)
        );
    }

    #[test]
    fn string_change() {
        let string_change = (None, String::from("string"));
        assert_eq!(
            create_expected_field(FIELD_NAME, None, Some("string".to_string())),
            string_change.to_field(FIELD_NAME)
        );
    }

    #[test]
    fn delta_string_change() {
        let delta = DeltaString {
            operation: Operation::Update,
            ordinal: 0,
            key: "change".to_string(),
            old_value: String::from("string1"),
            new_value: String::from("string2"),
        };

        assert_eq!(
            create_expected_field(
                FIELD_NAME,
                Some("string1".to_string()),
                Some("string2".to_string())
            ),
            delta.to_field(FIELD_NAME)
        );
    }

    #[test]
    fn bytes_change() {
        let bytes_change: (Option<Vec<u8>>, Vec<u8>) = (None, Vec::from("bytes"));
        assert_eq!(
            create_expected_field(FIELD_NAME, None, Some("6279746573".to_string())),
            bytes_change.to_field(FIELD_NAME)
        );
    }

    #[test]
    fn delta_bytes_change() {
        let delta = DeltaBytes {
            operation: Operation::Update,
            ordinal: 0,
            key: "change".to_string(),
            old_value: Vec::from("bytes1"),
            new_value: Vec::from("bytes2"),
        };

        assert_eq!(
            create_expected_field(
                FIELD_NAME,
                Some("627974657331".to_string()),
                Some("627974657332".to_string())
            ),
            delta.to_field(FIELD_NAME)
        );
    }

    #[test]
    fn bool_change() {
        let bool_change = (None, true);
        assert_eq!(
            create_expected_field(FIELD_NAME, None, Some("true".to_string())),
            bool_change.to_field(FIELD_NAME)
        );
    }

    #[test]
    fn delta_bool_change() {
        let delta = DeltaBool {
            operation: Operation::Update,
            ordinal: 0,
            key: "change".to_string(),
            old_value: true,
            new_value: false,
        };

        assert_eq!(
            create_expected_field(FIELD_NAME, Some(true.to_string()), Some(false.to_string()),),
            delta.to_field(FIELD_NAME)
        );
    }

    fn create_expected_field<N: AsRef<str>>(
        name: N,
        old_value: Option<String>,
        new_value: Option<String>,
    ) -> Field {
        let mut field = Field {
            name: name.as_ref().to_string(),
            ..Default::default()
        };
        if old_value.is_some() {
            field.old_value = old_value.unwrap()
        }
        if new_value.is_some() {
            field.new_value = new_value.unwrap()
        }
        field
    }
}
