use crate::{
    column::TableColumns,
    merkle::MerklizedTableColumn,
};
use fuel_core_storage::tables::ProcessedTransactions;

impl MerklizedTableColumn for ProcessedTransactions {
    fn table_column() -> TableColumns {
        TableColumns::ProcessedTransactions
    }
}
