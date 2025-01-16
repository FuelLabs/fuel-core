use crate::{
    column::TableColumn,
    merkle::MerklizedTableColumn,
};
use fuel_core_storage::tables::ProcessedTransactions;

impl MerklizedTableColumn for ProcessedTransactions {
    fn table_column() -> TableColumn {
        TableColumn::ProcessedTransactions
    }
}
