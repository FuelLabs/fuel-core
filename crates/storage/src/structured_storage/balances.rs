//! The module contains implementations and tests for the `ContractsAssets` table.

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
