use crate::pb::sf::substreams::sink::database::v1::field::UpdateOp;

/// The module database exists for backward compatibility reason, it should NOT be used anymore.
///
/// Replace `substreams_database_change::pb::database::<...>` by `substreams_database_change::pb::sf::substreams::sink::database::v1::<...>`,
/// this module will be removed in future versions.
///
/// A simple search/replace is usually sufficient to update your code.
#[deprecated(
    since = "4.0.0",
    note = "The module database exists for backward compatibility reason, it should NOT be used anymore. Replace substreams_database_change::pb::database::<...> by substreams_database_change::pb::sf::substreams::sink::database::v1::<...>, this module will be removed in future versions."
)]
pub mod database {
    macro_rules! deprecated_reexport {
        ($($item:ident),* $(,)?) => {
            $(
                #[deprecated(
                    since = "4.0.0",
                    note = "Use substreams_database_change::pb::sf::substreams::sink::database::v1 instead, this module will be removed in future versions."
                )]
                pub type $item = super::sf::substreams::sink::database::v1::$item;
            )*
        };
    }

    deprecated_reexport!(DatabaseChanges, TableChange, CompositePrimaryKey, Field);

    pub mod table_change {
        macro_rules! deprecated_reexport {
            ($($item:ident),* $(,)?) => {
                $(
                    #[deprecated(
                        since = "4.0.0",
                        note = "Use substreams_database_change::pb::sf::substreams::sink::database::v1::table_change instead, this module will be removed in future versions."
                    )]
                    pub type $item = super::super::sf::substreams::sink::database::v1::table_change::$item;
                )*
            };
        }

        deprecated_reexport!(Operation, PrimaryKey);
    }

    pub mod field {
        macro_rules! deprecated_reexport {
            ($($item:ident),* $(,)?) => {
                $(
                    #[deprecated(
                        since = "4.0.0",
                        note = "Use substreams_database_change::pb::sf::substreams::sink::database::v1::field instead, this module will be removed in future versions."
                    )]
                    pub type $item = super::super::sf::substreams::sink::database::v1::field::$item;
                )*
            };
        }

        deprecated_reexport!(UpdateOp);
    }
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
