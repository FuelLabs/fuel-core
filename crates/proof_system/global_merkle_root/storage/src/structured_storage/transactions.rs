use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
};
use fuel_core_storage::tables::ProcessedTransactions;

impl MerkleizedTableColumn for ProcessedTransactions {
    type TableColumn = TableColumn;

    fn table_column() -> TableColumn {
        TableColumn::ProcessedTransactions
    }
}
