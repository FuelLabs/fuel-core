use fuel_core_gas_price_service::fuel_gas_price_updater::fuel_core_storage_adapter::storage::GasPriceColumn;
use fuel_core_types::fuel_types::BlockHeight;
use crate::database::database_description::DatabaseDescription;

#[derive(Clone, Copy, Debug)]
pub struct GasPriceDatabase;

impl DatabaseDescription for GasPriceDatabase {
    type Column = GasPriceColumn;
    type Height = BlockHeight;

    fn version() -> u32 {
        0
    }

    fn name() -> String {
        "gas_price".to_string()
    }

    fn metadata_column() -> Self::Column {
        GasPriceColumn::Metadata
    }

    fn prefix(_column: &Self::Column) -> Option<usize> {
        None
    }
}
