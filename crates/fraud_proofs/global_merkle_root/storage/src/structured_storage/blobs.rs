use crate::{
    column::TableColumns,
    merkle::MerklizedTableColumn,
};
use fuel_core_storage::tables::BlobData;

impl MerklizedTableColumn for BlobData {
    fn table_column() -> TableColumns {
        TableColumns::Blobs
    }
}
