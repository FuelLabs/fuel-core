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

    use fuel_vm_private::fuel_storage::Mappable;

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

    #[cfg(test)]
    mod tests {
        use super::*;
        use fuel_core_types::entities::coins::coin::{
            CompressedCoin,
            CompressedCoinV1,
        };
        use fuel_vm_private::{
            fuel_storage::Mappable,
            prelude::{
                Address,
                AssetId,
                Bytes32,
                TxPointer,
                Word,
            },
        };

        fn generate_key(
            _primary_key: &<CoinsMerkleMetadata as Mappable>::Key,
            rng: &mut impl rand::Rng,
        ) -> <Coins as Mappable>::Key {
            let tx_id: Bytes32 = rng.gen();
            let output_index = rng.gen();
            <Coins as Mappable>::Key::new(tx_id, output_index)
        }

        fn generate_value(rng: &mut impl rand::Rng) -> <Coins as Mappable>::Value {
            let owner: Address = rng.gen();
            let amount: Word = rng.gen();
            let asset_id: AssetId = rng.gen();
            let tx_pointer: TxPointer = rng.gen();

            CompressedCoin::V1(CompressedCoinV1 {
                owner,      // Address,
                amount,     //  Word,
                asset_id,   // AssetId,
                tx_pointer, // TxPointer,
            })
        }

        crate::root_storage_tests!(
            Coins,
            CoinsMerkleMetadata,
            (),
            (),
            generate_key,
            generate_value
        );
    }
}

#[cfg(test)]
crate::basic_storage_tests!(
    Coins,
    <Coins as crate::Mappable>::Key::default(),
    <Coins as crate::Mappable>::Value::default()
);
