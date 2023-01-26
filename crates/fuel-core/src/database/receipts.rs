use crate::database::{
    storage::DatabaseColumn,
    Column,
};
use fuel_core_storage::tables::Receipts;

impl DatabaseColumn for Receipts {
    fn column() -> Column {
        Column::Receipts
    }
}
