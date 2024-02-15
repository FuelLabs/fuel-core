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
pub struct FuelBlockSecondaryKeyBlockHeights;

impl Mappable for FuelBlockSecondaryKeyBlockHeights {
    /// Primary key - `BlockId`.
    type Key = BlockId;
    type OwnedKey = Self::Key;
    /// Secondary key - `BlockHeight`.
    type Value = BlockHeight;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for FuelBlockSecondaryKeyBlockHeights {
    type Blueprint = Plain<Raw, Primitive<4>>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::FuelBlockSecondaryKeyBlockHeights
    }
}

#[cfg(test)]
fuel_core_storage::basic_storage_tests!(
    FuelBlockSecondaryKeyBlockHeights,
    <FuelBlockSecondaryKeyBlockHeights as Mappable>::Key::default(),
    <FuelBlockSecondaryKeyBlockHeights as Mappable>::Value::default()
);
