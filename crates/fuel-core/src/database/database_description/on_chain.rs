use crate::database::database_description::DatabaseDescription;
use fuel_core_types::fuel_types::BlockHeight;

#[derive(Copy, Clone, Debug)]
pub struct OnChain;

impl DatabaseDescription for OnChain {
    type Column = fuel_core_storage::column::Column;
    type Height = BlockHeight;

    fn version() -> u32 {
        0
    }

    fn name() -> String {
        "on_chain".to_string()
    }

    fn metadata_column() -> Self::Column {
        Self::Column::Metadata
    }

    fn prefix(column: &Self::Column) -> Option<usize> {
        match column {
            Self::Column::ContractsAssets | Self::Column::ContractsState => {
                // prefix is address length
                Some(32)
            }
            _ => None,
        }
    }
}
