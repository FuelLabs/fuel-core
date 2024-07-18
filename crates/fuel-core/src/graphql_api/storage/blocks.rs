use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        primitive::Primitive,
        raw::Raw,
    },
    storage_interlayer::Interlayer,
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
    type Blueprint = Plain;
}

impl Interlayer for FuelBlockIdsToHeights {
    type KeyCodec = Raw;
    type ValueCodec = Primitive<4>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::FuelBlockIdsToHeights
    }
}

#[cfg(test)]
fuel_core_storage::basic_storage_tests!(
    FuelBlockIdsToHeights,
    <FuelBlockIdsToHeights as Mappable>::Key::default(),
    <FuelBlockIdsToHeights as Mappable>::Value::default()
);
