use crate::{
    column::TableColumn,
    merkle::MerklizedTableColumn,
};
use fuel_core_storage::tables::BlobData;

impl MerklizedTableColumn for BlobData {
    fn table_column() -> TableColumn {
        TableColumn::Blobs
    }
}
