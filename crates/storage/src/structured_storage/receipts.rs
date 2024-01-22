//! The module contains implementations and tests for the `Receipts` table.

use crate::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::Receipts,
};

impl TableWithBlueprint for Receipts {
    type Blueprint = Plain<Raw, Postcard>;

    fn column() -> Column {
        Column::Receipts
    }
}

#[cfg(test)]
crate::basic_storage_tests!(
    Receipts,
    <Receipts as crate::Mappable>::Key::from([1u8; 32]),
    vec![fuel_core_types::fuel_tx::Receipt::ret(
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default()
    )]
);
