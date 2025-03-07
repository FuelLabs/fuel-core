//! The table for the compressed blocks sent to DA.

use crate::storage::column::CompressionColumn;
use fuel_core_compression::VersionedCompressedBlock;
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::fuel_types::BlockHeight;

/// The table for the compressed blocks sent to DA.
pub struct CompressedBlocks;

impl Mappable for CompressedBlocks {
    type Key = Self::OwnedKey;
    type OwnedKey = BlockHeight;
    type Value = Self::OwnedValue;
    type OwnedValue = VersionedCompressedBlock;
}

impl TableWithBlueprint for CompressedBlocks {
    type Blueprint = Plain<Primitive<4>, Postcard>;
    type Column = CompressionColumn;

    fn column() -> Self::Column {
        Self::Column::CompressedBlocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fuel_core_storage::basic_storage_tests!(
        CompressedBlocks,
        <CompressedBlocks as Mappable>::Key::default(),
        <CompressedBlocks as Mappable>::Value::default()
    );
}
