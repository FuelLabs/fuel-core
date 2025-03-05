use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
};
use fuel_core_storage::tables::BlobData;

impl MerkleizedTableColumn for BlobData {
    type TableColumn = TableColumn;

    fn table_column() -> TableColumn {
        TableColumn::Blobs
    }
}
