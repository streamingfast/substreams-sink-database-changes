// @generated
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DatabaseChanges {
    #[prost(message, repeated, tag="1")]
    pub table_changes: ::prost::alloc::vec::Vec<TableChange>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TableChange {
    #[prost(string, tag="1")]
    pub table: ::prost::alloc::string::String,
    #[prost(uint64, tag="3")]
    pub ordinal: u64,
    #[prost(enumeration="table_change::Operation", tag="4")]
    pub operation: i32,
    #[prost(message, repeated, tag="5")]
    pub fields: ::prost::alloc::vec::Vec<Field>,
    #[prost(oneof="table_change::PrimaryKey", tags="2, 6")]
    pub primary_key: ::core::option::Option<table_change::PrimaryKey>,
}
/// Nested message and enum types in `TableChange`.
pub mod table_change {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum Operation {
        /// Protobuf default should not be used, this is used so that the consume can ensure that the value was actually specified
        Unspecified = 0,
        Create = 1,
        Update = 2,
        Delete = 3,
        /// The upsert might not be supported by all drivers the sink supports,
        /// refer to <https://github.com/streamingfast/substreams-sink-sql> for
        /// which drivers support it.
        ///
        /// At time of writing, the Postgres driver supports was the only one supporting
        /// it.
        Upsert = 4,
    }
    impl Operation {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Operation::Unspecified => "OPERATION_UNSPECIFIED",
                Operation::Create => "OPERATION_CREATE",
                Operation::Update => "OPERATION_UPDATE",
                Operation::Delete => "OPERATION_DELETE",
                Operation::Upsert => "OPERATION_UPSERT",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "OPERATION_UNSPECIFIED" => Some(Self::Unspecified),
                "OPERATION_CREATE" => Some(Self::Create),
                "OPERATION_UPDATE" => Some(Self::Update),
                "OPERATION_DELETE" => Some(Self::Delete),
                "OPERATION_UPSERT" => Some(Self::Upsert),
                _ => None,
            }
        }
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum PrimaryKey {
        #[prost(string, tag="2")]
        Pk(::prost::alloc::string::String),
        #[prost(message, tag="6")]
        CompositePk(super::CompositePrimaryKey),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CompositePrimaryKey {
    #[prost(map="string, string", tag="1")]
    pub keys: ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Field {
    #[prost(string, tag="1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub new_value: ::prost::alloc::string::String,
    #[prost(string, tag="3")]
    pub old_value: ::prost::alloc::string::String,
    #[prost(enumeration="field::UpdateOp", tag="4")]
    pub update_op: i32,
}
/// Nested message and enum types in `Field`.
pub mod field {
    /// UpdateOp specifies how the field value should be applied during UPSERT.
    /// Only used when TableChange.operation is OPERATION_UPSERT.
    /// Defaults to UPDATE_OP_SET if not specified.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum UpdateOp {
        /// SET: column = new_value (default)
        Set = 0,
        /// ADD: column = COALESCE(column, 0) + new_value
        Add = 1,
        /// MAX: column = GREATEST(COALESCE(column, new_value), new_value)
        Max = 2,
        /// MIN: column = LEAST(COALESCE(column, new_value), new_value)
        Min = 3,
        /// SET_IF_NULL: column = COALESCE(column, new_value)
        SetIfNull = 4,
    }
    impl UpdateOp {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                UpdateOp::Set => "UPDATE_OP_SET",
                UpdateOp::Add => "UPDATE_OP_ADD",
                UpdateOp::Max => "UPDATE_OP_MAX",
                UpdateOp::Min => "UPDATE_OP_MIN",
                UpdateOp::SetIfNull => "UPDATE_OP_SET_IF_NULL",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "UPDATE_OP_SET" => Some(Self::Set),
                "UPDATE_OP_ADD" => Some(Self::Add),
                "UPDATE_OP_MAX" => Some(Self::Max),
                "UPDATE_OP_MIN" => Some(Self::Min),
                "UPDATE_OP_SET_IF_NULL" => Some(Self::SetIfNull),
                _ => None,
            }
        }
    }
}
// @@protoc_insertion_point(module)
