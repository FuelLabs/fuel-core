use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
};
use fuel_core_storage::tables::Messages;

impl MerkleizedTableColumn for Messages {
    fn table_column() -> TableColumn {
        TableColumn::Messages
    }
}
