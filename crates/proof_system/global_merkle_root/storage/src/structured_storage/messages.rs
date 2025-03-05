use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
};
use fuel_core_storage::tables::Messages;

impl MerkleizedTableColumn for Messages {
    type TableColumn = TableColumn;

    fn table_column() -> TableColumn {
        TableColumn::Messages
    }
}
