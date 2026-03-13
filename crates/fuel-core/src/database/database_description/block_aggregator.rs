use crate::database::database_description::DatabaseDescription;
use fuel_core_block_aggregator_api::db::table::Column;
use fuel_core_types::fuel_types::BlockHeight;

#[derive(Clone, Copy, Debug)]
pub struct BlockAggregatorDatabase;

impl DatabaseDescription for BlockAggregatorDatabase {
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
