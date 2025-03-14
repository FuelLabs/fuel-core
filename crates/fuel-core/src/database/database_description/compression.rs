use crate::database::database_description::DatabaseDescription;
use fuel_core_compression_service::storage::column::CompressionColumn;
use fuel_core_storage::merkle::column::MerkleizedColumn;
use fuel_core_types::fuel_types::BlockHeight;

#[derive(Clone, Copy, Debug)]
pub struct CompressionDatabase;

impl DatabaseDescription for CompressionDatabase {
    type Column = MerkleizedColumn<CompressionColumn>;
    type Height = BlockHeight;

    fn version() -> u32 {
        0
    }

    fn name() -> String {
        "compression".to_string()
    }

    fn metadata_column() -> Self::Column {
        Self::Column::MerkleMetadataColumn
    }

    fn prefix(_column: &Self::Column) -> Option<usize> {
        None
    }
}
