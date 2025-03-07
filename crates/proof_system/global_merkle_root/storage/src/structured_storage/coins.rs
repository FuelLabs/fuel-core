use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
};
use fuel_core_storage::tables::Coins;

impl MerkleizedTableColumn for Coins {
    type TableColumn = TableColumn;

    fn table_column() -> TableColumn {
        TableColumn::Coins
    }
}
