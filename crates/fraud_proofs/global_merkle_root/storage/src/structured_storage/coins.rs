use crate::{
    column::TableColumns,
    merkle::MerklizedTableColumn,
};
use fuel_core_storage::tables::Coins;

impl MerklizedTableColumn for Coins {
    fn table_column() -> TableColumns {
        TableColumns::Coins
    }
}
