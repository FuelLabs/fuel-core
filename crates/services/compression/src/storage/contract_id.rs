//! Contract ID table.

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

/// Table that indexes the addresses.
pub struct ContractId;

impl MerkleizedTableColumn for ContractId {
    type TableColumn = CompressionColumn;

    fn table_column() -> Self::TableColumn {
        Self::TableColumn::ContractId
    }
}

impl Mappable for ContractId {
    type Key = Self::OwnedKey;
    type OwnedKey = RegistryKey;
    type Value = Self::OwnedValue;
    type OwnedValue = fuel_core_types::fuel_tx::ContractId;
}

impl TableWithBlueprint for ContractId {
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
        ContractId,
        RegistryKey::ZERO,
        <ContractId as Mappable>::Value::default(),
        <ContractId as Mappable>::Value::default(),
        generate_key
    );
}
