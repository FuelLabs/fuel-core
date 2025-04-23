//! The module contains implementations and tests for the `ContractsAssets` table.

#[cfg(feature = "smt")]
mod smt {
    use crate::{
        Mappable,
        blueprint::sparse::{
            PrimaryKey,
            Sparse,
        },
        codec::{
            primitive::Primitive,
            raw::Raw,
        },
        column::Column,
        structured_storage::TableWithBlueprint,
        tables::{
            ContractsAssets,
            merkle::{
                ContractsAssetsMerkleData,
                ContractsAssetsMerkleMetadata,
            },
        },
    };
    use alloc::borrow::Cow;

    /// The key convertor used to convert the key from the `ContractsAssets` table
    /// to the key of the `ContractsAssetsMerkleMetadata` table.
    pub struct KeyConverter;

    impl PrimaryKey for KeyConverter {
        type InputKey = <ContractsAssets as Mappable>::Key;
        type OutputKey = <ContractsAssetsMerkleMetadata as Mappable>::Key;

        fn primary_key(key: &Self::InputKey) -> Cow<Self::OutputKey> {
            Cow::Borrowed(key.contract_id())
        }
    }

    impl TableWithBlueprint for ContractsAssets {
        type Blueprint = Sparse<
            Raw,
            Primitive<8>,
            ContractsAssetsMerkleMetadata,
            ContractsAssetsMerkleData,
            KeyConverter,
        >;
        type Column = Column;

        fn column() -> Column {
            Column::ContractsAssets
        }
    }

    #[cfg(test)]
    #[allow(non_snake_case)]
    mod test {
        use rand::{
            Rng,
            prelude::StdRng,
        };

        use crate::blueprint::sparse::root_storage_tests_smt::{
            SMTTestDataGenerator,
            Wrapper,
        };

        use super::*;

        fn generate_key(
            primary_key: &<ContractsAssetsMerkleMetadata as Mappable>::Key,
            rng: &mut impl rand::Rng,
        ) -> <ContractsAssets as Mappable>::Key {
            let mut bytes = [0u8; 32];
            rng.fill(bytes.as_mut());
            <ContractsAssets as Mappable>::Key::new(primary_key, &bytes.into())
        }

        fn generate_key_for_same_contract(
            rng: &mut impl rand::Rng,
        ) -> <ContractsAssets as Mappable>::Key {
            generate_key(&fuel_core_types::fuel_tx::ContractId::zeroed(), rng)
        }

        crate::basic_storage_tests!(
            ContractsAssets,
            <ContractsAssets as Mappable>::Key::default(),
            <ContractsAssets as Mappable>::Value::default(),
            <ContractsAssets as Mappable>::Value::default(),
            generate_key_for_same_contract
        );

        impl SMTTestDataGenerator for ContractsAssets {
            type Key = <ContractsAssets as Mappable>::Key;
            type PrimaryKey = <ContractsAssetsMerkleMetadata as Mappable>::Key;
            type Value = Wrapper<<ContractsAssets as Mappable>::Value>;

            fn primary_key() -> Self::PrimaryKey {
                <ContractsAssetsMerkleMetadata as Mappable>::Key::from([1u8; 32])
            }

            fn foreign_key() -> Self::PrimaryKey {
                <ContractsAssetsMerkleMetadata as Mappable>::Key::from([2u8; 32])
            }

            fn generate_key(
                current_key: &Self::PrimaryKey,
                rng: &mut StdRng,
            ) -> Self::Key {
                let mut bytes = [0u8; 32];
                rng.fill(bytes.as_mut());
                <ContractsAssets as Mappable>::Key::new(current_key, &bytes.into())
            }

            fn generate_value(rng: &mut StdRng) -> Self::Value {
                Wrapper(rng.r#gen())
            }
        }

        crate::root_storage_tests!(ContractsAssets);
    }
}

#[cfg(not(feature = "smt"))]
mod plain {
    use crate::{
        blueprint::plain::Plain,
        codec::{
            primitive::Primitive,
            raw::Raw,
        },
        column::Column,
        structured_storage::TableWithBlueprint,
        tables::ContractsAssets,
    };

    impl TableWithBlueprint for ContractsAssets {
        type Blueprint = Plain<Raw, Primitive<8>>;
        type Column = Column;

        fn column() -> Column {
            Column::ContractsAssets
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;
        use crate::Mappable;
        use fuel_core_types::fuel_tx::ContractId;

        fn generate_key(
            primary_key: &ContractId,
            rng: &mut impl rand::Rng,
        ) -> <ContractsAssets as Mappable>::Key {
            let mut bytes = [0u8; 32];
            rng.fill(bytes.as_mut());
            <ContractsAssets as Mappable>::Key::new(primary_key, &bytes.into())
        }

        fn generate_key_for_same_contract(
            rng: &mut impl rand::Rng,
        ) -> <ContractsAssets as Mappable>::Key {
            generate_key(&fuel_core_types::fuel_tx::ContractId::zeroed(), rng)
        }

        crate::basic_storage_tests!(
            ContractsAssets,
            <ContractsAssets as Mappable>::Key::default(),
            <ContractsAssets as Mappable>::Value::default(),
            <ContractsAssets as Mappable>::Value::default(),
            generate_key_for_same_contract
        );
    }
}
