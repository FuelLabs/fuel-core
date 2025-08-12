//! Asset ID table.

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

/// Table that indexes the asset ids.
pub struct AssetId;

impl Mappable for AssetId {
    type Key = Self::OwnedKey;
    type OwnedKey = RegistryKey;
    type Value = Self::OwnedValue;
    type OwnedValue = fuel_core_types::fuel_tx::AssetId;
}

impl TableWithBlueprint for AssetId {
    type Blueprint = Plain<Postcard, Raw>;
    type Column = CompressionColumn;

    fn column() -> Self::Column {
        Self::Column::AssetId
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_compression_service::storage::generate_key;

    fuel_core_storage::basic_storage_tests!(
        AssetId,
        RegistryKey::ZERO,
        <AssetId as Mappable>::Value::default(),
        <AssetId as Mappable>::Value::default(),
        generate_key
    );
}
