//! The module contains implementations and tests for the `ContractsAssets` table.

use crate::{
    blueprint::sparse::{
        PrimaryKey,
        Sparse,
    },
    codec::{
        manual::Manual,
        primitive::Primitive,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::{
        merkle::{
            ContractsAssetsMerkleData,
            ContractsAssetsMerkleMetadata,
        },
        ContractsAssets,
    },
    Mappable,
};
use fuel_core_types::fuel_vm::ContractsAssetKey;

/// The key convertor used to convert the key from the `ContractsAssets` table
/// to the key of the `ContractsAssetsMerkleMetadata` table.
pub struct KeyConverter;

impl PrimaryKey for KeyConverter {
    type InputKey = <ContractsAssets as Mappable>::Key;
    type OutputKey = <ContractsAssetsMerkleMetadata as Mappable>::Key;

    fn primary_key(key: &Self::InputKey) -> &Self::OutputKey {
        key.contract_id()
    }
}

impl TableWithBlueprint for ContractsAssets {
    type Blueprint = Sparse<
        Manual<ContractsAssetKey>,
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
mod test {
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

    fn generate_value(rng: &mut impl rand::Rng) -> <ContractsAssets as Mappable>::Value {
        rng.gen()
    }

    crate::root_storage_tests!(
        ContractsAssets,
        ContractsAssetsMerkleMetadata,
        <ContractsAssetsMerkleMetadata as Mappable>::Key::from([1u8; 32]),
        <ContractsAssetsMerkleMetadata as Mappable>::Key::from([2u8; 32]),
        generate_key,
        generate_value
    );
}
