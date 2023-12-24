use crate::{
    codec::{
        manual::Manual,
        raw::Raw,
    },
    column::Column,
    structure::sparse::{
        MetadataKey,
        Sparse,
    },
    structured_storage::TableWithStructure,
    tables::{
        merkle::{
            ContractsStateMerkleData,
            ContractsStateMerkleMetadata,
        },
        ContractsState,
    },
    Mappable,
};
use fuel_core_types::fuel_vm::ContractsStateKey;

pub struct KeyConvertor;

impl MetadataKey for KeyConvertor {
    type InputKey = <ContractsState as Mappable>::Key;
    type OutputKey = <ContractsStateMerkleMetadata as Mappable>::Key;

    fn metadata_key(key: &Self::InputKey) -> &Self::OutputKey {
        key.contract_id()
    }
}

impl TableWithStructure for ContractsState {
    type Structure = Sparse<
        Manual<ContractsStateKey>,
        Raw,
        ContractsStateMerkleMetadata,
        ContractsStateMerkleData,
        KeyConvertor,
    >;

    fn column() -> Column {
        Column::ContractsState
    }
}

#[cfg(test)]
fn generate_key(
    metadata_key: &<ContractsStateMerkleMetadata as Mappable>::Key,
    rng: &mut impl rand::Rng,
) -> <ContractsState as Mappable>::Key {
    let mut bytes = [0u8; 32];
    rng.fill(bytes.as_mut());
    <ContractsState as Mappable>::Key::new(metadata_key, &bytes.into())
}

#[cfg(test)]
fn generate_key_for_same_contract(
    rng: &mut impl rand::Rng,
) -> <ContractsState as Mappable>::Key {
    generate_key(&fuel_core_types::fuel_tx::ContractId::zeroed(), rng)
}

crate::basic_storage_tests!(
    ContractsState,
    <ContractsState as Mappable>::Key::default(),
    <ContractsState as Mappable>::Value::zeroed(),
    <ContractsState as Mappable>::Value::zeroed(),
    generate_key_for_same_contract
);

#[cfg(test)]
fn generate_value(rng: &mut impl rand::Rng) -> <ContractsState as Mappable>::Value {
    let mut bytes = [0u8; 32];
    rng.fill(bytes.as_mut());
    bytes.into()
}

crate::root_storage_tests!(
    ContractsState,
    ContractsStateMerkleMetadata,
    <ContractsStateMerkleMetadata as Mappable>::Key::from([1u8; 32]),
    <ContractsStateMerkleMetadata as Mappable>::Key::from([2u8; 32]),
    generate_key,
    generate_value
);
