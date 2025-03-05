use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::raw::Raw,
    merkle::column::MerkleizedColumn,
    structured_storage::TableWithBlueprint,
};

use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
    tables::BlobData,
};

impl MerkleizedTableColumn for BlobData {
    type TableColumn = TableColumn;

    fn table_column() -> TableColumn {
        TableColumn::Blobs
    }
}

impl TableWithBlueprint for BlobData {
    type Blueprint = Plain<Raw, Raw>;
    type Column = MerkleizedColumn<TableColumn>;

    fn column() -> Self::Column {
        MerkleizedColumn::TableColumn(Self::table_column())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fuel_core_storage::basic_storage_tests!(
        BlobData,
        <BlobData as fuel_core_storage::Mappable>::Key::default(),
        vec![32u8],
        <BlobData as fuel_core_storage::Mappable>::OwnedValue::from(vec![32u8])
    );
}
