//! Script code table

use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    merkle::{
        column::MerkleizedColumn,
        sparse::MerkleizedTableColumn,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::fuel_compression::RegistryKey;

use super::column::{
    CompressionColumn,
    MerkleizedColumnOf,
};

/// Table that indexes the script codes
pub struct ScriptCode;

impl MerkleizedTableColumn for ScriptCode {
    type TableColumn = CompressionColumn;

    fn table_column() -> Self::TableColumn {
        Self::TableColumn::ScriptCode
    }
}

impl Mappable for ScriptCode {
    type Key = Self::OwnedKey;
    type OwnedKey = RegistryKey;
    type Value = Self::OwnedValue;
    type OwnedValue = fuel_core_types::fuel_tx::ScriptCode;
}

impl TableWithBlueprint for ScriptCode {
    type Blueprint = Plain<Postcard, Raw>;
    type Column = MerkleizedColumnOf<Self>;

    fn column() -> Self::Column {
        Self::Column::TableColumn(Self::table_column())
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
