//! Predicate code table

use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    merkle::sparse::MerkleizedTableColumn,
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::fuel_compression::RegistryKey;

use super::column::{
    CompressionColumn,
    MerkleizedColumnOf,
};

/// Table that indexes the predicate codes
pub struct PredicateCode;

impl MerkleizedTableColumn for PredicateCode {
    type TableColumn = CompressionColumn;

    fn table_column() -> Self::TableColumn {
        Self::TableColumn::PredicateCode
    }
}

impl Mappable for PredicateCode {
    type Key = Self::OwnedKey;
    type OwnedKey = RegistryKey;
    type Value = Self::OwnedValue;
    type OwnedValue = fuel_core_types::fuel_tx::input::PredicateCode;
}

impl TableWithBlueprint for PredicateCode {
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
        PredicateCode,
        RegistryKey::ZERO,
        <PredicateCode as Mappable>::Value::default(),
        <PredicateCode as Mappable>::Value::default(),
        generate_key
    );
}
