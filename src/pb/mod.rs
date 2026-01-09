use crate::pb::sf::substreams::sink::database::v1::field::UpdateOp;

/// The module database exists for backward compatibility reason, it should NOT be used anymore.
///
/// Replace `substreams_database_change::pb::database::<...>` by `substreams_database_change::pb::sf::substreams::sink::database::v1::<...>`,
/// this module will be removed in future versions.
///
/// A simple search/replace is usually sufficient to update your code.
pub mod database {
    include!("sf.substreams.sink.database.v1.rs");
}

include!("pb.rs");

impl UpdateOp {
    pub fn as_display_name(&self) -> &'static str {
        match self {
            UpdateOp::Unspecified => "unspecified",
            UpdateOp::Add => "add/sub",
            UpdateOp::Max => "max",
            UpdateOp::Min => "min",
            UpdateOp::SetIfNull => "set_if_null",
            UpdateOp::Set => "set",
        }
    }
}
