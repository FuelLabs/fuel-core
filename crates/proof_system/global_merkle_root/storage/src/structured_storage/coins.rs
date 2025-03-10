use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    merkle::column::MerkleizedColumn,
    structured_storage::TableWithBlueprint,
};

use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
    tables::Coins,
};

impl MerkleizedTableColumn for Coins {
    type TableColumn = TableColumn;

    fn table_column() -> TableColumn {
        TableColumn::Coins
    }
}

impl TableWithBlueprint for Coins {
    type Blueprint = Plain<Primitive<34>, Postcard>;
    type Column = MerkleizedColumn<TableColumn>;

    fn column() -> Self::Column {
        Self::Column::TableColumn(Self::table_column())
    }
}

#[cfg(test)]
fuel_core_storage::basic_storage_tests!(
    Coins,
    <Coins as fuel_core_storage::Mappable>::Key::default(),
    <Coins as fuel_core_storage::Mappable>::Value::default()
);
