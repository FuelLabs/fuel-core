//! The module contains implementations and tests for the `SealedBlockConsensus` table.

use crate::{
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    column::Column,
    structure::plain::Plain,
    structured_storage::TableWithStructure,
    tables::SealedBlockConsensus,
};

impl TableWithStructure for SealedBlockConsensus {
    type Structure = Plain<Primitive<4>, Postcard>;

    fn column() -> Column {
        Column::FuelBlockConsensus
    }
}

#[cfg(test)]
crate::basic_storage_tests!(
    SealedBlockConsensus,
    <SealedBlockConsensus as crate::Mappable>::Key::default(),
    <SealedBlockConsensus as crate::Mappable>::Value::default()
);
