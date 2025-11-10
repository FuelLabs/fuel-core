use crate::database::database_description::DatabaseDescription;
use fuel_core_block_aggregator_api::db::table::Column;
use fuel_core_types::fuel_types::BlockHeight;

#[derive(Clone, Copy, Debug)]
pub struct BlockAggregatorDatabaseStorage;

impl DatabaseDescription for BlockAggregatorDatabaseStorage {
    type Column = Column;
    type Height = BlockHeight;

    fn version() -> u32 {
        0
    }

    fn name() -> String {
        "block_aggregator_storage".to_string()
    }

    fn metadata_column() -> Self::Column {
        Column::Metadata
    }

    fn prefix(_column: &Self::Column) -> Option<usize> {
        None
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BlockAggregatorDatabaseS3;

impl DatabaseDescription for BlockAggregatorDatabaseS3 {
    type Column = Column;
    type Height = BlockHeight;

    fn version() -> u32 {
        0
    }

    fn name() -> String {
        "block_aggregator_s3".to_string()
    }

    fn metadata_column() -> Self::Column {
        Column::Metadata
    }

    fn prefix(_column: &Self::Column) -> Option<usize> {
        None
    }
}
