//! Contract ID table.

use fuel_core_storage::{
    Mappable,
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
};
use fuel_core_types::fuel_compression::RegistryKey;

use super::column::CompressionColumn;

/// Table that indexes the addresses.
pub struct ContractId;

impl Mappable for ContractId {
    type Key = Self::OwnedKey;
    type OwnedKey = RegistryKey;
    type Value = Self::OwnedValue;
    type OwnedValue = fuel_core_types::fuel_tx::ContractId;
}

impl TableWithBlueprint for ContractId {
    type Blueprint = Plain<Postcard, Raw>;
    type Column = CompressionColumn;

    fn column() -> Self::Column {
        Self::Column::ContractId
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_compression_service::storage::generate_key;

    fuel_core_storage::basic_storage_tests!(
        ContractId,
        RegistryKey::ZERO,
        <ContractId as Mappable>::Value::default(),
        <ContractId as Mappable>::Value::default(),
        generate_key
    );
}
