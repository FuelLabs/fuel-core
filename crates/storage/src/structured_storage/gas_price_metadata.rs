//! The module contains implementations and tests for the `GasPriceMetadata` table.
use crate::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::GasPriceMetadata,
};

impl TableWithBlueprint for GasPriceMetadata {
    type Blueprint = Plain<Primitive<4>, Postcard>;
    type Column = Column;

    fn column() -> Self::Column {
        Column::GasPriceMetadata
    }
}
