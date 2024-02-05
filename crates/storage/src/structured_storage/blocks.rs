//! The module contains implementations and tests for the `FuelBlocks` table.

use crate::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::FuelBlocks,
};

impl TableWithBlueprint for FuelBlocks {
    type Blueprint = Plain<Primitive<4>, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::FuelBlocks
    }
}

#[cfg(test)]
crate::basic_storage_tests!(
    FuelBlocks,
    <FuelBlocks as crate::Mappable>::Key::default(),
    <FuelBlocks as crate::Mappable>::Value::default()
);
