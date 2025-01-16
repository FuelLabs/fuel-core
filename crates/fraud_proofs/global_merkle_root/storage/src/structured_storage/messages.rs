use crate::{
    column::TableColumns,
    merkle::MerklizedTableColumn,
};
use fuel_core_storage::tables::Messages;

impl MerklizedTableColumn for Messages {
    fn table_column() -> TableColumns {
        TableColumns::Messages
    }
}
