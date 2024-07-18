//! The module contains implementations and tests for the `SealedBlockConsensus` table.

use crate::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    column::Column,
    storage_interlayer::Interlayer,
    structured_storage::TableWithBlueprint,
    tables::SealedBlockConsensus,
};

impl TableWithBlueprint for SealedBlockConsensus {
    type Blueprint = Plain;
}

impl Interlayer for SealedBlockConsensus {
    type KeyCodec = Primitive<4>;
    type ValueCodec = Postcard;
    type Column = Column;

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
