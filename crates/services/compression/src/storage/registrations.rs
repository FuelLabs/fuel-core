//! Registrations table.
//! Maps from block height -> registrations made for compressing that block

use fuel_core_storage::{
    Mappable,
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    structured_storage::TableWithBlueprint,
};

use super::column::{
    CompressionColumn,
    MerkleizedColumnOf,
};

/// Table that indexes the registrations.
pub struct Registrations;

impl Mappable for Registrations {
    type Key = Self::OwnedKey;
    type OwnedKey = fuel_core_types::fuel_types::BlockHeight;
    type Value = Self::OwnedValue;
    type OwnedValue = fuel_core_compression::registry::RegistrationsPerTable;
}

impl TableWithBlueprint for Registrations {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = CompressionColumn;

    fn column() -> Self::Column {
        Self::Column::Registrations
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
