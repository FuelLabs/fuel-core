//! The module contains implementations and tests for the `ContractsAssets` table.

use crate::{
    blueprint::{
        avl_merkle,
        btree_merkle,
        sparse::{
            PrimaryKey,
            Sparse,
        },
    },
    codec::{
        primitive::Primitive,
        raw::Raw,
    },
    column::Column,
    storage_interlayer::Interlayer,
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
use fuel_core_types::fuel_merkle::common::Bytes32;
use fuel_vm_private::storage::ContractsAssetKey;

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

impl avl_merkle::PrefixedKey for ContractsAssetKey {
    fn prefix(&self) -> (Bytes32, Bytes32) {
        let contract_id = *self.contract_id();
        let asset_id = *self.asset_id();
        (contract_id.into(), asset_id.into())
    }
}

impl avl_merkle::FromPrefix for ContractsAssetKey {
    fn from_prefix(prefix: Bytes32, unique: Bytes32) -> Self {
        let contract_id = prefix.into();
        let asset_id = unique.into();
        Self::new(&contract_id, &asset_id)
    }
}

impl btree_merkle::PrefixedKey for ContractsAssetKey {
    fn prefix(&self) -> (Bytes32, Bytes32) {
        let contract_id = *self.contract_id();
        let asset_id = *self.asset_id();
        (contract_id.into(), asset_id.into())
    }
}

impl btree_merkle::FromPrefix for ContractsAssetKey {
    fn from_prefix(prefix: Bytes32, unique: Bytes32) -> Self {
        let contract_id = prefix.into();
        let asset_id = unique.into();
        Self::new(&contract_id, &asset_id)
    }
}

impl TableWithBlueprint for ContractsAssets {
    type Blueprint = btree_merkle::BTreeMerkle<
        ContractsAssetsMerkleMetadata,
        ContractsAssetsMerkleData,
        Primitive<8>,
    >;
}

impl Interlayer for ContractsAssets {
    type KeyCodec = Raw;
    type ValueCodec = Primitive<8>;
    type Column = Column;

    fn column() -> Column {
        Column::ContractsAssets
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Mappable;

    fn generate_key(
        primary_key: &<ContractsAssetsMerkleMetadata as Mappable>::Key,
        rng: &mut impl rand::Rng,
    ) -> <ContractsAssets as Mappable>::Key {
        let mut bytes = [0u8; 32];
        rng.fill(bytes.as_mut());
        <ContractsAssets as Mappable>::Key::new(primary_key, &bytes.into())
    }

    crate::basic_storage_tests!(
        ContractsAssets,
        <ContractsAssets as Mappable>::Key::default(),
        <ContractsAssets as Mappable>::Value::default(),
        <ContractsAssets as Mappable>::Value::default()
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
