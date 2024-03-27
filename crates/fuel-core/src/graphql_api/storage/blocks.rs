use fuel_core_chain_config::{
    AddTable,
    AsTable,
    StateConfig,
    StateConfigBuilder,
    TableEntry,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        primitive::Primitive,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::{
    blockchain::primitives::BlockId,
    fuel_types::BlockHeight,
};

/// The table of fuel block's secondary key - `BlockId`.
/// It links the `BlockId` to corresponding `BlockHeight`.
pub struct FuelBlockIdsToHeights;

impl Mappable for FuelBlockIdsToHeights {
    /// Primary key - `BlockId`.
    type Key = BlockId;
    type OwnedKey = Self::Key;
    /// Secondary key - `BlockHeight`.
    type Value = BlockHeight;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for FuelBlockIdsToHeights {
    type Blueprint = Plain<Raw, Primitive<4>>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::FuelBlockIdsToHeights
    }
}

impl AsTable<FuelBlockIdsToHeights> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<FuelBlockIdsToHeights>> {
        Vec::new() // Do not include these for now
    }
}

impl AddTable<FuelBlockIdsToHeights> for StateConfigBuilder {
    fn add(&mut self, _entries: Vec<TableEntry<FuelBlockIdsToHeights>>) {
        // Do not include these for now
    }
}

#[cfg(test)]
fuel_core_storage::basic_storage_tests!(
    FuelBlockIdsToHeights,
    <FuelBlockIdsToHeights as Mappable>::Key::default(),
    <FuelBlockIdsToHeights as Mappable>::Value::default()
);
