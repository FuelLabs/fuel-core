//! Predicate code table

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

/// Table that indexes the predicate codes
pub struct PredicateCode;

impl Mappable for PredicateCode {
    type Key = Self::OwnedKey;
    type OwnedKey = RegistryKey;
    type Value = Self::OwnedValue;
    type OwnedValue = fuel_core_types::fuel_tx::input::PredicateCode;
}

impl TableWithBlueprint for PredicateCode {
    type Blueprint = Plain<Postcard, Raw>;
    type Column = CompressionColumn;

    fn column() -> Self::Column {
        Self::Column::PredicateCode
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_compression_service::storage::generate_key;

    fuel_core_storage::basic_storage_tests!(
        PredicateCode,
        RegistryKey::ZERO,
        <PredicateCode as Mappable>::Value::default(),
        <PredicateCode as Mappable>::Value::default(),
        generate_key
    );
}
