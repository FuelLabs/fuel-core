//! The module contains implementations and tests for the `Coins` table.

use crate::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::Coins,
};

impl TableWithBlueprint for Coins {
    type Blueprint = Plain<Primitive<34>, Postcard>;
    type Column = Column;

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
