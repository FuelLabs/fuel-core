//! The module contains implementations and tests for the `SealedBlockConsensus` table.

use crate::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::SealedBlockConsensus,
};

impl TableWithBlueprint for SealedBlockConsensus {
    type Blueprint = Plain<Raw, Postcard>;

    fn column() -> Column {
        Column::FuelBlockConsensus
    }
}

#[cfg(test)]
crate::basic_storage_tests!(
    SealedBlockConsensus,
    <SealedBlockConsensus as crate::Mappable>::Key::from([1u8; 32]),
    <SealedBlockConsensus as crate::Mappable>::Value::default()
);
