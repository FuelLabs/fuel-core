//! The module contains implementations and tests for the `ContractsState` table.

use crate::{
    blueprint::sparse::{
        PrimaryKey,
        Sparse,
    },
    codec::{
        manual::Manual,
        raw::Raw,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
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

/// The key convertor used to convert the key from the `ContractsState` table
/// to the key of the `ContractsStateMerkleMetadata` table.
pub struct KeyConverter;

impl PrimaryKey for KeyConverter {
    type InputKey = <ContractsState as Mappable>::Key;
    type OutputKey = <ContractsStateMerkleMetadata as Mappable>::Key;

    fn primary_key(key: &Self::InputKey) -> &Self::OutputKey {
        key.contract_id()
    }
}

impl TableWithBlueprint for ContractsState {
    type Blueprint = Sparse<
        Manual<ContractsStateKey>,
        Raw,
        ContractsStateMerkleMetadata,
        ContractsStateMerkleData,
        KeyConverter,
    >;
    type Column = Column;

    fn column() -> Column {
        Column::ContractsState
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn generate_key(
        primary_key: &<ContractsStateMerkleMetadata as Mappable>::Key,
        rng: &mut impl rand::Rng,
    ) -> <ContractsState as Mappable>::Key {
        let mut bytes = [0u8; 32];
        rng.fill(bytes.as_mut());
        <ContractsState as Mappable>::Key::new(primary_key, &bytes.into())
    }

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
}
