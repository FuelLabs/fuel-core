use crate::{
    column::TableColumn,
    merkle::MerklizedTableColumn,
};
use fuel_core_storage::tables::Messages;

impl MerklizedTableColumn for Messages {
    fn table_column() -> TableColumn {
        TableColumn::Messages
    }
}
