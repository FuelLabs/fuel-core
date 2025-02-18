use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
};
use fuel_core_storage::tables::ProcessedTransactions;

impl MerkleizedTableColumn for ProcessedTransactions {
    fn table_column() -> TableColumn {
        TableColumn::ProcessedTransactions
    }
}
