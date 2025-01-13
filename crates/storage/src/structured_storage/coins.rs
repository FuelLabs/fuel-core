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
mod plain {
    use super::*;
    use crate::blueprint::plain::Plain;

    #[cfg(not(feature = "global-state-root"))]
    impl TableWithBlueprint for Coins {
        type Blueprint = Plain<Primitive<34>, Postcard>;
        type Column = Column;

        fn column() -> Column {
            Column::Coins
        }
    }
}

#[cfg(feature = "global-state-root")]
mod sparse {
    use super::*;

    use crate::{
        blueprint::sparse::{
            PrimaryKey,
            Sparse,
        },
        tables::merkle::{
            CoinsMerkleData,
            CoinsMerkleMetadata,
        },
    };

    use fuel_vm_private::fuel_storage::Mappable;

    /// Maps coin IDs to the unit type, enabling a single global merkle root for all coins.
    pub struct KeyConverter;

    impl PrimaryKey for KeyConverter {
        type InputKey = <Coins as Mappable>::Key;
        type OutputKey = ();

        fn primary_key(_key: &Self::InputKey) -> &Self::OutputKey {
            &()
        }
    }

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
}

#[cfg(test)]
crate::basic_storage_tests!(
    Coins,
    <Coins as crate::Mappable>::Key::default(),
    <Coins as crate::Mappable>::Value::default()
);
