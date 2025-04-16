//! The table for indexing the compressed blocks roots.

use super::column::{
    CompressionColumn,
    MerkleizedColumnOf,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        primitive::Primitive,
        raw::Raw,
    },
    merkle::sparse::MerkleizedTableColumn,
    structured_storage::TableWithBlueprint,
    Mappable,
    MerkleRoot,
};

use fuel_core_types::fuel_types::BlockHeight;

/// The table for the compressed blocks sent to DA.
pub struct CompressedBlocksRoots;

impl Mappable for CompressedBlocksRoots {
    type Key = Self::OwnedKey;
    type OwnedKey = BlockHeight;
    type Value = Self::OwnedValue;
    type OwnedValue = MerkleRoot;
}

impl MerkleizedTableColumn for CompressedBlocksRoots {
    type TableColumn = CompressionColumn;

    fn table_column() -> Self::TableColumn {
        Self::TableColumn::CompressedBlocksRoots
    }
}

impl TableWithBlueprint for CompressedBlocksRoots {
    type Blueprint = Plain<Primitive<4>, Raw>;
    type Column = MerkleizedColumnOf<Self>;

    fn column() -> Self::Column {
        Self::Column::TableColumn(Self::table_column())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fuel_core_storage::basic_storage_tests!(
        CompressedBlocksRoots,
        <CompressedBlocksRoots as Mappable>::Key::default(),
        <CompressedBlocksRoots as Mappable>::Value::default()
    );
}
