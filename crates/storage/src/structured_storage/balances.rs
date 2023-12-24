use crate::{
    codec::{
        manual::Manual,
        primitive::Primitive,
    },
    column::Column,
    structure::sparse::{
        MetadataKey,
        Sparse,
    },
    structured_storage::TableWithStructure,
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

pub struct KeyConvertor;

impl MetadataKey for KeyConvertor {
    type InputKey = <ContractsAssets as Mappable>::Key;
    type OutputKey = <ContractsAssetsMerkleMetadata as Mappable>::Key;

    fn metadata_key(key: &Self::InputKey) -> &Self::OutputKey {
        key.contract_id()
    }
}

impl TableWithStructure for ContractsAssets {
    type Structure = Sparse<
        Manual<ContractsAssetKey>,
        Primitive<8>,
        ContractsAssetsMerkleMetadata,
        ContractsAssetsMerkleData,
        KeyConvertor,
    >;

    fn column() -> Column {
        Column::ContractsAssets
    }
}

#[cfg(test)]
fn generate_key(
    metadata_key: &<ContractsAssetsMerkleMetadata as Mappable>::Key,
    rng: &mut impl rand::Rng,
) -> <ContractsAssets as Mappable>::Key {
    let mut bytes = [0u8; 32];
    rng.fill(bytes.as_mut());
    <ContractsAssets as Mappable>::Key::new(metadata_key, &bytes.into())
}

#[cfg(test)]
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

#[cfg(test)]
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
