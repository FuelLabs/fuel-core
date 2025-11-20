//! The module contains implementations and tests for the `ContractsState` table.

use crate::{
    blueprint::plain::Plain, codec::raw::Raw, column::Column,
    structured_storage::TableWithBlueprint, tables::ContractsState,
};

impl TableWithBlueprint for ContractsState {
    type Blueprint = Plain<Raw, Raw>;
    type Column = Column;

    fn column() -> Column {
        Column::ContractsState
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
        [0u8; 32],
        vec![0u8; 32].into(),
        generate_key_for_same_contract
    );
}
