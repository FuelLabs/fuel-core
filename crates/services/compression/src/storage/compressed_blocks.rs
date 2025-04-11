//! The table for the compressed blocks sent to DA.

use super::column::{
    CompressionColumn,
    MerkleizedColumnOf,
};
use fuel_core_compression::VersionedCompressedBlock;
use fuel_core_storage::{
    Mappable,
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    merkle::sparse::MerkleizedTableColumn,
    structured_storage::TableWithBlueprint,
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

impl MerkleizedTableColumn for CompressedBlocks {
    type TableColumn = CompressionColumn;

    fn table_column() -> Self::TableColumn {
        Self::TableColumn::CompressedBlocks
    }
}

impl TableWithBlueprint for CompressedBlocks {
    // we don't use the Merkleized blueprint because we don't need it for this table
    type Blueprint = Plain<Primitive<4>, Postcard>;
    type Column = MerkleizedColumnOf<Self>;

    fn column() -> Self::Column {
        Self::Column::TableColumn(Self::table_column())
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
