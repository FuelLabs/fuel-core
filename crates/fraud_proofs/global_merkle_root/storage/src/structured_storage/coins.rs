use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
};
use fuel_core_storage::tables::Coins;

impl MerkleizedTableColumn for Coins {
    fn table_column() -> TableColumn {
        TableColumn::Coins
    }
}
