//! Address table.

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
pub struct Address;

impl Mappable for Address {
    type Key = Self::OwnedKey;
    type OwnedKey = RegistryKey;
    type Value = Self::OwnedValue;
    type OwnedValue = fuel_core_types::fuel_tx::Address;
}

impl TableWithBlueprint for Address {
    type Blueprint = Plain<Postcard, Raw>;
    type Column = CompressionColumn;

    fn column() -> Self::Column {
        Self::Column::Address
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_compression_service::storage::generate_key;

    fuel_core_storage::basic_storage_tests!(
        Address,
        RegistryKey::ZERO,
        <Address as Mappable>::Value::default(),
        <Address as Mappable>::Value::default(),
        generate_key
    );
}
