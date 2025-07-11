//! Script code table

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

/// Table that indexes the script codes
pub struct ScriptCode;

impl Mappable for ScriptCode {
    type Key = Self::OwnedKey;
    type OwnedKey = RegistryKey;
    type Value = Self::OwnedValue;
    type OwnedValue = fuel_core_types::fuel_tx::ScriptCode;
}

impl TableWithBlueprint for ScriptCode {
    type Blueprint = Plain<Postcard, Raw>;
    type Column = CompressionColumn;

    fn column() -> Self::Column {
        Self::Column::ScriptCode
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_compression_service::storage::generate_key;

    fuel_core_storage::basic_storage_tests!(
        ScriptCode,
        RegistryKey::ZERO,
        <ScriptCode as Mappable>::Value::default(),
        <ScriptCode as Mappable>::Value::default(),
        generate_key
    );
}
