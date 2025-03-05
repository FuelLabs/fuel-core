use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    merkle::column::MerkleizedColumn,
    structured_storage::TableWithBlueprint,
};

use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
    tables::ProcessedTransactions,
};

impl MerkleizedTableColumn for ProcessedTransactions {
    type TableColumn = TableColumn;

    fn table_column() -> TableColumn {
        TableColumn::ProcessedTransactions
    }
}

impl TableWithBlueprint for ProcessedTransactions {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = MerkleizedColumn<TableColumn>;

    fn column() -> Self::Column {
        MerkleizedColumn::TableColumn(Self::table_column())
    }
}

#[cfg(test)]
fuel_core_storage::basic_storage_tests!(
    ProcessedTransactions,
    <ProcessedTransactions as fuel_core_storage::Mappable>::Key::from([1u8; 32]),
    <ProcessedTransactions as fuel_core_storage::Mappable>::Value::default()
);
