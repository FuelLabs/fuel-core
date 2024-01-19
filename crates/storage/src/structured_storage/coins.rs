//! The module contains implementations and tests for the `Coins` table.

use crate::{
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    column::Column,
    structure::plain::Plain,
    structured_storage::TableWithStructure,
    tables::Coins,
};

impl TableWithStructure for Coins {
    type Structure = Plain<Primitive<33>, Postcard>;

    fn column() -> Column {
        Column::Coins
    }
}

#[cfg(test)]
crate::basic_storage_tests!(
    Coins,
    <Coins as crate::Mappable>::Key::default(),
    <Coins as crate::Mappable>::Value::default()
);
