use crate::{
    column::TableColumn,
    merkle::MerklizedTableColumn,
};
use fuel_core_storage::tables::Coins;

impl MerklizedTableColumn for Coins {
    fn table_column() -> TableColumn {
        TableColumn::Coins
    }
}
