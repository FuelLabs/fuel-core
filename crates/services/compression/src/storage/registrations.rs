//! Registrations table.
//! Maps from block height -> registrations made for compressing that block

use fuel_core_storage::{
    Mappable,
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    merkle::sparse::MerkleizedTableColumn,
    structured_storage::TableWithBlueprint,
};

use super::column::{
    CompressionColumn,
    MerkleizedColumnOf,
};

/// Table that indexes the registrations.
pub struct Registrations;

impl MerkleizedTableColumn for Registrations {
    type TableColumn = CompressionColumn;

    fn table_column() -> Self::TableColumn {
        Self::TableColumn::Registrations
    }
}

impl Mappable for Registrations {
    type Key = Self::OwnedKey;
    type OwnedKey = fuel_core_types::fuel_types::BlockHeight;
    type Value = Self::OwnedValue;
    type OwnedValue = fuel_core_compression::registry::RegistrationsPerTable;
}

impl TableWithBlueprint for Registrations {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = MerkleizedColumnOf<Self>;

    fn column() -> Self::Column {
        Self::Column::TableColumn(Self::table_column())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fuel_core_storage::basic_storage_tests!(
        Registrations,
        <Registrations as Mappable>::Key::default(),
        <Registrations as Mappable>::Value::default(),
        <Registrations as Mappable>::Value::default()
    );
}
