//! The module contains implementations and tests for the `Coins` table.

use crate::{
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::Coins,
};

#[cfg(not(feature = "global-state-root"))]
use crate::blueprint::plain::Plain;

#[cfg(feature = "global-state-root")]
use crate::{
    blueprint::sparse::PrimaryKey,
    blueprint::sparse::Sparse,
    tables::merkle::{
        CoinsMerkleData,
        CoinsMerkleMetadata,
    },
};

#[cfg(feature = "global-state-root")]
use fuel_vm_private::fuel_storage::Mappable;

#[cfg(not(feature = "global-state-root"))]
impl TableWithBlueprint for Coins {
    type Blueprint = Plain<Primitive<34>, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::Coins
    }
}

/// TODO: Document
#[cfg(feature = "global-state-root")]
pub struct KeyConverter;

#[cfg(feature = "global-state-root")]
impl PrimaryKey for KeyConverter {
    type InputKey = <Coins as Mappable>::Key;
    type OutputKey = ();

    fn primary_key(_key: &Self::InputKey) -> &Self::OutputKey {
        &()
    }
}

#[cfg(feature = "global-state-root")]
impl TableWithBlueprint for Coins {
    type Blueprint = Sparse<
        Primitive<34>,
        Postcard,
        CoinsMerkleMetadata,
        CoinsMerkleData,
        KeyConverter,
    >;

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
