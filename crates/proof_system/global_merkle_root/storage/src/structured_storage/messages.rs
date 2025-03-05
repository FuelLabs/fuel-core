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
    tables::Messages,
};

impl MerkleizedTableColumn for Messages {
    type TableColumn = TableColumn;

    fn table_column() -> TableColumn {
        TableColumn::Messages
    }
}

impl TableWithBlueprint for Messages {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = MerkleizedColumn<TableColumn>;

    fn column() -> Self::Column {
        MerkleizedColumn::TableColumn(Self::table_column())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fuel_core_storage::basic_storage_tests!(
        Messages,
        <Messages as fuel_core_storage::Mappable>::Key::default(),
        <Messages as fuel_core_storage::Mappable>::Value::default()
    );
}
